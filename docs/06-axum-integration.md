# 06. 接入 Axum 和 JWT

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](05-integration-production.md) | [下一章](07-examples.md)

Web 集成的关键是分清认证和授权：

- 认证层解析请求，构造 `AuthSubject` 或 `PlatformSubject`。
- 授权层调用 `Engine` 或 `PlatformEngine`。
- 业务层负责加载目标对象，并把授权范围下推到查询。

## 启用 feature

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["axum"] }
```

如果需要内置 JWT 解析层：

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["axum-jwt"] }
```

`axum-jwt` 会启用 `axum` 和 `serde`。

## 手动注入 `AuthSubject`

你可以从 Session、JWT、网关注入头或自定义认证中间件里得到租户和主体，然后写入 request extensions。

```rust
use axum::{body::Body, http::Request, middleware::Next, response::Response};
use rs_tenant::{AuthSubject, PrincipalId, TenantId};

async fn inject_subject(mut req: Request<Body>, next: Next) -> Response {
    let tenant = TenantId::parse("tenant_a").expect("valid tenant id");
    let principal = PrincipalId::parse("user_1").expect("valid principal id");

    req.extensions_mut()
        .insert(AuthSubject::new(tenant, principal));

    next.run(req).await
}
```

## 租户级路由

租户设置、租户级配置、租户级报表等操作可以用 `can_tenant`。

```rust
use axum::{extract::Extension, http::StatusCode};
use rs_tenant::{AccessDecision, AuthSubject, Permission, TenantAccessRequest};

async fn update_tenant_settings(
    Extension(subject): Extension<AuthSubject>,
    Extension(engine): Extension<AppEngine>,
) -> Result<StatusCode, StatusCode> {
    let decision = engine
        .can_tenant(TenantAccessRequest {
            subject,
            permission: Permission::parse("tenant/settings:update")
                .map_err(|_| StatusCode::BAD_REQUEST)?,
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match decision {
        AccessDecision::Allow => Ok(StatusCode::NO_CONTENT),
        AccessDecision::Deny => Err(StatusCode::FORBIDDEN),
    }
}
```

## 对象级路由

对象级接口要先加载对象，再用对象真实路径判断。

```rust
use axum::{extract::Extension, http::StatusCode};
use rs_tenant::{AccessDecision, AuthSubject, Permission, ScopedAccessRequest};

async fn read_order(
    Extension(subject): Extension<AuthSubject>,
    Extension(engine): Extension<AppEngine>,
    order_id: OrderId,
) -> Result<Order, StatusCode> {
    let order = load_order(order_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let decision = engine
        .can_access_scope(ScopedAccessRequest {
            subject,
            permission: Permission::parse("order:read").map_err(|_| StatusCode::BAD_REQUEST)?,
            target: order.scope_path(),
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if decision == AccessDecision::Deny {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(order)
}
```

不要让客户端传 `scope_path` 来完成授权判断。

## 使用内置租户授权 Layer

`TenantAuthorizeLayer` 适合那些只需要租户级权限的路由。它从 request extensions 读取 `AuthSubject` 或 `AuthContext`。

```rust
use std::sync::Arc;

use axum::{Router, routing::post};
use rs_tenant::{Permission, axum::TenantAuthorizeLayer};

fn routes(engine: Arc<AppEngine>) -> Router {
    Router::new().route(
        "/tenant/settings",
        post(update_tenant_settings).layer(TenantAuthorizeLayer::new(
            engine,
            Permission::parse("tenant/settings:update").expect("valid permission"),
        )),
    )
}
```

访问具体业务对象时，通常仍建议在 handler 中调用 `can_access_scope`，因为 handler 才能加载对象的真实路径。

## JWT 集成

`axum-jwt` 提供默认 claims 和 layer。默认 claims 会提取租户主体上下文并写入 extensions。

```rust
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use rs_tenant::axum::jwt::{DefaultClaims, JwtAuthLayer, JwtAuthState};

let validation = Validation::new(Algorithm::HS256);
let state = JwtAuthState::<DefaultClaims>::new(
    DecodingKey::from_secret(b"replace-with-application-secret"),
    validation,
);
let layer = JwtAuthLayer::new(state);
```

如果你的 JWT 字段不同，实现自定义 claims/provider，把结果转换成 `AuthSubject`。

## HTTP 状态码建议

| 情况 | 状态码 |
|---|---|
| 没有认证信息或 token 无效 | `401 Unauthorized` |
| 认证通过但授权拒绝 | `403 Forbidden` |
| 业务对象不存在 | `404 Not Found` |
| 数据源或业务读取异常 | `500 Internal Server Error` |

不要把 `AuthorizationSource` 错误映射成 403。那是系统错误，不是用户没有权限。

## 平台路由

启用 `axum + platform` 后，可以使用 `PlatformAuthorizeLayer` 保护平台自身资源。

```rust
use std::sync::Arc;

use axum::{Router, routing::post};
use rs_tenant::{
    Permission,
    axum::PlatformAuthorizeLayer,
    platform::{PlatformAuthorizationSource, PlatformEngine},
};

fn platform_routes<S>(engine: Arc<PlatformEngine<S>>) -> Router
where
    S: PlatformAuthorizationSource + 'static,
{
    Router::new().route(
        "/platform/roles",
        post(create_platform_role).layer(PlatformAuthorizeLayer::new(
            engine,
            Permission::parse("platform/role:create").expect("valid permission"),
        )),
    )
}
```

这个 layer 只调用 `can_platform`，适合平台角色管理、平台权限配置、租户创建入口等平台自身资源。跨租户列表和导出仍应在 handler 中调用 `accessible_tenants`，再把结果下推到查询。

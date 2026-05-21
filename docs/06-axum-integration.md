# 06. Axum 与 JWT 集成

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](05-integration-production.md) | [下一章](07-examples.md)

v0.3.0 的 Web 集成只基于租户内上下文：从请求中得到 `AuthSubject`，再调用 `can_tenant`、`can_access_scope` 或 `accessible_scope`。

## 集成边界

Web 层可以负责：

- 从请求扩展、Session、JWT 或网关注入信息中提取 `tenant id`。
- 提取 `principal id`。
- 构造 `AuthSubject`。
- 在路由上调用租户级授权或范围级 helper。

内置 `TenantAuthorizeLayer` 会优先读取 request extension 里的 `AuthContext`，也接受直接注入的 `AuthSubject`。`axum-jwt` 中间件会同时写入这两个 extension，方便业务 handler 用 `Extension<AuthSubject>`。

Web 层不应该自动推断：

- 平台身份如何映射为租户内主体。
- super admin 是否绕过 membership。
- 业务对象的 `ScopePath`。

不同业务的目标路径通常来自数据库关系，而不是 URL 字符串本身。

## 手动注入 `AuthSubject`

```rust
use axum::{body::Body, http::Request, middleware::Next, response::Response};
use rs_tenant::{AuthSubject, PrincipalId, TenantId};

async fn inject_subject(mut req: Request<Body>, next: Next) -> Response {
    let tenant = TenantId::parse("tenant_a").expect("valid tenant");
    let principal = PrincipalId::parse("user_1").expect("valid principal");

    req.extensions_mut().insert(AuthSubject { tenant, principal });
    next.run(req).await
}
```

## 租户级路由

租户设置、租户级报表等无下级目标路径的接口，可以调用 `can_tenant`。

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

## 范围级路由

访问具体订单、客户、门店等资源时，应先从业务数据中得到目标 `ScopePath`，再调用 `can_access_scope`。

```rust
use rs_tenant::{AccessDecision, Permission, ScopedAccessRequest};

async fn read_order(
    Extension(subject): Extension<AuthSubject>,
    Extension(engine): Extension<AppEngine>,
    order_id: OrderId,
) -> Result<Order, StatusCode> {
    let order = load_order(order_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let target = order.scope_path();

    let decision = engine
        .can_access_scope(ScopedAccessRequest {
            subject,
            permission: Permission::parse("order:read").map_err(|_| StatusCode::BAD_REQUEST)?,
            target,
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if decision == AccessDecision::Deny {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(order)
}
```

## JWT 集成

JWT 解析层只应提取：

- `tenant id`
- `principal id`

解析后写入 `AuthSubject`。JWT 不负责生成平台授权，不负责解释 super admin，也不负责替业务对象推导范围。

## 状态码建议

- `401 Unauthorized`：没有认证信息，或 JWT 无效。
- `403 Forbidden`：认证通过，但 `rs-tenant` 返回拒绝。
- `404 Not Found`：业务对象不存在；是否隐藏存在性由应用策略决定。
- `500 Internal Server Error`：`AuthorizationSource` 或业务数据读取异常。

## 继续阅读

- [上一章：05. 生产环境集成指南](05-integration-production.md)
- [下一章：07. 典型案例](07-examples.md)
- [返回目录](SUMMARY.md)

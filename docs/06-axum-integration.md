# 06. Axum 与 JWT 集成

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](05-integration-production.md) | [下一章](07-examples.md)

本章给出两种接入方式：

1. 你已有鉴权体系：手动注入 `AuthContext`
2. 使用库内 JWT 中间件：`JwtAuthLayer`

## 1) 启用 feature

```toml
[dependencies]
rs-tenant = { version = "0.1", features = ["axum"] }
# 如果要用内置 JWT 解析：
# rs-tenant = { version = "0.1", features = ["axum-jwt"] }
```

## 2) 手动注入 AuthContext（不使用内置 JWT）

你可以在自定义中间件中把租户与主体写入请求扩展：

```rust
use axum::{middleware::Next, response::Response, http::Request, body::Body};
use rs_tenant::axum::AuthContext;
use rs_tenant::{PrincipalId, TenantId};

async fn inject_auth_context(mut req: Request<Body>, next: Next) -> Response {
    // 示例：真实项目中从网关头、Session 或外部鉴权服务读取
    let tenant = TenantId::try_from("tenant_a").unwrap();
    let principal = PrincipalId::try_from("user_1").unwrap();
    req.extensions_mut().insert(AuthContext { tenant, principal });
    next.run(req).await
}
```

路由上挂授权层：

```rust
use rs_tenant::axum::AuthorizeLayer;
use rs_tenant::Permission;
use std::sync::Arc;

let permission = Permission::try_from("invoice:read").unwrap();
let app = app.layer(AuthorizeLayer::new(Arc::clone(&engine), permission));
```

## 3) 使用 JwtAuthLayer（`axum-jwt`）

```rust
use jsonwebtoken::{DecodingKey, Validation, Algorithm};
use rs_tenant::axum::jwt::{DefaultClaims, JwtAuthLayer, JwtAuthState};
use rs_tenant::axum::AuthorizeLayer;
use rs_tenant::Permission;
use std::sync::Arc;

let mut validation = Validation::new(Algorithm::HS256);
validation.validate_exp = true;

let jwt_state = JwtAuthState::<DefaultClaims>::new(
    DecodingKey::from_secret(b"your-secret"),
    validation,
);

let app = app
    .layer(AuthorizeLayer::new(
        Arc::clone(&engine),
        Permission::try_from("invoice:read").unwrap(),
    ))
    .layer(JwtAuthLayer::new(jwt_state));
```

中间件顺序建议与上面一致：先声明 `AuthorizeLayer`，再声明 `JwtAuthLayer`，以保证请求进入时先完成 JWT 解码并注入 `AuthContext`。

## 4) 常见返回码说明

- `401 Unauthorized`：缺少或无效的认证上下文/JWT
- `403 Forbidden`：认证通过但无权限
- `500 Internal Server Error`：授权查询过程异常（如 Store 报错）

## 5) 调试建议

1. 先确认请求里是否有有效 `AuthContext`
2. 打印 `tenant/principal/permission` 三元组
3. 校验 feature 是否正确开启（`axum` / `axum-jwt`）
4. 对照第 03 章逐步排查短路点

## 继续阅读

- [上一页：05. 生产环境集成指南](05-integration-production.md)
- [下一页：07. 典型案例](07-examples.md)
- [返回目录](SUMMARY.md)

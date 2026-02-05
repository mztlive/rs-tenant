# rs-tenant 接入指南（DDD + Axum）

本文面向 **DDD 架构 + Axum 项目**，说明如何接入本库完成多租户 RBAC 授权。

## 1. 依赖与 Feature

在业务项目 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
rs-tenant = { path = "../rs-tenant", features = ["axum", "axum-jwt"] }
axum = "0.7"
tokio = { version = "1", features = ["full"] }
```

如果你想启用内存缓存：

```toml
rs-tenant = { path = "../rs-tenant", features = ["axum", "axum-jwt", "memory-cache"] }
```

## 2. DDD 分层接入方式

推荐按照以下分层接入：

1. **领域层（Domain）**：不依赖 rs-tenant，只保留业务实体与领域规则  
2. **应用层（Application）**：调用 `Engine` 做授权决策  
3. **基础设施层（Infrastructure）**：实现 Store，连接 DB/缓存  
4. **接口层（Presentation/Axum）**：解析 JWT，插入 AuthContext，使用中间件授权  

## 3. 基础设施层：实现 Store

你需要实现 `TenantStore`、`RoleStore`、`GlobalRoleStore`：

```rust
use async_trait::async_trait;
use rs_tenant::{
    GlobalRoleId, Permission, PrincipalId, RoleId, StoreError, TenantId,
    GlobalRoleStore, RoleStore, TenantStore,
};

pub struct AuthStore {
    // 放你的 DB 连接池
}

#[async_trait]
impl TenantStore for AuthStore {
    async fn tenant_active(&self, tenant: TenantId) -> Result<bool, StoreError> {
        // SELECT active FROM tenants WHERE id = tenant
        Ok(true)
    }

    async fn principal_active(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
    ) -> Result<bool, StoreError> {
        // SELECT active FROM principals WHERE tenant_id = tenant AND id = principal
        Ok(true)
    }
}

#[async_trait]
impl RoleStore for AuthStore {
    async fn principal_roles(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
    ) -> Result<Vec<RoleId>, StoreError> {
        Ok(vec![])
    }

    async fn role_permissions(
        &self,
        tenant: TenantId,
        role: RoleId,
    ) -> Result<Vec<Permission>, StoreError> {
        Ok(vec![])
    }

    async fn role_inherits(
        &self,
        tenant: TenantId,
        role: RoleId,
    ) -> Result<Vec<RoleId>, StoreError> {
        Ok(vec![])
    }
}

#[async_trait]
impl GlobalRoleStore for AuthStore {
    async fn global_roles(
        &self,
        principal: PrincipalId,
    ) -> Result<Vec<GlobalRoleId>, StoreError> {
        Ok(vec![])
    }

    async fn global_role_permissions(
        &self,
        role: GlobalRoleId,
    ) -> Result<Vec<Permission>, StoreError> {
        Ok(vec![])
    }
}
```

## 4. 应用层：构建 Engine

应用层建议持有一个 `Engine`：

```rust
use rs_tenant::{Engine, EngineBuilder, MemoryCache};
use std::sync::Arc;
use std::time::Duration;

pub fn build_engine(store: crate::infrastructure::AuthStore) -> Arc<Engine<_, MemoryCache>> {
    Arc::new(
        EngineBuilder::new(store)
            .enable_role_hierarchy(true)
            .enable_wildcard(true)
            .max_inherit_depth(16)
            .cache(MemoryCache::new(10_000).with_ttl(Duration::from_secs(30)))
            .build(),
    )
}
```

## 5. Axum 接入：JWT 解析 + AuthContext

### 5.1 定义 JWT Claims

默认 claims 可以直接使用 `DefaultClaims`，如果你有自定义字段：

```rust
#[derive(Clone, serde::Deserialize)]
pub struct MyClaims {
    pub tenant_id: String,
    pub principal_id: String,
    pub exp: usize,
}

impl rs_tenant::axum::jwt::JwtClaims for MyClaims {
    fn tenant_id(&self) -> &str { &self.tenant_id }
    fn principal_id(&self) -> &str { &self.principal_id }
}
```

### 5.2 把 JWT State 放入 AppState

```rust
use jsonwebtoken::{DecodingKey, Validation};
use rs_tenant::axum::jwt::JwtAuthState;

pub struct AppState {
    pub engine: std::sync::Arc<rs_tenant::Engine<crate::infrastructure::AuthStore, rs_tenant::MemoryCache>>,
    pub jwt: JwtAuthState<MyClaims>,
}
```

并在 `AppState` 上实现 `JwtAuthProvider`：

```rust
impl rs_tenant::axum::jwt::JwtAuthProvider<MyClaims> for AppState {
    fn jwt_auth(&self) -> &JwtAuthState<MyClaims> {
        &self.jwt
    }
}
```

### 5.3 使用 JwtAuthLayer 注入 AuthContext

```rust
use rs_tenant::axum::jwt::JwtAuthLayer;

let jwt_state = JwtAuthState::new(
    DecodingKey::from_secret(b"secret"),
    Validation::default(),
);

let app_state = AppState {
    engine,
    jwt: jwt_state.clone(),
};

let router = axum::Router::new()
    .route("/invoices", axum::routing::get(list_invoices))
    .layer(JwtAuthLayer::new(jwt_state))
    .with_state(app_state);
```

## 6. Axum 接入：权限中间件

每个路由可以挂 `AuthorizeLayer`：

```rust
use rs_tenant::axum::AuthorizeLayer;
use rs_tenant::Permission;

let permission = Permission::try_from("invoice:read").unwrap();
let router = axum::Router::new()
    .route("/invoices", axum::routing::get(list_invoices))
    .layer(AuthorizeLayer::new(engine.clone(), permission))
    .layer(JwtAuthLayer::new(jwt_state))
    .with_state(app_state);
```

**注意顺序**：使用 `.layer(AuthorizeLayer).layer(JwtAuthLayer)`，让 `JwtAuthLayer` 先执行，确保请求里已有 `AuthContext`。

## 7. Handler 中使用 Extractor

如果你需要在 handler 中拿到 context 或 claims：

```rust
use rs_tenant::axum::jwt::JwtAuth;

async fn list_invoices(
    auth: JwtAuth<MyClaims>,
) -> String {
    format!(
        "tenant={} principal={}",
        auth.context.tenant,
        auth.context.principal
    )
}
```

## 8. 外部系统需要做的事情（清单）

1. 在业务系统中添加 `rs-tenant` 依赖和 feature  
2. 在基础设施层实现 Store（连接 DB）  
3. 在应用层构建 Engine  
4. 在接口层配置 JWT 解码与 AuthContext  
5. 为路由挂载 `JwtAuthLayer` + `AuthorizeLayer`  
6. 在 handler 里按需使用 `JwtAuth` extractor  

## 9. 常见问题

- **我不需要 JWT？**  
  不启用 `axum-jwt`，只使用 `AuthorizeLayer`。你可以自己把 `AuthContext` 插入 request extensions。

- **权限串格式不符合默认规则怎么办？**  
  可以先在外部生成符合规则的 Permission；后续如需自定义校验器再扩展。

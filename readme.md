# rs-tenant

`rs-tenant` 是一个面向 Rust SaaS 服务的 RBAC 授权内核。它帮助应用回答两个实际问题：

1. 这个租户成员能不能执行某个动作？
2. 如果能，它可以查询哪些租户数据范围？

它不管理用户、租户、角色、数据库迁移或后台页面。你的应用负责这些业务数据；`rs-tenant` 通过你的 `AuthorizationSource` 读取授权数据，计算后返回 decision 或 scope，再由你把 scope 下推到查询层。

## 安装

生产数据源接入：

```toml
[dependencies]
rs-tenant = "0.4.0"
```

示例和本地测试使用内存数据源：

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["memory-store"] }
```

常用 feature：

| Feature | 什么时候启用 |
|---|---|
| `memory-store` | 使用 `MemorySource` 写示例或测试。 |
| `memory-cache` | 使用内置 `MemoryCache`。 |
| `serde` | 公共值对象需要 serde 支持。 |
| `axum` | 使用 Axum 授权 layer 和 helper。 |
| `axum-jwt` | 使用内置 JWT 解析 layer。 |
| `platform` | 需要平台员工授权和跨租户数据范围。 |

## 第一个租户内授权

下面的例子把 `invoice:read` 授给 `agent/1` 路径，然后判断用户能否读取子路径。

```rust
use futures::executor::block_on;
use rs_tenant::{
    AccessDecision, AuthSubject, EngineBuilder, GrantScope, MembershipStatus, MemorySource,
    Permission, PrincipalId, RoleId, ScopePath, ScopedAccessRequest, TenantId, TenantStatus,
};

fn main() -> rs_tenant::Result<()> {
    block_on(async {
        let tenant = TenantId::parse("tenant_demo")?;
        let principal = PrincipalId::parse("user_demo")?;
        let role = RoleId::parse("store_reader")?;
        let permission = Permission::parse("invoice:read")?;

        let source = MemorySource::new();
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            role.clone(),
            GrantScope::paths(vec![ScopePath::parse("agent/1")?])?,
        );
        source.add_role_permission(tenant.clone(), role, permission.clone());

        let engine = EngineBuilder::new(source).build();
        let decision = engine
            .can_access_scope(ScopedAccessRequest {
                subject: AuthSubject::new(tenant, principal),
                permission,
                target: ScopePath::parse("agent/1/store/9")?,
            })
            .await?;

        assert_eq!(decision, AccessDecision::Allow);
        Ok(())
    })
}
```

运行仓库示例：

```bash
cargo run --example quickstart --features memory-store
```

## 应该调用哪个 API

| 需求 | API |
|---|---|
| 列表、搜索、导出前计算可见范围 | `Engine::accessible_scope(ScopeQuery)` |
| 判断一个有真实归属路径的业务对象 | `Engine::can_access_scope(ScopedAccessRequest)` |
| 判断租户级操作 | `Engine::can_tenant(TenantAccessRequest)` |
| 排查拒绝原因 | `Engine::explain_access_scope(...)` 或 `Engine::explain_tenant(...)` |

`accessible_scope` 会返回：

- `AccessScope::None`：返回空列表或拒绝。
- `AccessScope::Tenant { tenant }`：查询该租户下所有数据。
- `AccessScope::Paths { tenant, roots }`：只查询这些路径根下面的数据。

## 生产接入

生产环境实现 `AuthorizationSource`，读取你的数据库或内部服务：

```rust
use async_trait::async_trait;
use rs_tenant::{
    AuthSubject, AuthorizationSource, GrantScope, MembershipStatus, Permission, RoleAssignment,
    RoleId, SourceError, TenantId, TenantStatus,
};

#[derive(Clone)]
struct DbAuthorizationSource;

#[async_trait]
impl AuthorizationSource for DbAuthorizationSource {
    async fn tenant_status(&self, tenant: &TenantId) -> Result<TenantStatus, SourceError> {
        let _ = tenant;
        Ok(TenantStatus::Active)
    }

    async fn membership_status(
        &self,
        subject: &AuthSubject,
    ) -> Result<MembershipStatus, SourceError> {
        let _ = subject;
        Ok(MembershipStatus::Active)
    }

    async fn role_assignments(
        &self,
        subject: &AuthSubject,
    ) -> Result<Vec<RoleAssignment>, SourceError> {
        let _ = subject;
        let role = RoleId::parse("reader").map_err(|err| Box::new(err) as SourceError)?;
        Ok(vec![RoleAssignment::new(role, GrantScope::tenant())])
    }

    async fn role_permissions(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> Result<Vec<Permission>, SourceError> {
        let _ = (tenant, role);
        let permission =
            Permission::parse("invoice:read").map_err(|err| Box::new(err) as SourceError)?;
        Ok(vec![permission])
    }

    async fn parent_roles(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> Result<Vec<RoleId>, SourceError> {
        let _ = (tenant, role);
        Ok(Vec::new())
    }
}
```

Source 只读取数据。不要在 Source 里做 wildcard 匹配、角色继承、范围合并或路径判断；这些规则由引擎负责。

## 平台授权

如果你的 SaaS 有平台客服、运营、内部管理员，需要访问平台后台资源或受控地访问跨租户数据，启用 `platform`：

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["platform"] }
```

平台授权使用独立的 `PlatformEngine` 和 `PlatformAuthorizationSource`。它不是 super admin 绕过，也不会把平台员工变成租户成员。

运行平台示例：

```bash
cargo run --example platform --features memory-store,platform
```

## 文档

从这里开始：

- [开发者使用手册](docs/README.md)
- [5 分钟跑通第一个授权](docs/04-quickstart.md)
- [接入生产数据源](docs/05-integration-production.md)
- [接入 Axum 和 JWT](docs/06-axum-integration.md)
- [平台授权](docs/11-platform-authorization.md)
- [FAQ 与故障排查](docs/09-faq-troubleshooting.md)

本地预览 mdBook：

```bash
mdbook serve
```

## 开发命令

```bash
cargo check
cargo test --offline
cargo test --offline --features memory-store,memory-cache,serde
cargo fmt --all
cargo clippy --all-targets --all-features -D warnings
```

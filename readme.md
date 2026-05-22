# rs-tenant

`rs-tenant` 是面向 Rust SaaS 系统的 RBAC 授权内核。v0.4.0 保留 v0.3.0 的租户内授权核心，并在可选 `platform` feature 下新增平台授权子域。

租户内授权回答一个问题：

> 某个主体在某个租户内，基于角色分配，是否拥有某个权限，以及该权限对应的数据范围是什么。

平台授权回答两个问题：

> 平台主体是否可以访问平台自身资源，以及它可以管理哪些租户数据范围。

默认策略是 deny-by-default：租户无效、成员无效、没有角色分配、没有匹配权限、没有显式范围或数据源读取失败，都不会被静默放行。

## v0.4.0 定位

适合：

- 租户内 RBAC 授权。
- 平台后台自身权限判定。
- 平台账号跨租户查询前的数据范围计算。
- 需要在查询前得到可访问数据范围的 SaaS 系统。
- 希望用强类型模型约束权限、角色、主体、范围的 Rust 服务。
- 希望把授权数据读取与授权规则计算分离的项目。

不适合：

- 通用策略语言或复杂 ABAC。
- super admin 绕过框架。
- Casbin 的完整替代品。
- ORM、迁移、用户/组织/租户后台管理系统。

平台能力不是全局绕过：v0.4.0 不恢复 `GlobalRole`，不引入 `SuperAdmin`，不把平台主体伪装成租户成员，也不让租户角色继承平台角色。

## 核心 API

租户内 API 继续沿用 v0.3.0 语义：

- `Engine::accessible_scope(ScopeQuery)`：查询某个权限对应的最终数据范围。
- `Engine::can_access_scope(ScopedAccessRequest)`：判断某个目标路径是否可访问。
- `Engine::can_tenant(TenantAccessRequest)`：判断是否拥有全租户范围的权限。
- `Engine::explain_*`：返回轻量解释结果，用于日志、测试和排查。

关键类型：

- 主体与请求：`AuthSubject`、`ScopeQuery`、`TenantAccessRequest`、`ScopedAccessRequest`
- 范围：`ScopePath`、`GrantScope`、`AccessScope`
- 角色：`RoleId`、`RoleAssignment`
- 决策：`AccessDecision`、`AuthorizationSource`
- 组装与内存实现：`EngineBuilder`、`MemorySource`、`MemoryCache`

启用 `platform` feature 后，平台 API 作为 sibling engine 暴露，不改变 `Engine` 的租户内含义：

- `PlatformEngine::can_platform(PlatformAccessRequest)`：判断平台自身资源权限。
- `PlatformEngine::accessible_tenants(TenantDataScopeQuery)`：查询平台主体可管理的租户数据范围。
- `PlatformEngine::can_access_tenant(TenantDataAccessRequest)`：判断是否拥有某个租户级数据访问权限。
- `PlatformEngine::can_access_tenant_scope(TenantScopedDataAccessRequest)`：判断是否可以访问某个租户下的目标路径。

关键平台类型：

- 主体与角色：`PlatformSubject`、`PlatformPrincipalId`、`PlatformRoleId`、`PlatformRoleAssignment`
- 范围：`PlatformGrantScope`、`TenantDataAccessScope`、`TenantSet`、`TenantScopeRoots`
- 数据源：`PlatformAuthorizationSource`

## 快速示例

租户内授权示例：

```rust
use rs_tenant::{
    AccessDecision, AuthSubject, EngineBuilder, GrantScope, MembershipStatus, MemorySource,
    Permission, PrincipalId, RoleId, ScopePath, ScopedAccessRequest, TenantId, TenantStatus,
};

async fn demo() -> rs_tenant::Result<()> {
    let source = MemorySource::new();

    let tenant = TenantId::parse("tenant_a")?;
    let principal = PrincipalId::parse("user_1")?;
    let subject = AuthSubject {
        tenant: tenant.clone(),
        principal: principal.clone(),
    };
    let role = RoleId::parse("store_order_reader")?;

    source.set_tenant_status(tenant.clone(), TenantStatus::Active);
    source.set_membership_status(
        tenant.clone(),
        principal.clone(),
        MembershipStatus::Active,
    );
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        role.clone(),
        GrantScope::paths(vec![ScopePath::parse("agent/123/store/456")?])?,
    );
    source.add_role_permission(tenant.clone(), role, Permission::parse("order:read")?);

    let engine = EngineBuilder::new(source).enable_wildcard(true).build();

    let decision = engine
        .can_access_scope(ScopedAccessRequest {
            subject,
            permission: Permission::parse("order:read")?,
            target: ScopePath::parse("agent/123/store/456/order/789")?,
        })
        .await?;

    assert_eq!(decision, AccessDecision::Allow);
    Ok(())
}
```

平台授权示例需要启用 `platform`，内存演示需要同时启用 `memory-store`：

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["memory-store", "platform"] }
```

```rust
use rs_tenant::{
    platform::{
        MemoryPlatformSource, PlatformEngineBuilder, PlatformGrantScope, PlatformPrincipalId,
        PlatformPrincipalStatus, PlatformRoleId, PlatformSubject, TenantDataAccessScope,
        TenantDataScopeQuery,
    },
    Permission, TenantId,
};

async fn platform_demo() -> rs_tenant::Result<()> {
    let source = MemoryPlatformSource::new();
    let subject = PlatformSubject {
        principal: PlatformPrincipalId::parse("ops_1")?,
    };
    let role = PlatformRoleId::parse("tenant_support")?;

    source.set_platform_principal_status(
        subject.principal.clone(),
        PlatformPrincipalStatus::Active,
    );
    source.add_platform_role_assignment(
        subject.principal.clone(),
        role.clone(),
        PlatformGrantScope::tenants(vec![TenantId::parse("tenant_a")?])?,
    );
    source.add_platform_role_permission(role, Permission::parse("tenant:read")?);

    let engine = PlatformEngineBuilder::new(source).build();
    let scope = engine
        .accessible_tenants(TenantDataScopeQuery {
            subject,
            permission: Permission::parse("tenant:read")?,
        })
        .await?;

    assert!(matches!(scope, TenantDataAccessScope::Tenants { .. }));
    Ok(())
}
```

查询列表接口优先使用 `accessible_scope`，由业务仓储把 `AccessScope` 转成 SQL、ORM 或搜索条件：

```rust
let scope = engine.accessible_scope(query).await?;

match scope {
    AccessScope::None => Ok(vec![]),
    AccessScope::Tenant { tenant } => repo.list_by_tenant(tenant).await,
    AccessScope::Paths { tenant, roots } => repo.list_by_scope_roots(tenant, roots).await,
}
```

## 已删除的 v0.2 兼容层

v0.3.0 是 breaking rewrite，不保留旧概念别名或兼容 feature：

- 删除旧 `authorize(...)`。
- 删除旧 `scope(...)`。
- 删除旧 `Scope`。
- 删除 `Store` / `TenantStore` / `RoleStore` / `GlobalRoleStore` / `ScopeStore`。
- 删除 `GlobalRoleId` / `GlobalRole`。
- 删除 `SuperAdminMode`、`enable_super_admin(...)`、`is_super_admin(...)`。
- 删除空的 `casbin` feature。
- 删除公开 unchecked constructor。

租户内管理员请用普通角色表达：授予 `*:*`，并通过 `GrantScope::Tenant` 分配。平台权限和跨租户数据范围应通过 v0.4.0 `platform` feature 显式建模；运维救援的审批、原因、过期时间和审计日志仍由应用层维护。

## 文档

完整中文文档见 `docs/`：

- [文档首页](docs/README.md)
- [01. 项目总览](docs/01-overview.md)
- [02. 领域模型与权限语义](docs/02-domain-model.md)
- [03. 授权流程详解](docs/03-authorization-flow.md)
- [04. 5 分钟快速接入](docs/04-quickstart.md)
- [05. 生产环境集成指南](docs/05-integration-production.md)
- [06. Axum 与 JWT 集成](docs/06-axum-integration.md)
- [07. 典型案例](docs/07-examples.md)
- [08. 测试与性能基准](docs/08-testing-benchmark.md)
- [09. FAQ 与故障排查](docs/09-faq-troubleshooting.md)
- [10. Casbin 边界](docs/10-rs-tenant-vs-casbin.md)
- [11. 平台授权](docs/11-platform-authorization.md)
- [v0.3 重构方案](docs/redesign-v0.3.md)
- [v0.4 平台授权设计方案](docs/redesign-v0.4.md)

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

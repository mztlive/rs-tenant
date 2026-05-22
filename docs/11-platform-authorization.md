# 11. 平台授权

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](10-rs-tenant-vs-casbin.md)

v0.4.0 在可选 `platform` feature 下新增平台授权子域。它与租户内 `Engine` 是 sibling 关系：共享基础值对象，但不共享主体、角色、数据源和授权结果语义。

## Feature 边界

启用方式：

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["platform"] }
```

如果需要内存演示或测试数据源，同时启用：

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["memory-store", "platform"] }
```

`platform` feature 暴露 `rs_tenant::platform` 模块；未启用时，普通租户内授权用户不需要理解平台模型。

## 与租户内授权的关系

共享：

- `Permission`
- `TenantId`
- `ScopePath`
- `ScopeRoots`
- `AccessDecision`
- `SourceError`
- wildcard 匹配规则

隔离：

- `AuthSubject` 与 `PlatformSubject` 隔离。
- `RoleId` 与 `PlatformRoleId` 隔离。
- `AuthorizationSource` 与 `PlatformAuthorizationSource` 隔离。
- `Engine` 与 `PlatformEngine` 隔离。
- `AccessScope` 与 `TenantDataAccessScope` 隔离。

因此，平台主体不会被伪装成租户成员，平台角色也不会成为租户角色的父角色。

## 非目标

v0.4.0 平台授权不做以下事情：

- 不恢复旧 `GlobalRole`。
- 不引入 `SuperAdmin` 或全局绕过开关。
- 不把平台主体伪装成租户成员。
- 不让租户角色继承平台角色。
- 不提供 ORM、迁移脚本、后台 CRUD 或审计落库。
- 不做通用 ABAC 或策略语言。
- 不在现有 `Engine` 里塞入跨租户分支。

平台授权只回答“能否访问”和“能访问哪些范围”。租户、角色、成员、业务数据的持久化和管理界面仍由应用层负责。

## 平台主体与角色

```rust
pub struct PlatformPrincipalId(String);

pub struct PlatformSubject {
    pub principal: PlatformPrincipalId,
}

pub enum PlatformPrincipalStatus {
    Active,
    Inactive,
}

pub struct PlatformRoleId(String);

pub struct PlatformRoleAssignment {
    pub role: PlatformRoleId,
    pub scope: PlatformGrantScope,
}
```

平台主体不携带 `TenantId`。它先通过平台角色获得平台授权；当它需要管理租户数据时，再由平台授权结果给出可访问的租户范围。

## 平台授权范围

```rust
pub enum PlatformGrantScope {
    Platform,
    AllTenants,
    Tenants(TenantSet),
    TenantPaths(TenantScopeRoots),
}
```

语义：

- `Platform`：只覆盖平台自身资源，例如平台角色管理、权限配置、租户创建入口。
- `AllTenants`：覆盖所有租户的数据管理范围。
- `Tenants(TenantSet)`：覆盖明确的租户集合。
- `TenantPaths(TenantScopeRoots)`：覆盖部分租户内的部分路径。

辅助类型：

```rust
pub struct TenantSet {
    tenants: Vec<TenantId>,
}

pub struct TenantScopedRoots {
    pub tenant: TenantId,
    pub roots: ScopeRoots,
}

pub struct TenantScopeRoots {
    entries: Vec<TenantScopedRoots>,
}
```

规则：

- `TenantSet` 必须非空并去重。
- `TenantScopeRoots` 必须非空。
- 同一租户下的 roots 复用 `ScopeRoots` 压缩规则。
- `AllTenants` 覆盖任意租户和任意租户路径。
- `Tenants` 覆盖租户级数据，但不表达租户内路径限制。
- `TenantPaths` 只覆盖给定租户下的给定路径及其子孙。
- 同一 permission 下不能混用 `Tenants` 和 `TenantPaths`。v0.4.0 的 `TenantDataAccessScope` 没有 mixed 结果形态；如果需要同时表达“某些租户全量 + 另一些租户路径”，应拆成不同 permission 或在应用层建模更细的业务权限。

## PlatformEngine

平台授权使用独立引擎：

```rust
pub struct PlatformEngine<S> {
    source: S,
    config: PlatformEngineConfig,
}

pub struct PlatformEngineBuilder<S> {
    source: S,
    config: PlatformEngineConfig,
}

pub struct PlatformEngineConfig {
    pub enable_role_hierarchy: bool,
    pub enable_wildcard: bool,
    pub max_role_depth: usize,
}
```

常用构建方式：

```rust
let platform_engine = PlatformEngineBuilder::new(source)
    .enable_role_hierarchy(true)
    .enable_wildcard(true)
    .max_role_depth(16)
    .build();
```

核心 API：

```rust
impl<S> PlatformEngine<S>
where
    S: PlatformAuthorizationSource,
{
    pub async fn can_platform(
        &self,
        request: PlatformAccessRequest,
    ) -> Result<AccessDecision>;

    pub async fn accessible_tenants(
        &self,
        query: TenantDataScopeQuery,
    ) -> Result<TenantDataAccessScope>;

    pub async fn can_access_tenant(
        &self,
        request: TenantDataAccessRequest,
    ) -> Result<AccessDecision>;

    pub async fn can_access_tenant_scope(
        &self,
        request: TenantScopedDataAccessRequest,
    ) -> Result<AccessDecision>;
}
```

`PlatformEngineConfig` 与 `EngineConfig` 语义相近，但独立建模，避免未来平台配置和租户配置互相牵制。

## 平台自身资源判定

```rust
pub struct PlatformAccessRequest {
    pub subject: PlatformSubject,
    pub permission: Permission,
}
```

`can_platform` 只允许命中 `PlatformGrantScope::Platform` 的授权。

适用场景：

- 平台角色管理。
- 平台权限配置。
- 租户创建入口。
- 平台级配置。

示例：

```rust
let decision = platform_engine
    .can_platform(PlatformAccessRequest {
        subject: platform_subject.clone(),
        permission: Permission::parse("platform/role:update")?,
    })
    .await?;
```

`AllTenants`、`Tenants`、`TenantPaths` 都不是平台自身资源权限。

## 租户数据范围查询

```rust
pub struct TenantDataScopeQuery {
    pub subject: PlatformSubject,
    pub permission: Permission,
}

pub enum TenantDataAccessScope {
    None,
    AllTenants,
    Tenants { tenants: Vec<TenantId> },
    TenantPaths { entries: Vec<TenantScopedRoots> },
}
```

适用场景：

- 平台查询租户列表。
- 平台导出租户数据。
- 平台查询跨租户业务数据。

调用方拿到 `TenantDataAccessScope` 后，由业务仓储下推到 SQL、ORM 或搜索条件：

```rust
let scope = platform_engine
    .accessible_tenants(TenantDataScopeQuery {
        subject: platform_subject.clone(),
        permission: Permission::parse("tenant:read")?,
    })
    .await?;

match scope {
    TenantDataAccessScope::None => Ok(vec![]),
    TenantDataAccessScope::AllTenants => tenant_repo.list_all().await,
    TenantDataAccessScope::Tenants { tenants } => tenant_repo.list_by_ids(tenants).await,
    TenantDataAccessScope::TenantPaths { entries } => {
        business_repo.list_by_tenant_roots(entries).await
    }
}
```

如果同一平台主体对同一 permission 同时命中 `Tenants` 和 `TenantPaths`，`accessible_tenants` 会返回错误，避免查询前范围结果丢失路径级授权或与点判定不一致。

## 指定租户判定

```rust
pub struct TenantDataAccessRequest {
    pub subject: PlatformSubject,
    pub permission: Permission,
    pub tenant: TenantId,
}
```

允许规则：

- `AllTenants`：允许。
- `Tenants` 包含目标租户：允许。
- `TenantPaths` 不应被当作租户级访问：拒绝，并返回目标路径必需。
- `None`：拒绝。

这个规则与租户内 `can_tenant` 类似：路径级授权不会被提升为租户级授权。

## 指定租户路径判定

```rust
pub struct TenantScopedDataAccessRequest {
    pub subject: PlatformSubject,
    pub permission: Permission,
    pub tenant: TenantId,
    pub target: ScopePath,
}
```

允许规则：

- `AllTenants`：允许。
- `Tenants` 包含目标租户：允许。
- `TenantPaths` 中目标租户的 roots 覆盖 target：允许。
- 其他情况拒绝。

示例：

```rust
let decision = platform_engine
    .can_access_tenant_scope(TenantScopedDataAccessRequest {
        subject: platform_subject,
        permission: Permission::parse("tenant/order:update")?,
        tenant: TenantId::parse("tenant_a")?,
        target: ScopePath::parse("agent/123/store/456/order/789")?,
    })
    .await?;
```

## PlatformAuthorizationSource

平台数据源只读授权数据：

```rust
#[async_trait]
pub trait PlatformAuthorizationSource: Send + Sync {
    async fn platform_principal_status(
        &self,
        subject: &PlatformSubject,
    ) -> std::result::Result<PlatformPrincipalStatus, SourceError>;

    async fn platform_role_assignments(
        &self,
        subject: &PlatformSubject,
    ) -> std::result::Result<Vec<PlatformRoleAssignment>, SourceError>;

    async fn platform_role_permissions(
        &self,
        role: &PlatformRoleId,
    ) -> std::result::Result<Vec<Permission>, SourceError>;

    async fn platform_parent_roles(
        &self,
        role: &PlatformRoleId,
    ) -> std::result::Result<Vec<PlatformRoleId>, SourceError>;
}
```

Source 不负责创建租户、更新平台角色、绑定平台主体、写审计日志或检查业务数据是否存在。角色继承展开、环检测、最大深度限制由 `PlatformEngine` 负责。

## memory-store + platform 示例

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["memory-store", "platform"] }
```

```rust
use rs_tenant::{
    platform::{
        MemoryPlatformSource, PlatformAccessRequest, PlatformEngineBuilder, PlatformGrantScope,
        PlatformPrincipalId, PlatformPrincipalStatus, PlatformRoleId, PlatformSubject,
        TenantDataAccessScope, TenantDataScopeQuery,
    },
    AccessDecision, Permission, TenantId,
};

async fn memory_platform_source_demo() -> rs_tenant::Result<()> {
    let source = MemoryPlatformSource::new();
    let subject = PlatformSubject {
        principal: PlatformPrincipalId::parse("ops_1")?,
    };
    let platform_admin = PlatformRoleId::parse("platform_admin")?;
    let tenant_support = PlatformRoleId::parse("tenant_support")?;

    source.set_platform_principal_status(
        subject.principal.clone(),
        PlatformPrincipalStatus::Active,
    );
    source.add_platform_role_assignment(
        subject.principal.clone(),
        platform_admin.clone(),
        PlatformGrantScope::Platform,
    );
    source.add_platform_role_assignment(
        subject.principal.clone(),
        tenant_support.clone(),
        PlatformGrantScope::tenants(vec![TenantId::parse("tenant_a")?])?,
    );
    source.add_platform_role_permission(
        platform_admin,
        Permission::parse("platform/role:update")?,
    );
    source.add_platform_role_permission(tenant_support, Permission::parse("tenant:read")?);

    let engine = PlatformEngineBuilder::new(source).build();

    let platform_decision = engine
        .can_platform(PlatformAccessRequest {
            subject: subject.clone(),
            permission: Permission::parse("platform/role:update")?,
        })
        .await?;
    assert_eq!(platform_decision, AccessDecision::Allow);

    let tenant_scope = engine
        .accessible_tenants(TenantDataScopeQuery {
            subject,
            permission: Permission::parse("tenant:read")?,
        })
        .await?;
    assert!(matches!(tenant_scope, TenantDataAccessScope::Tenants { .. }));

    Ok(())
}
```

这个例子里，`PlatformGrantScope::Platform` 只允许平台角色管理；`PlatformGrantScope::Tenants` 只允许查询指定租户数据。两者不会互相提升。

## 生产接入建议

推荐最小逻辑表：

- `platform_principals(id, status)`
- `platform_role_assignments(principal_id, role_id, scope_kind, tenants, tenant_scope_roots)`
- `platform_role_permissions(role_id, permission)`
- `platform_role_inherits(role_id, parent_role_id)`

租户内授权表仍然保留自己的 `tenant_id` 前导索引，不需要为了平台主体改造 `AuthSubject` 或 `RoleId`。

业务查询下推建议：

- `TenantDataAccessScope::None`：直接返回空列表。
- `TenantDataAccessScope::AllTenants`：允许全量租户查询，但仍应受业务分页、审计和操作权限约束。
- `TenantDataAccessScope::Tenants { tenants }`：转换为 `tenant_id IN (...)`。
- `TenantDataAccessScope::TenantPaths { entries }`：转换为按租户分组的 path roots 条件。

## 继续阅读

- [上一章：10. Casbin 边界](10-rs-tenant-vs-casbin.md)
- [v0.4 平台授权设计方案](redesign-v0.4.md)
- [回到文档首页](README.md)
- [返回目录](SUMMARY.md)

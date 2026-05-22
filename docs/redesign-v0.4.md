# rs-tenant v0.4 平台授权设计方案

> 状态：0.4.0 设计草案
> 目标版本：`0.4.0`
> 范围：平台主体、平台角色、平台自身权限、平台管理租户数据的授权范围
> 兼容策略：保留 v0.3 租户内授权核心语义，新增独立 `platform` feature

## 1. 背景

v0.3.0 已经把 `rs-tenant` 收敛为租户内 RBAC 授权内核：

```text
tenant + principal + permission -> access scope
```

这个模型适合租户内成员访问租户内数据，但平台系统还需要两类能力：

1. 平台自身对于 role 的需求，例如平台角色管理、权限配置、租户管理入口。
2. 平台对于租户数据的管理需求，例如平台账号可以查看哪些租户、管理哪些租户路径下的数据。

这两类能力不能简单等同于租户内管理员。租户内管理员仍然是某个租户的成员，并通过租户角色获得 `GrantScope::Tenant`。平台操作者不是天然属于所有租户的成员，也不应该绕过租户内授权流程。

## 2. 目标

v0.4.0 的目标是引入一个清晰的平台授权子域：

- 支持平台主体。
- 支持平台角色。
- 支持平台自身资源的权限判定。
- 支持平台账号管理租户数据时的可访问租户范围计算。
- 支持跨租户但仍受 `ScopePath` 约束的数据访问判定。
- 复用现有 `Permission`、`TenantId`、`ScopePath`、`AccessDecision` 等基础值对象。
- 保持现有 `Engine`、`AuthorizationSource`、`RoleAssignment` 语义稳定。

## 3. 非目标

v0.4.0 不做以下事情：

- 不恢复旧 `GlobalRole`。
- 不引入 `SuperAdmin` 或全局绕过开关。
- 不把平台主体伪装成租户成员。
- 不让租户角色继承平台角色。
- 不提供 ORM、迁移脚本、后台 CRUD 或审计落库。
- 不做通用 ABAC 或策略语言。
- 不在现有 `Engine` 里塞入跨租户分支。

平台授权负责回答“能否访问”和“能访问哪些范围”。租户、角色、成员、业务数据的持久化和管理界面仍由应用层负责。

## 4. 为什么不直接扩展当前 core

当前 core 的不变量很强：

- `AuthSubject` 必须包含 `TenantId` 和 `PrincipalId`。
- 授权前一定检查 `tenant_status`。
- 授权前一定检查该主体在当前租户下的 `membership_status`。
- cache key 以 `tenant + principal + config` 为核心。
- `AccessScope::Tenant` 的含义是“当前租户内全量”，不是“所有租户”。

如果直接扩展当前 core，会很快变成混合模型：

```rust
enum Subject {
    Tenant(AuthSubject),
    Platform(PlatformSubject),
}

enum AccessScope {
    None,
    Tenant { tenant: TenantId },
    Paths { tenant: TenantId, roots: Vec<ScopePath> },
    Platform,
    AllTenants,
    Tenants(Vec<TenantId>),
    TenantPaths(Vec<TenantScopedRoots>),
}
```

这会造成几个问题：

- `can_tenant` 会同时承担“租户内全量权限”和“平台管理某个租户”的语义。
- `membership_status` 对平台主体没有自然含义。
- `RoleId` 会变得既像租户角色又像平台角色。
- `AccessScope::Tenant` 容易被误读成平台可管理租户。
- 普通租户内授权调用方需要理解平台分支，误用风险增加。

因此 v0.4.0 应该共享基础值对象，而不是共享业务语义入口。现有 `Engine` 继续保持租户内授权；平台能力由 sibling module 承担。

## 5. 模块边界

建议新增 `platform` feature：

```toml
[features]
platform = []
```

模块结构建议：

```text
src/
  platform/
    mod.rs
    ids.rs
    subject.rs
    role.rs
    scope.rs
    request.rs
    decision.rs
    source.rs
    engine.rs
    memory_source.rs   # 仅在 memory-store + platform 下启用
```

公开导出建议：

```rust
#[cfg(feature = "platform")]
pub mod platform;
```

现有模块保持不变：

- `Engine`：租户内授权。
- `AuthorizationSource`：租户内授权数据读取。
- `AuthSubject`：租户内主体。
- `RoleId`：租户内角色。
- `RoleAssignment`：租户内角色分配。
- `AccessScope`：单租户授权结果。

## 6. 平台领域模型

### 6.1 平台主体

```rust
pub struct PlatformPrincipalId(String);

pub struct PlatformSubject {
    pub principal: PlatformPrincipalId,
}

pub enum PlatformPrincipalStatus {
    Active,
    Inactive,
}
```

平台主体不携带 `TenantId`。它先通过平台角色得到平台授权，再在需要管理租户数据时得到可访问租户范围。

### 6.2 平台角色

```rust
pub struct PlatformRoleId(String);

pub struct PlatformRoleAssignment {
    pub role: PlatformRoleId,
    pub scope: PlatformGrantScope,
}
```

平台角色只服务平台主体。它不作为 `RoleId` 的父角色，也不参与租户内 `Engine` 的角色继承。

### 6.3 平台授权范围

```rust
pub enum PlatformGrantScope {
    Platform,
    AllTenants,
    Tenants(TenantSet),
    TenantPaths(TenantScopeRoots),
}
```

语义：

- `Platform`：只覆盖平台自身资源，例如平台角色管理、租户创建入口。
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

## 7. 平台请求与结果

### 7.1 平台自身资源判定

```rust
pub struct PlatformAccessRequest {
    pub subject: PlatformSubject,
    pub permission: Permission,
}
```

只允许命中 `PlatformGrantScope::Platform` 的授权。

适用场景：

- 平台角色管理。
- 平台权限配置。
- 租户创建入口。
- 平台级配置。

### 7.2 租户数据范围查询

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

调用方拿到 `TenantDataAccessScope` 后，由业务仓储下推到 SQL、ORM 或搜索条件。

### 7.3 指定租户判定

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

### 7.4 指定租户路径判定

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

## 8. PlatformEngine API

建议新增独立引擎：

```rust
pub struct PlatformEngine<S> {
    source: S,
    config: PlatformEngineConfig,
}

pub struct PlatformEngineConfig {
    pub enable_role_hierarchy: bool,
    pub enable_wildcard: bool,
    pub max_role_depth: usize,
}
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

`PlatformEngineConfig` 可以复用 v0.3 的设计语义，但不要直接复用 `EngineConfig`，避免后续平台配置和租户配置互相牵制。

## 9. PlatformAuthorizationSource

新增只读 source：

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

边界规则：

- Source 只读取授权数据。
- Source 不负责创建租户、更新角色、绑定成员或写审计日志。
- Source 不检查业务数据是否存在。
- 角色继承展开、环检测、最大深度限制由 `PlatformEngine` 负责。

## 10. 与租户内授权的关系

平台授权和租户授权是 sibling 关系，不是父子关系。

共享：

- `Permission`
- `TenantId`
- `ScopePath`
- `ScopeRoots`
- `AccessDecision`
- `SourceError`
- wildcard 匹配规则
- 角色继承展开算法可以抽为 crate-private helper

隔离：

- `AuthSubject` 与 `PlatformSubject` 隔离。
- `RoleId` 与 `PlatformRoleId` 隔离。
- `AuthorizationSource` 与 `PlatformAuthorizationSource` 隔离。
- `Engine` 与 `PlatformEngine` 隔离。
- `AccessScope` 与 `TenantDataAccessScope` 隔离。

这样可以避免平台能力改变普通租户内授权的解释性。

## 11. Role Template 的处理

平台可能还需要维护“租户默认角色模板”，但它不应该在 0.4.0 MVP 中直接进入判定内核。

建议未来单独建模：

```rust
pub struct RoleTemplateId(String);
```

模板语义：

- 平台维护模板。
- 应用层把模板物化为租户内 `RoleId` 和权限集合。
- 租户内授权仍然只读取物化后的租户角色。
- 模板不直接出现在 `Engine` 或 `PlatformEngine` 的授权结果里。

这样可以避免把“配置来源”和“运行时授权判定”混在一起。

## 12. 使用示例

平台自身权限：

```rust
let allowed = platform_engine
    .can_platform(PlatformAccessRequest {
        subject: platform_subject.clone(),
        permission: Permission::parse("platform/role:update")?,
    })
    .await?;
```

平台查询可管理租户：

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

平台访问某个租户路径对象：

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

## 13. 实施阶段

### 阶段一：文档与 public API 草案

- 新增本文档。
- 确认命名：`PlatformEngine`、`PlatformSubject`、`PlatformGrantScope`、`TenantDataAccessScope`。
- 确认 `platform` feature 边界。
- 补充 README 中对 0.4.0 的定位说明。

### 阶段二：领域类型

- 新增平台 ID 类型。
- 新增平台主体类型。
- 新增平台角色分配类型。
- 新增平台 scope 类型。
- 新增 serde 支持，沿用构造校验，不允许反序列化绕过不变量。

### 阶段三：平台授权引擎

- 实现 `PlatformAuthorizationSource`。
- 实现 `PlatformEngine`。
- 实现平台角色继承、环检测、深度限制。
- 实现平台自身权限判定。
- 实现租户数据范围合并。

### 阶段四：内存实现与示例

- 在 `memory-store + platform` 下提供 `MemoryPlatformSource`。
- 增加 README 和 docs 示例。
- 增加典型用例测试。

### 阶段五：可选适配

- 在 `axum + platform` 下提供平台权限中间件。
- 评估是否需要独立 `PlatformCache`。
- 如果引入缓存，cache key 应以 `platform principal + config` 为核心，不复用租户 cache key。

当前实现状态：`axum + platform` 已提供 `PlatformAuthorizeLayer` 和 `PlatformAuthContext`，用于平台自身资源的 `can_platform` 判定。v0.4.0 暂不引入 `PlatformCache`；未来如需缓存，应新增独立平台缓存抽象，不复用租户内 cache key。

## 14. 测试清单

必须覆盖：

- 平台主体 inactive 时拒绝。
- 无平台角色分配时拒绝。
- 平台角色拥有 `Platform` scope 时可以访问平台资源。
- `Platform` scope 不能访问租户数据。
- `AllTenants` 可以访问任意租户数据。
- `Tenants([a])` 可以访问 tenant a，拒绝 tenant b。
- `TenantPaths` 可以访问目标路径子孙，拒绝兄弟路径。
- `TenantPaths` 不能被 `can_access_tenant` 当成租户级权限。
- 多个平台角色分配可以合并租户范围。
- 同一租户下的路径 roots 会被压缩。
- 平台角色继承支持父角色权限。
- 平台角色继承能检测 cycle。
- 平台角色继承能限制最大深度。
- wildcard 仍然受 `enable_wildcard` 控制。
- 租户 `Engine` 的现有测试不需要为了平台 feature 改语义。

## 15. 兼容性策略

v0.4.0 应尽量保持 v0.3 public API 不破坏：

- 不改 `AuthSubject`。
- 不改 `RoleId`。
- 不改 `RoleAssignment`。
- 不改 `GrantScope`。
- 不改 `AccessScope`。
- 不改 `AuthorizationSource`。
- 不改现有 `Engine` 方法含义。

新增能力通过 `platform` feature 暴露。普通租户内授权用户不启用 `platform` feature 时，不应感知平台模型。

如果实现过程中必须抽公共 helper，优先放在 crate-private 模块中，不暴露过早抽象。

## 16. 结论

0.4.0 应该扩展 `rs-tenant` 的能力边界，但不要扩散现有租户内 core 的语义边界。

推荐架构是：

```text
基础值对象：Permission / TenantId / ScopePath / ScopeRoots
        |
        +-- 租户内授权：Engine / AuthSubject / AuthorizationSource / AccessScope
        |
        +-- 平台授权：PlatformEngine / PlatformSubject / PlatformAuthorizationSource / TenantDataAccessScope
```

这样既能支持平台自身 role 和平台管理租户数据，又能保留 v0.3 的 deny-by-default、强类型、难误用的授权内核。

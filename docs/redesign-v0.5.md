# rs-tenant v0.5 Store 与 IAM Service 设计方案

> 状态：0.5.0 设计草案
> 目标版本：`0.5.0`
> 范围：统一存储 trait、角色/绑定/权限管理 service、租户与平台 source 适配、缓存失效编排
> 兼容策略：保留 v0.4 core 和 platform 语义，新增高层管理模块，不破坏现有判定 API

## 1. 背景

当前 `rs-tenant` 已经可以稳定回答两类问题：

```text
tenant + principal + permission -> decision / access scope
platform principal + permission -> platform decision / tenant data scope
```

但业务项目接入时仍然需要自己实现大量 IAM 周边代码：

- 角色怎么创建、编辑、删除。
- 用户和角色怎么绑定。
- 角色权限怎么保存和读取。
- 角色继承关系怎么维护。
- 写入后怎么失效授权缓存。
- profile 或角色编辑页怎么读取当前权限。
- `AuthorizationSource` 和管理侧 repository 怎么避免重复实现。

这导致接入项目虽然复用了判定内核，但没有明显减少角色管理和绑定管理代码。

v0.5 的目标是补上最关键的一层：让接入方实现官方 Store trait，库负责提供通用管理 service，并把同一个 Store 适配给授权判定链路。

其中租户侧和平台侧继续沿用 v0.4 的 sibling 边界：

- 租户侧：`TenantAuthStore` -> `TenantIamService` -> `AuthorizationSource` / `Engine`。
- 平台侧：`PlatformAuthStore` -> `PlatformIamService` -> `PlatformAuthorizationSource` / `PlatformEngine`。
- 两边共享基础值对象和部分校验模式，但不共享主体、角色、scope、cache key 或写入 service。

## 2. 目标

v0.5 需要让使用方的默认接入路径变成：

1. 实现 `TenantAuthStore`。
2. 用 `TenantIamService` 做角色、绑定、权限管理。
3. 用 `TenantAuthSource` 或直接由 service 暴露的 source 接入 `Engine`。
4. 在业务接口中继续使用 `Engine` 的 `can_tenant`、`can_access_scope`、`accessible_scope`。

平台侧默认接入路径变成：

1. 实现 `PlatformAuthStore`。
2. 用 `PlatformIamService` 做平台角色、平台绑定、平台权限管理。
3. 用 `PlatformAuthSource` 或直接由 service 暴露的 source 接入 `PlatformEngine`。
4. 在平台后台接口中继续使用 `PlatformEngine` 的 `can_platform`、`accessible_tenants`、`can_access_tenant`、`can_access_tenant_scope`。

核心目标：

- 提供租户内 IAM 管理的标准数据结构。
- 提供角色 CRUD、角色权限维护、角色绑定维护、角色继承维护的 service。
- 写入后统一执行缓存失效。
- 复用现有 `RoleId`、`PrincipalId`、`TenantId`、`Permission`、`GrantScope`、`RoleAssignment`。
- 让 `AuthorizationSource` 变成 Store 的只读视图，而不是接入方额外手写一套读取逻辑。
- 提供平台 IAM 管理的并行数据结构，复用 `PlatformRoleId`、`PlatformPrincipalId`、`PlatformGrantScope`、`PlatformRoleAssignment`。
- 让 `PlatformAuthorizationSource` 也能由平台 Store 只读适配得到。

## 3. 非目标

v0.5 不做以下事情：

- 不绑定具体数据库、ORM 或迁移工具。
- 不提供完整后台页面。
- 不定义用户、员工、代理商、门店等业务身份模型。
- 不替应用决定 `AccountKind -> PrincipalId` 的映射。
- 不处理业务数据归属过滤，例如设备、销售单、耗材归属。
- 不引入动态策略语言或 Casbin policy 模型。
- 不把平台授权和租户授权混成一个 Store。
- 不把平台主体伪装成租户成员。
- 不在 `TenantIamService` 中增加平台分支，也不在 `PlatformIamService` 中管理租户内 membership。

## 4. 模块边界

建议新增 feature：

```toml
[features]
iam = []
```

模块结构：

```text
src/
  iam/
    mod.rs
    common/
      role_record.rs
      input.rs
      permission_set.rs
      assignment_set.rs
      validation.rs
      cache_invalidation.rs
      service_support.rs
      error.rs
    tenant/
      mod.rs
      store.rs
      service.rs
      source.rs
      api.rs
  platform/
    iam/
      mod.rs
      store.rs
      service.rs
      source.rs
      api.rs
```

公开导出：

```rust
#[cfg(feature = "iam")]
pub mod iam;
```

现有 core 继续保持：

- `Engine`：只负责判定。
- `AuthorizationSource`：只读授权数据源。
- `MemorySource`：测试和示例数据源。
- `RoleAssignment`：判定时使用的角色分配。
- `GrantScope`：分配级授权范围。

v0.5 的 `iam` 是 core 之上的应用层能力。

`platform/iam` 是 `platform` feature 之上的应用层能力。它可以由组合 feature 暴露：

```toml
[features]
iam = []
platform-iam = ["platform", "iam"]
```

如果项目只需要租户内授权，不启用 `platform-iam` 时不应感知平台 Store 或平台 service。

### 4.1 复用与抽象边界

租户侧和平台侧不应该复制两套完整实现。它们的 public API 要保持语义隔离，但实现层应抽公共支撑：

| 可复用层 | 说明 |
|---|---|
| `iam::common::role_record` | 泛型角色元数据，例如 `RoleRecordBase<R>`，承载 `id/name/description/system/disabled` |
| `iam::common::input` | 名称 trim、空描述归一、系统角色保护、更新字段归一 |
| `iam::common::permission_set` | permission 去重、排序稳定、unknown/deprecated 策略挂钩 |
| `iam::common::assignment_set` | assignment 去重、空列表语义、引用角色存在性校验的公共流程 |
| `iam::common::validation` | 通用校验 helper，例如非空名称、重复父角色、禁止自继承 |
| `iam::common::cache_invalidation` | 缓存失效动作建模，但 tenant cache 和 platform cache 使用不同 key |
| `iam::common::service_support` | CRUD 编排、写入后失效、错误映射的模板函数 |
| 既有 crate-private helper | 继续复用 `role_hierarchy::expand_roles`、`grant::ScopedGrant`、`ScopeRoots`、`Permission::matches` |

不建议抽成统一 public trait：

```rust
pub trait GenericIamStore<Subject, Role, Scope> { ... }
```

原因是这个 trait 会把 `AuthSubject` 和 `PlatformSubject`、`GrantScope` 和 `PlatformGrantScope`、`AccessScope` 和 `TenantDataAccessScope` 混成一组泛型参数。类型看似复用，语义反而更难读，错误也会从“用错入口”变成“泛型约束报错”。

推荐做法是：

- public API 保持 `TenantAuthStore`、`PlatformAuthStore`、`TenantIamService`、`PlatformIamService`。
- 内部实现使用 `iam::common::*` 的泛型 helper。
- `tenant::store`、`platform::iam::store` 只定义业务语义不同的读写 trait。
- `tenant::service`、`platform::iam::service` 只负责把各自 public 类型转换到 common helper，再调用对应 engine。

这样可以复用 60%-70% 的管理层代码，同时保留 v0.4 定下的“平台和租户是 sibling，不是同一个上下文”的边界。

## 5. 领域模型

### 5.1 RoleRecord

```rust
pub struct RoleRecord {
    pub tenant: TenantId,
    pub id: RoleId,
    pub name: String,
    pub description: Option<String>,
    pub system: bool,
    pub disabled: bool,
}
```

语义：

- `system` 表示系统内置或模板物化角色，默认不允许随意删除。
- `disabled` 表示角色存在但不参与授权。
- `name` 面向管理界面展示，不参与权限判定。

### 5.2 RoleInput

```rust
pub struct CreateRoleInput {
    pub tenant: TenantId,
    pub id: RoleId,
    pub name: String,
    pub description: Option<String>,
    pub system: bool,
}

pub struct UpdateRoleInput {
    pub tenant: TenantId,
    pub id: RoleId,
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub disabled: Option<bool>,
}
```

规则：

- `name` 由库统一 trim，不能为空。
- `description` 由库统一 trim，空字符串归一为 `None`。
- `RoleId` 继续使用现有构造校验。
- 默认不允许删除 `system = true` 的角色，可通过 service 配置覆盖。

### 5.3 AssignmentInput

```rust
pub struct SetRoleAssignmentsInput {
    pub subject: AuthSubject,
    pub assignments: Vec<RoleAssignment>,
}
```

规则：

- assignment 不能为空时必须引用存在且未禁用的角色。
- 同一 subject 下重复 role + scope 应归并。
- `GrantScope::paths` 的非空和压缩继续由 core 类型保证。

### 5.4 RolePermissionInput

```rust
pub struct SetRolePermissionsInput {
    pub tenant: TenantId,
    pub role: RoleId,
    pub permissions: Vec<Permission>,
}
```

规则：

- 权限去重。
- 可选接入 v0.6 的 `PermissionCatalog`，拒绝未登记权限。
- 写入空权限表示角色存在但没有授权。

## 6. Store Trait

v0.5 的核心是一个接入方实现的 Store：

```rust
#[async_trait]
pub trait TenantAuthStore: Send + Sync {
    async fn tenant_status(&self, tenant: &TenantId) -> Result<TenantStatus, StoreError>;

    async fn membership_status(
        &self,
        subject: &AuthSubject,
    ) -> Result<MembershipStatus, StoreError>;

    async fn roles(&self, tenant: &TenantId) -> Result<Vec<RoleRecord>, StoreError>;

    async fn role(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> Result<Option<RoleRecord>, StoreError>;

    async fn create_role(&self, input: CreateRoleInput) -> Result<RoleRecord, StoreError>;

    async fn update_role(&self, input: UpdateRoleInput) -> Result<RoleRecord, StoreError>;

    async fn delete_role(&self, tenant: &TenantId, role: &RoleId) -> Result<(), StoreError>;

    async fn role_assignments(
        &self,
        subject: &AuthSubject,
    ) -> Result<Vec<RoleAssignment>, StoreError>;

    async fn set_role_assignments(
        &self,
        input: SetRoleAssignmentsInput,
    ) -> Result<(), StoreError>;

    async fn role_permissions(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> Result<Vec<Permission>, StoreError>;

    async fn set_role_permissions(
        &self,
        input: SetRolePermissionsInput,
    ) -> Result<(), StoreError>;

    async fn parent_roles(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> Result<Vec<RoleId>, StoreError>;

    async fn set_parent_roles(
        &self,
        tenant: &TenantId,
        role: &RoleId,
        parents: Vec<RoleId>,
    ) -> Result<(), StoreError>;
}
```

这个 trait 是“最小生产边界”，不暴露数据库事务、连接池或 ORM 类型。具体项目可以在实现内部使用 SQL、Mongo、Redis 或内部服务。

## 7. AuthorizationSource 适配

提供一个只读适配器：

```rust
pub struct TenantAuthSource<S> {
    store: S,
}
```

实现：

```rust
#[async_trait]
impl<S> AuthorizationSource for TenantAuthSource<S>
where
    S: TenantAuthStore,
{
    // 委托到 store 的只读方法
}
```

处理规则：

- `StoreError` 转成 `SourceError`。
- disabled role 不应参与 `role_assignments` 或 `role_permissions`。
- 角色是否 disabled 可以由 Store 实现层过滤，也可以由 adapter 查询 role 后过滤；优先推荐 Store 层直接过滤，避免 N+1。

## 8. TenantIamService

Service 是应用推荐使用的入口：

```rust
pub struct TenantIamService<S, C = NoCache> {
    store: S,
    engine: Engine<TenantAuthSource<S>, C>,
}
```

核心方法：

```rust
impl<S, C> TenantIamService<S, C>
where
    S: TenantAuthStore,
    C: Cache,
{
    pub async fn create_role(&self, input: CreateRoleInput) -> Result<RoleRecord>;
    pub async fn update_role(&self, input: UpdateRoleInput) -> Result<RoleRecord>;
    pub async fn delete_role(&self, tenant: &TenantId, role: &RoleId) -> Result<()>;

    pub async fn assign_roles(&self, input: SetRoleAssignmentsInput) -> Result<()>;
    pub async fn role_ids(&self, subject: &AuthSubject) -> Result<Vec<RoleId>>;
    pub async fn assignments(&self, subject: &AuthSubject) -> Result<Vec<RoleAssignment>>;

    pub async fn set_role_permissions(&self, input: SetRolePermissionsInput) -> Result<()>;
    pub async fn permissions(&self, subject: &AuthSubject) -> Result<Vec<Permission>>;

    pub async fn accessible_scope(&self, query: ScopeQuery) -> Result<AccessScope>;
    pub async fn can_tenant(&self, request: TenantAccessRequest) -> Result<AccessDecision>;
    pub async fn can_access_scope(&self, request: ScopedAccessRequest) -> Result<AccessDecision>;
}
```

写入后的缓存失效：

| 操作 | 失效策略 |
|---|---|
| `assign_roles(subject, ...)` | `invalidate_principal(subject.tenant, subject.principal)` |
| `set_role_permissions(tenant, role, ...)` | `invalidate_role(tenant, role)` |
| `set_parent_roles(tenant, role, ...)` | `invalidate_tenant(tenant)`，因为子角色影响范围不一定可快速求出 |
| `update_role(disabled = true)` | `invalidate_role(tenant, role)` |
| `delete_role` | `invalidate_role(tenant, role)` |
| `tenant_status` 变更 | 由应用调用 `invalidate_tenant` |
| `membership_status` 变更 | 由应用调用 `invalidate_principal` |

## 9. 平台侧 Store 与 Service

平台侧能力不能复用 `TenantAuthStore`，因为平台主体没有 `TenantId`，平台授权结果也不是 `AccessScope`。v0.5 需要提供并行 Store：

```rust
#[async_trait]
pub trait PlatformAuthStore: Send + Sync {
    async fn platform_principal_status(
        &self,
        subject: &PlatformSubject,
    ) -> Result<PlatformPrincipalStatus, PlatformStoreError>;

    async fn platform_roles(&self) -> Result<Vec<PlatformRoleRecord>, PlatformStoreError>;

    async fn platform_role(
        &self,
        role: &PlatformRoleId,
    ) -> Result<Option<PlatformRoleRecord>, PlatformStoreError>;

    async fn create_platform_role(
        &self,
        input: CreatePlatformRoleInput,
    ) -> Result<PlatformRoleRecord, PlatformStoreError>;

    async fn update_platform_role(
        &self,
        input: UpdatePlatformRoleInput,
    ) -> Result<PlatformRoleRecord, PlatformStoreError>;

    async fn delete_platform_role(
        &self,
        role: &PlatformRoleId,
    ) -> Result<(), PlatformStoreError>;

    async fn platform_role_assignments(
        &self,
        subject: &PlatformSubject,
    ) -> Result<Vec<PlatformRoleAssignment>, PlatformStoreError>;

    async fn set_platform_role_assignments(
        &self,
        input: SetPlatformRoleAssignmentsInput,
    ) -> Result<(), PlatformStoreError>;

    async fn platform_role_permissions(
        &self,
        role: &PlatformRoleId,
    ) -> Result<Vec<Permission>, PlatformStoreError>;

    async fn set_platform_role_permissions(
        &self,
        input: SetPlatformRolePermissionsInput,
    ) -> Result<(), PlatformStoreError>;

    async fn platform_parent_roles(
        &self,
        role: &PlatformRoleId,
    ) -> Result<Vec<PlatformRoleId>, PlatformStoreError>;

    async fn set_platform_parent_roles(
        &self,
        role: &PlatformRoleId,
        parents: Vec<PlatformRoleId>,
    ) -> Result<(), PlatformStoreError>;
}
```

平台角色记录建议与租户角色保持形状相近，但使用平台 ID：

```rust
pub struct PlatformRoleRecord {
    pub id: PlatformRoleId,
    pub name: String,
    pub description: Option<String>,
    pub system: bool,
    pub disabled: bool,
}
```

平台 source 适配器：

```rust
pub struct PlatformAuthSource<S> {
    store: S,
}

#[async_trait]
impl<S> PlatformAuthorizationSource for PlatformAuthSource<S>
where
    S: PlatformAuthStore,
{
    // 委托到 store 的只读方法
}
```

平台 service：

```rust
pub struct PlatformIamService<S> {
    store: S,
    engine: PlatformEngine<PlatformAuthSource<S>>,
}
```

核心方法：

```rust
impl<S> PlatformIamService<S>
where
    S: PlatformAuthStore,
{
    pub async fn create_role(&self, input: CreatePlatformRoleInput) -> Result<PlatformRoleRecord>;
    pub async fn update_role(&self, input: UpdatePlatformRoleInput) -> Result<PlatformRoleRecord>;
    pub async fn delete_role(&self, role: &PlatformRoleId) -> Result<()>;

    pub async fn assign_roles(&self, input: SetPlatformRoleAssignmentsInput) -> Result<()>;
    pub async fn role_ids(&self, subject: &PlatformSubject) -> Result<Vec<PlatformRoleId>>;
    pub async fn assignments(&self, subject: &PlatformSubject) -> Result<Vec<PlatformRoleAssignment>>;

    pub async fn set_role_permissions(&self, input: SetPlatformRolePermissionsInput) -> Result<()>;
    pub async fn permissions(&self, subject: &PlatformSubject) -> Result<Vec<Permission>>;

    pub async fn can_platform(&self, request: PlatformAccessRequest) -> Result<AccessDecision>;
    pub async fn accessible_tenants(&self, query: TenantDataScopeQuery) -> Result<TenantDataAccessScope>;
    pub async fn can_access_tenant(&self, request: TenantDataAccessRequest) -> Result<AccessDecision>;
    pub async fn can_access_tenant_scope(&self, request: TenantScopedDataAccessRequest) -> Result<AccessDecision>;
}
```

平台缓存失效必须独立于租户缓存：

| 操作 | 失效策略 |
|---|---|
| `assign_roles(subject, ...)` | `invalidate_platform_principal(subject.principal)` |
| `set_role_permissions(role, ...)` | `invalidate_platform_role(role)` |
| `set_parent_roles(role, ...)` | `invalidate_platform_all()`，因为平台角色继承影响范围不一定可快速求出 |
| `update_role(disabled = true)` | `invalidate_platform_role(role)` |
| `delete_role` | `invalidate_platform_role(role)` |
| `platform_principal_status` 变更 | 由应用调用 `invalidate_platform_principal` |

如果 v0.5 暂不实现平台缓存，也要在接口命名和文档中预留独立缓存边界，不能复用 `tenant + principal + config` 的 cache key。

## 10. 事务边界

Store trait 不直接定义事务泛型，避免把各种数据库事务类型泄漏到公共 API。

推荐规则：

- 单表写入可在 Store 实现内直接完成。
- 多表写入由 Store 实现内部开启事务。
- Service 只表达业务顺序和缓存失效，不持有具体数据库 session。

后续如果需要事务扩展，可以另行设计：

```rust
pub trait TransactionalTenantAuthStore {
    type Transaction<'a>;
}
```

v0.5 不提前暴露该复杂度。

## 11. 错误模型

建议新增：

```rust
pub enum StoreError {
    NotFound,
    Conflict,
    InvalidInput(String),
    SystemRoleProtected,
    DisabledRole,
    Source(SourceError),
}
```

Service 层转换成：

```rust
pub enum IamError {
    Store(StoreError),
    InvalidRole(String),
    InvalidAssignment(String),
    InvalidPermission(String),
    RoleCycleDetected,
}
```

规则：

- 存储不可用、数据库错误应向上返回错误，Web 层 fail closed。
- 不要把系统错误伪装成无权限。
- `NotFound`、`Conflict` 等语义留给 Axum 层映射成 HTTP 状态码。

## 12. 接入示例

推荐使用方式：

```rust
let service = TenantIamService::builder(store)
    .engine_config(EngineConfig {
        enable_role_hierarchy: true,
        enable_wildcard: true,
        max_role_depth: 16,
    })
    .cache(MemoryCache::new(100_000))
    .build();

service
    .assign_roles(SetRoleAssignmentsInput {
        subject: AuthSubject::new(tenant.clone(), principal.clone()),
        assignments: vec![RoleAssignment::new(role.clone(), GrantScope::tenant())],
    })
    .await?;

let decision = service
    .can_tenant(TenantAccessRequest {
        subject: AuthSubject::new(tenant, principal),
        permission: Permission::parse("order:read")?,
    })
    .await?;
```

接入方仍然可以只使用 core：

```rust
let source = TenantAuthSource::new(store);
let engine = EngineBuilder::new(source).build();
```

平台侧推荐使用方式：

```rust
let platform_service = PlatformIamService::builder(platform_store)
    .engine_config(PlatformEngineConfig {
        enable_role_hierarchy: true,
        enable_wildcard: true,
        max_role_depth: 16,
    })
    .build();

platform_service
    .assign_roles(SetPlatformRoleAssignmentsInput {
        subject: PlatformSubject::new(platform_principal.clone()),
        assignments: vec![
            PlatformRoleAssignment::new(
                platform_role.clone(),
                PlatformGrantScope::all_tenants(),
            ),
        ],
    })
    .await?;

let scope = platform_service
    .accessible_tenants(TenantDataScopeQuery {
        subject: PlatformSubject::new(platform_principal),
        permission: Permission::parse("tenant/order:read")?,
    })
    .await?;
```

## 13. 测试清单

必须覆盖：

- Store adapter 能正确实现 `AuthorizationSource`。
- disabled role 不参与授权。
- `assign_roles` 后失效 principal cache。
- `set_role_permissions` 后失效 role cache。
- `set_parent_roles` 后失效 tenant cache。
- `delete_role` 拒绝删除 system role。
- 空角色名被拒绝。
- 重复 permission 去重。
- 重复 assignment 去重。
- `permissions(subject)` 返回继承后的有效权限。
- Store 错误不会被转换成 allow。
- `PlatformAuthSource` 能正确实现 `PlatformAuthorizationSource`。
- disabled platform role 不参与平台授权。
- `set_platform_role_permissions` 后失效 platform role cache。
- `set_platform_parent_roles` 后不影响租户 cache。
- `PlatformGrantScope::platform()` 不能访问租户数据。
- `PlatformGrantScope::tenant_paths(...)` 不能被当成租户级全量授权。

## 14. 实施阶段

### 阶段一：API 草案

- 新增 `iam` feature。
- 新增 `platform-iam = ["platform", "iam"]` feature。
- 新增 `iam::common` crate-private 支撑模块，先沉淀通用角色元数据、输入归一、permission/assignment 去重和缓存失效动作。
- 定义 `RoleRecord`、输入 DTO、错误类型。
- 定义 `TenantAuthStore`。
- 定义 `PlatformRoleRecord`、平台输入 DTO、平台错误类型。
- 定义 `PlatformAuthStore`。
- 文档中标注这是高层管理模块，不替代 core。

### 阶段二：Source Adapter

- 实现 `TenantAuthSource<S>`。
- 实现 `PlatformAuthSource<S>`。
- 补 adapter 单元测试。
- 更新生产接入文档，推荐新项目优先实现 Store。

### 阶段三：Service

- 实现 `TenantIamService` builder。
- 基于 `iam::common::service_support` 实现角色、绑定、权限、继承管理方法。
- 实现缓存失效编排。
- 实现 `PlatformIamService` builder。
- 基于同一组 common helper 实现平台角色、平台绑定、平台权限、平台继承管理方法。
- 实现平台缓存失效编排或预留独立失效接口。

### 阶段四：内存实现

- 可选提供 `MemoryTenantAuthStore`，用于集成测试和示例。
- 可选提供 `MemoryPlatformAuthStore`，用于集成测试和示例。
- 不替换现有 `MemorySource`；`MemorySource` 保持 core 示例用途。
- 不替换现有 `MemoryPlatformSource`；`MemoryPlatformSource` 保持 platform core 示例用途。

### 阶段五：示例和迁移文档

- 增加“从 `AuthorizationSource` 迁移到 `TenantAuthStore`”章节。
- 增加“从 `PlatformAuthorizationSource` 迁移到 `PlatformAuthStore`”章节。
- 增加最小 SQL 表建议。
- 增加 profile 权限读取示例。

## 15. 结论

v0.5 的重点不是让授权规则更复杂，而是把业务项目最常重复实现的 IAM 管理边界收敛成标准 API。租户侧和平台侧都需要这层能力，但必须保持并行而不是合并。

完成后，新项目的默认路径应该是：

```text
实现 TenantAuthStore
        |
        +-- TenantIamService 管理角色/绑定/权限
        |
        +-- TenantAuthSource 接入 Engine
        |
        +-- 业务代码调用 can_tenant / can_access_scope / accessible_scope
```

平台侧对应路径是：

```text
实现 PlatformAuthStore
        |
        +-- PlatformIamService 管理平台角色/绑定/权限
        |
        +-- PlatformAuthSource 接入 PlatformEngine
        |
        +-- 平台代码调用 can_platform / accessible_tenants / can_access_tenant_scope
```

这样 `rs-tenant` 才开始从“判定内核”向“可接入框架”迈出第一步。

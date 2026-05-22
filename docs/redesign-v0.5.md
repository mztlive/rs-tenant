# rs-tenant v0.5 Store 与 IAM Service 设计方案

> 状态：0.5.0 实施合同草案
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

### 4.2 Feature 合同

v0.5 需要把 feature 关系固定下来，避免使用方启用一个高层模块时意外引入另一侧语义：

```toml
[features]
iam = []
platform-iam = ["platform", "iam"]
iam-memory-store = ["iam", "memory-store"]
platform-iam-memory-store = ["platform-iam", "memory-store"]
```

规则：

- `iam` 只暴露租户侧 IAM 管理能力和 `iam::common` 内部支撑。
- `platform-iam` 才暴露 `platform::iam`，并隐式启用 `platform`。
- `memory-store` 继续只代表 core 示例数据源；如果要提供 IAM Store 内存实现，应使用 `iam-memory-store` 或 `platform-iam-memory-store`。
- `serde` 继续控制 DTO 的序列化能力；所有新增 public DTO 都应使用 `#[cfg_attr(feature = "serde", derive(...))]`。
- 不启用 `iam` 时，现有 v0.4 public API、示例和编译结果不应变化。

### 4.3 Service 所有权合同

`TenantIamService` 和 `PlatformIamService` 必须能同时持有 Store 和 Source Adapter。实现时不要把同一个 `S` 移动到两个字段里，应使用 `Arc<S>` 作为共享所有权边界：

```rust
pub struct TenantAuthSource<S: ?Sized> {
    store: Arc<S>,
}

pub struct TenantIamService<S: ?Sized, C = NoCache> {
    store: Arc<S>,
    engine: Engine<TenantAuthSource<S>, C>,
    options: TenantIamServiceOptions,
}
```

构造器规则：

- `TenantIamService::builder(store: S)` 接收 owned store，并内部包装成 `Arc<S>`。
- `TenantIamService::builder_from_arc(store: Arc<S>)` 支持复用外部已有 Store。
- `TenantAuthSource::new(store: Arc<S>)` 只持有 `Arc`，clone 成本固定。
- 平台侧 `PlatformAuthSource`、`PlatformIamService` 使用同样形状。
- 如果后续要支持 trait object，应允许 `Arc<dyn TenantAuthStore>` 和 `Arc<dyn PlatformAuthStore>` 作为构造输入。

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

- `tenant + id` 是租户角色的唯一身份。
- `system` 表示系统内置或模板物化角色，默认不允许删除。
- `disabled` 表示角色存在但不参与授权；管理侧仍然可以展示和编辑。
- `name` 面向管理界面展示，不参与权限判定；库只保证非空和归一，不强制全局唯一。

归一规则：

- `name` 由 service trim，trim 后不能为空。
- `description` 由 service trim，trim 后为空字符串时归一为 `None`。
- 读接口返回的 `name`、`description` 必须已经是归一后的值。
- 列表接口返回顺序应稳定；推荐按 `disabled asc, system desc, name asc, id asc` 排序。

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

pub struct DeleteRoleInput {
    pub tenant: TenantId,
    pub role: RoleId,
    pub allow_system: bool,
}
```

规则：

- `RoleId` 继续使用现有构造校验。
- `create_role` 遇到已存在 `tenant + id` 必须返回 `Conflict`。
- `update_role` 遇到不存在角色必须返回 `NotFound`。
- `UpdateRoleInput` 所有字段都是 `None` 时返回当前记录，不写库，不触发缓存失效。
- `DeleteRoleInput::allow_system = false` 时，删除 system role 必须返回 `SystemRoleProtected`。
- 禁用角色后，该角色及通过该角色继承得到的授权都不得继续参与判定。

### 5.3 AssignmentInput

```rust
pub struct SetRoleAssignmentsInput {
    pub subject: AuthSubject,
    pub assignments: Vec<RoleAssignment>,
}
```

规则：

- `set_role_assignments` 是全量替换，不是增量追加。
- 空 `assignments` 表示清空该 subject 的所有角色绑定。
- 非空 assignment 必须引用存在且未禁用的角色。
- 同一 subject 下重复的 `role + scope` 必须去重。
- 同一 role 允许以不同 scope 多次绑定；最终授权由 core 合并。
- `GrantScope::paths` 的非空和路径压缩继续由 core 类型保证。
- 库不强制 subject 当前 membership 为 active，避免阻止预授权；真正判定仍由 `membership_status` fail closed。

### 5.4 RolePermissionInput

```rust
pub struct SetRolePermissionsInput {
    pub tenant: TenantId,
    pub role: RoleId,
    pub permissions: Vec<Permission>,
}
```

规则：

- `set_role_permissions` 是全量替换。
- 空 `permissions` 表示角色存在但没有授权。
- 权限必须去重，并保持稳定顺序；推荐按 `resource asc, action asc`。
- 角色不存在返回 `NotFound`。
- 角色禁用时仍允许维护权限，但授权读取视图必须返回空权限。
- 可选接入 v0.6 `PermissionCatalog`；启用 catalog 后，未登记权限返回 `UnknownPermission`。

### 5.5 ParentRoleInput

```rust
pub struct SetParentRolesInput {
    pub tenant: TenantId,
    pub role: RoleId,
    pub parents: Vec<RoleId>,
}
```

规则：

- `set_parent_roles` 是全量替换。
- 空 `parents` 表示移除该角色的直接父角色。
- `role` 和所有 parent 必须存在且未禁用。
- 禁止自继承、重复 parent、继承环、超过 `max_role_depth` 的继承链。
- 继承链检查必须以写入后的完整图为准。

### 5.6 EffectivePermissionGrant

`permissions(subject)` 只返回权限会丢失 scope，不能作为业务授权依据。v0.5 Service 应公开带 scope 的有效授权读取：

```rust
pub struct EffectivePermissionGrant {
    pub role: RoleId,
    pub permission: Permission,
    pub scope: GrantScope,
}
```

规则：

- `effective_grants(subject)` 返回继承展开后的 `role + permission + scope`。
- `permissions(subject)` 可以作为展示用 convenience 方法，但文档必须标注“去重后仅用于 profile 展示，不可用于授权判断”。
- 业务接口仍应使用 `can_tenant`、`can_access_scope`、`accessible_scope`。

## 6. Tenant Store 合同

v0.5 的核心是接入方实现的 Store。Store 既服务管理侧，也服务授权判定侧，因此必须区分 raw/admin 读取和 authz 读取：

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

    async fn delete_role(&self, input: DeleteRoleInput) -> Result<(), StoreError>;

    async fn role_assignments(
        &self,
        subject: &AuthSubject,
    ) -> Result<Vec<RoleAssignment>, StoreError>;

    async fn authorization_role_assignments(
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

    async fn authorization_role_permissions(
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

    async fn authorization_parent_roles(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> Result<Vec<RoleId>, StoreError>;

    async fn set_parent_roles(&self, input: SetParentRolesInput) -> Result<(), StoreError>;
}
```

读取语义：

| 方法 | 合同 |
|---|---|
| `tenant_status` | 不存在、禁用、不可用租户都返回 `Inactive`；存储错误返回 `StoreError` |
| `membership_status` | 不存在、禁用、不可用成员都返回 `Inactive`；存储错误返回 `StoreError` |
| `roles` | 管理侧 raw 列表，包含 disabled/system role |
| `role` | 管理侧 raw 单条，disabled/system role 也应返回 |
| `role_assignments` | 管理侧 raw 绑定，保留已绑定但后来被禁用的角色 |
| `authorization_role_assignments` | 授权读取视图，只返回存在且未禁用角色的绑定 |
| `role_permissions` | 管理侧 raw 权限，disabled role 的权限也应返回 |
| `authorization_role_permissions` | 授权读取视图，角色不存在或 disabled 时返回空列表 |
| `parent_roles` | 管理侧 raw 父角色关系 |
| `authorization_parent_roles` | 授权读取视图，只返回存在且未禁用的父角色 |

写入语义：

| 方法 | 合同 |
|---|---|
| `create_role` | 原子创建角色；重复 `tenant + role` 返回 `Conflict` |
| `update_role` | 原子更新角色元数据；不存在返回 `NotFound` |
| `delete_role` | `allow_system = false` 时拒绝删除 system role；默认拒绝删除被绑定或被继承引用的角色 |
| `set_role_assignments` | 原子替换 subject 全部绑定；空列表清空 |
| `set_role_permissions` | 原子替换 role 全部权限；空列表清空 |
| `set_parent_roles` | 原子替换直接父角色；写入前完成存在性、disabled、重复、自继承、环检测 |

这个 trait 不暴露数据库事务、连接池或 ORM 类型。具体项目可以在实现内部使用 SQL、Mongo、Redis 或内部服务，但对外必须满足上面的原子性和错误语义。

## 7. AuthorizationSource 适配

提供一个只读适配器：

```rust
pub struct TenantAuthSource<S: ?Sized> {
    store: Arc<S>,
}
```

实现：

```rust
#[async_trait]
impl<S> AuthorizationSource for TenantAuthSource<S>
where
    S: TenantAuthStore + ?Sized,
{
    async fn tenant_status(&self, tenant: &TenantId) -> Result<TenantStatus, SourceError> {
        self.store.tenant_status(tenant).await.map_err(to_source_error)
    }

    async fn membership_status(&self, subject: &AuthSubject) -> Result<MembershipStatus, SourceError> {
        self.store.membership_status(subject).await.map_err(to_source_error)
    }

    async fn role_assignments(&self, subject: &AuthSubject) -> Result<Vec<RoleAssignment>, SourceError> {
        self.store.authorization_role_assignments(subject).await.map_err(to_source_error)
    }

    async fn role_permissions(&self, tenant: &TenantId, role: &RoleId) -> Result<Vec<Permission>, SourceError> {
        self.store.authorization_role_permissions(tenant, role).await.map_err(to_source_error)
    }

    async fn parent_roles(&self, tenant: &TenantId, role: &RoleId) -> Result<Vec<RoleId>, SourceError> {
        self.store.authorization_parent_roles(tenant, role).await.map_err(to_source_error)
    }
}
```

处理规则：

- Adapter 必须只调用 authz 读取视图，不能调用 raw 管理读取视图。
- `StoreError` 转成 `SourceError`，再由 `Engine` 包装为 `Error::Source`。
- Store 错误必须 fail closed：不得转换成空列表、Inactive 或 Allow。
- `tenant_status = Inactive` 或 `membership_status = Inactive` 是业务状态，不是 Store 错误。

## 8. TenantIamService

Service 是应用推荐使用的入口：

```rust
pub struct TenantIamService<S: ?Sized, C = NoCache> {
    store: Arc<S>,
    engine: Engine<TenantAuthSource<S>, C>,
    options: TenantIamServiceOptions,
}

pub struct TenantIamServiceOptions {
    pub allow_delete_system_role: bool,
}
```

默认配置：

- `allow_delete_system_role = false`。
- 角色删除使用 restrict 语义：仍被绑定或被继承引用时返回 `Conflict`。
- v0.5 不提供库内 cascade 删除；应用如果需要级联清理，应在自己的 Store/service 中显式实现。
- 先完成 Store 写入，写入成功后执行缓存失效。
- 写入失败不执行缓存失效。

核心方法：

```rust
impl<S, C> TenantIamService<S, C>
where
    S: TenantAuthStore + ?Sized,
    C: Cache,
{
    pub async fn create_role(&self, input: CreateRoleInput) -> Result<RoleRecord, IamError>;
    pub async fn update_role(&self, input: UpdateRoleInput) -> Result<RoleRecord, IamError>;
    pub async fn delete_role(&self, tenant: &TenantId, role: &RoleId) -> Result<(), IamError>;

    pub async fn assign_roles(&self, input: SetRoleAssignmentsInput) -> Result<(), IamError>;
    pub async fn role_ids(&self, subject: &AuthSubject) -> Result<Vec<RoleId>, IamError>;
    pub async fn assignments(&self, subject: &AuthSubject) -> Result<Vec<RoleAssignment>, IamError>;

    pub async fn set_role_permissions(&self, input: SetRolePermissionsInput) -> Result<(), IamError>;
    pub async fn set_parent_roles(&self, input: SetParentRolesInput) -> Result<(), IamError>;
    pub async fn effective_grants(&self, subject: &AuthSubject) -> Result<Vec<EffectivePermissionGrant>, IamError>;
    pub async fn permissions(&self, subject: &AuthSubject) -> Result<Vec<Permission>, IamError>;

    pub async fn accessible_scope(&self, query: ScopeQuery) -> Result<AccessScope, IamError>;
    pub async fn can_tenant(&self, request: TenantAccessRequest) -> Result<AccessDecision, IamError>;
    pub async fn can_access_scope(&self, request: ScopedAccessRequest) -> Result<AccessDecision, IamError>;

    pub async fn invalidate_principal(&self, tenant: &TenantId, principal: &PrincipalId);
    pub async fn invalidate_role(&self, tenant: &TenantId, role: &RoleId);
    pub async fn invalidate_tenant(&self, tenant: &TenantId);
}
```

缓存失效：

| 操作 | 失效策略 |
|---|---|
| `assign_roles(subject, ...)` | `invalidate_principal(subject.tenant, subject.principal)` |
| `set_role_permissions(tenant, role, ...)` | `invalidate_role(tenant, role)` |
| `set_parent_roles(tenant, role, ...)` | `invalidate_tenant(tenant)` |
| `update_role(disabled = true)` | `invalidate_role(tenant, role)` |
| `update_role(disabled = false)` | `invalidate_role(tenant, role)` |
| `delete_role` | `invalidate_role(tenant, role)` |
| `tenant_status` 变更 | 由应用调用 `invalidate_tenant` |
| `membership_status` 变更 | 由应用调用 `invalidate_principal` |

缓存合同：

- v0.5 复用现有 `Cache`，失效方法返回 `()`，实现层不得 panic。
- `MemoryCache::invalidate_role` 可以退化为 tenant 级失效，这是安全但较粗的策略。
- 分布式缓存如果无法提供可靠失效，应关闭缓存或用短 TTL；不能在角色收权场景依赖“最终会过期”作为唯一安全机制。

## 9. 平台侧 Store 与 Service

平台侧能力不能复用 `TenantAuthStore`，因为平台主体没有 `TenantId`，平台授权结果也不是 `AccessScope`。v0.5 需要提供并行 Store，并同样区分 raw/admin 读取和 authz 读取：

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
        input: DeletePlatformRoleInput,
    ) -> Result<(), PlatformStoreError>;

    async fn platform_role_assignments(
        &self,
        subject: &PlatformSubject,
    ) -> Result<Vec<PlatformRoleAssignment>, PlatformStoreError>;

    async fn authorization_platform_role_assignments(
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

    async fn authorization_platform_role_permissions(
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

    async fn authorization_platform_parent_roles(
        &self,
        role: &PlatformRoleId,
    ) -> Result<Vec<PlatformRoleId>, PlatformStoreError>;

    async fn set_platform_parent_roles(
        &self,
        input: SetPlatformParentRolesInput,
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

pub struct DeletePlatformRoleInput {
    pub role: PlatformRoleId,
    pub allow_system: bool,
}

pub struct PlatformEffectivePermissionGrant {
    pub role: PlatformRoleId,
    pub permission: Permission,
    pub scope: PlatformGrantScope,
}
```

平台写入语义与租户侧保持一致：

- `DeletePlatformRoleInput::allow_system = false` 时，删除 system platform role 必须返回 `SystemRoleProtected`。
- 平台角色删除默认使用 restrict 语义，仍被平台主体绑定或被子角色继承引用时返回 `Conflict`。
- 平台 raw 读取保留 disabled role，`authorization_platform_*` 读取必须排除 disabled role。
- 平台 `permissions(subject)` 同样只用于展示；平台授权判断必须使用 `can_platform`、`accessible_tenants`、`can_access_tenant` 或 `can_access_tenant_scope`。

平台 source adapter 必须只调用 `authorization_platform_*` 读取视图。平台 service 使用 `Arc<S>`，并暴露与租户侧对应的管理方法和判定代理方法。

平台缓存不能复用租户 `Cache`。如果 v0.5 要支持平台缓存，应新增独立 trait：

```rust
#[async_trait]
pub trait PlatformCache: Send + Sync {
    async fn get_platform_effective_grants(
        &self,
        principal: &PlatformPrincipalId,
        config_signature: &str,
    ) -> Option<Vec<PlatformEffectivePermissionGrant>>;

    async fn set_platform_effective_grants(
        &self,
        principal: &PlatformPrincipalId,
        config_signature: &str,
        grants: Vec<PlatformEffectivePermissionGrant>,
    );

    async fn invalidate_platform_principal(&self, principal: &PlatformPrincipalId);
    async fn invalidate_platform_role(&self, role: &PlatformRoleId);
    async fn invalidate_platform_all(&self);
}
```

平台缓存失效：

| 操作 | 失效策略 |
|---|---|
| `assign_roles(subject, ...)` | `invalidate_platform_principal(subject.principal)` |
| `set_role_permissions(role, ...)` | `invalidate_platform_role(role)` |
| `set_parent_roles(role, ...)` | `invalidate_platform_all()` |
| `update_role(disabled = true/false)` | `invalidate_platform_role(role)` |
| `delete_role` | `invalidate_platform_role(role)` |
| `platform_principal_status` 变更 | 由应用调用 `invalidate_platform_principal` |

如果 v0.5 暂不实现平台缓存，`PlatformIamService` 应显式使用 `NoPlatformCache`，不要把平台缓存塞进 `Cache` 的 `tenant + principal + config` key。

## 10. 事务与并发边界

Store trait 不直接定义事务泛型，避免把各种数据库事务类型泄漏到公共 API。

必须原子完成的操作：

| 操作 | 原子范围 |
|---|---|
| `set_role_assignments` | 删除旧绑定、写入新绑定、去重结果必须在同一事务内完成 |
| `set_role_permissions` | 删除旧权限、写入新权限必须在同一事务内完成 |
| `set_parent_roles` | 校验继承图、删除旧父角色、写入新父角色必须在同一事务内完成 |
| `delete_role` | 引用检查和删除必须在同一事务内完成，避免检查后被新绑定引用 |

并发规则：

- 数据库唯一约束是最终一致性来源，service 层检查只改善错误信息。
- 并发创建同一个角色时，一个成功，其他返回 `Conflict`。
- 并发 `set_*` 默认 last-write-wins；如果应用需要乐观锁，应在应用 Store 实现中扩展版本字段。
- 缓存失效必须发生在事务提交之后；事务回滚不应失效缓存。
- 失效范围宁可过大，不可过小。

后续如果需要显式事务扩展，可以另行设计：

```rust
pub trait TransactionalTenantAuthStore {
    type Transaction<'a>;
}
```

v0.5 不提前暴露该复杂度。

## 11. 错误模型

错误类型需要支持稳定匹配，不能只靠字符串：

```rust
pub enum StoreEntity {
    TenantRole,
    TenantRoleAssignment,
    TenantRolePermission,
    TenantRoleParent,
    PlatformRole,
    PlatformRoleAssignment,
    PlatformRolePermission,
    PlatformRoleParent,
}

pub enum ConflictReason {
    AlreadyExists,
    ReferencedByAssignment,
    ReferencedByChildRole,
    DuplicateParent,
    CycleDetected,
    DepthExceeded,
}

pub enum StoreError {
    NotFound { entity: StoreEntity, id: String },
    Conflict { entity: StoreEntity, id: String, reason: ConflictReason },
    InvalidInput { field: &'static str, message: String },
    UnknownPermission { permission: Permission },
    SystemRoleProtected { role: String },
    DisabledRole { role: String },
    Storage { message: String },
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
    RoleDepthExceeded { max_depth: usize },
    UnauthorizedManagementOperation,
}
```

规则：

- 存储不可用、数据库错误应向上返回错误，Web 层 fail closed。
- 不要把系统错误伪装成无权限。
- `NotFound`、`Conflict`、`InvalidInput`、`SystemRoleProtected` 留给 Axum 层映射 HTTP 状态码。
- 授权判定方法返回的 Store 错误必须经过 `SourceError` 进入 `Error::Source`，不能变成 deny decision。
- 平台侧可以复用同一组错误形状，也可以提供 `PlatformStoreError = StoreError` type alias。

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
let store = Arc::new(store);
let source = TenantAuthSource::new(store);
let engine = EngineBuilder::new(source).build();
```

profile 展示应读取有效授权，而不是只读权限字符串：

```rust
let grants = service.effective_grants(&subject).await?;
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

## 13. 最小持久化建议

v0.5 不绑定数据库，但生产接入至少需要这些逻辑表。

租户侧：

| 表 | 主键 | 关键列 | 必要索引 |
|---|---|---|---|
| `tenant_roles` | `(tenant_id, role_id)` | `name, description, system, disabled` | `(tenant_id, disabled)`, `(tenant_id, name)` |
| `tenant_role_assignments` | `(tenant_id, principal_id, role_id, scope_hash)` | `scope_type, scope_payload` | `(tenant_id, principal_id)`, `(tenant_id, role_id)` |
| `tenant_role_permissions` | `(tenant_id, role_id, permission)` | `resource, action` 或完整 `permission` | `(tenant_id, role_id)` |
| `tenant_role_parents` | `(tenant_id, role_id, parent_role_id)` | 无 | `(tenant_id, parent_role_id)` |

平台侧：

| 表 | 主键 | 关键列 | 必要索引 |
|---|---|---|---|
| `platform_roles` | `role_id` | `name, description, system, disabled` | `(disabled)`, `(name)` |
| `platform_role_assignments` | `(principal_id, role_id, scope_hash)` | `scope_type, scope_payload` | `(principal_id)`, `(role_id)` |
| `platform_role_permissions` | `(role_id, permission)` | `resource, action` 或完整 `permission` | `(role_id)` |
| `platform_role_parents` | `(role_id, parent_role_id)` | 无 | `(parent_role_id)` |

持久化规则：

- `scope_hash` 必须基于归一后的 scope 计算，避免同义 scope 重复写入。
- 推荐把 scope payload 存成结构化 JSON 或数据库原生 JSON；不要只存展示字符串。
- 权限字符串必须使用 `Permission::parse` 成功后的标准格式。
- 删除 role 默认使用 restrict 语义；如果应用自定义 cascade，必须显式清理绑定、权限和继承边。

## 14. 测试与验收清单

必须覆盖：

- Store adapter 能正确实现 `AuthorizationSource`。
- raw 管理读取包含 disabled role，authz 读取排除 disabled role。
- disabled role 不参与授权、继承和权限读取。
- `assign_roles` 后失效 principal cache。
- `set_role_permissions` 后失效 role cache。
- `set_parent_roles` 后失效 tenant cache。
- `delete_role` 拒绝删除 system role。
- `delete_role` 默认拒绝删除仍被绑定或被继承引用的角色。
- 空角色名被拒绝，空描述归一为 `None`。
- 重复 permission 去重并稳定排序。
- 重复 assignment 去重，不同 scope 的同 role 绑定保留。
- 父角色重复、自继承、环、深度超限都被拒绝。
- `effective_grants(subject)` 返回继承后的 `permission + scope`。
- `permissions(subject)` 明确只用于展示，不作为授权依据。
- Store 错误不会被转换成 allow 或空授权。
- `PlatformAuthSource` 能正确实现 `PlatformAuthorizationSource`。
- disabled platform role 不参与平台授权。
- `set_platform_role_permissions` 后失效 platform role cache。
- `set_platform_parent_roles` 后不影响租户 cache。
- `PlatformGrantScope::platform()` 不能访问租户数据。
- `PlatformGrantScope::tenant_paths(...)` 不能被当成租户级全量授权。

实现完成后的验证命令：

```bash
cargo test --offline
cargo test --offline --features iam
cargo test --offline --features iam,memory-store,memory-cache,serde
cargo test --offline --features platform-iam,memory-store,memory-cache,serde
cargo test --offline --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --offline --no-deps --all-features
cargo package --list --allow-dirty
git diff --check
```

## 15. 实施阶段

### 阶段一：API 合同

- 新增 `iam` feature。
- 新增 `platform-iam = ["platform", "iam"]` feature。
- 新增 `iam-memory-store` 和 `platform-iam-memory-store` feature。
- 新增 `iam::common` crate-private 支撑模块，先沉淀通用角色元数据、输入归一、permission/assignment 去重和缓存失效动作。
- 定义 `RoleRecord`、输入 DTO、错误类型、`EffectivePermissionGrant`。
- 定义 raw/authz 分离后的 `TenantAuthStore`。
- 定义 `PlatformRoleRecord`、平台输入 DTO、平台错误类型、`PlatformEffectivePermissionGrant`。
- 定义 raw/authz 分离后的 `PlatformAuthStore`。
- 文档中标注这是高层管理模块，不替代 core。

### 阶段二：Source Adapter

- 实现 `TenantAuthSource<S>`，内部持有 `Arc<S>`。
- 实现 `PlatformAuthSource<S>`，内部持有 `Arc<S>`。
- Adapter 只调用 authz 读取视图。
- 补 adapter 单元测试。
- 更新生产接入文档，推荐新项目优先实现 Store。

### 阶段三：Service

- 实现 `TenantIamService` builder 和 `builder_from_arc`。
- 基于 `iam::common::service_support` 实现角色、绑定、权限、继承管理方法。
- 实现 `effective_grants` 和展示用 `permissions`。
- 实现缓存失效编排。
- 实现 `PlatformIamService` builder 和 `builder_from_arc`。
- 基于同一组 common helper 实现平台角色、平台绑定、平台权限、平台继承管理方法。
- 实现 `PlatformCache` 或显式 `NoPlatformCache`。

### 阶段四：内存实现

- 提供 `MemoryTenantAuthStore`，用于集成测试和示例。
- 提供 `MemoryPlatformAuthStore`，用于集成测试和示例。
- 不替换现有 `MemorySource`；`MemorySource` 保持 core 示例用途。
- 不替换现有 `MemoryPlatformSource`；`MemoryPlatformSource` 保持 platform core 示例用途。

### 阶段五：示例和迁移文档

- 增加“从 `AuthorizationSource` 迁移到 `TenantAuthStore`”章节。
- 增加“从 `PlatformAuthorizationSource` 迁移到 `PlatformAuthStore`”章节。
- 增加 SQL 表建议和索引说明。
- 增加 profile 有效授权读取示例。
- 增加 cache 失效和 disabled role 行为说明。

## 16. 结论

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

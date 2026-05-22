# rs-tenant v0.5 Store 与 IAM Service 设计方案

> 状态：0.5.0 设计草案
> 目标版本：`0.5.0`
> 范围：统一存储 trait、角色/绑定/权限管理 service、`AuthorizationSource` 适配、缓存失效编排
> 兼容策略：保留 v0.4 core 和 platform 语义，新增高层管理模块，不破坏现有判定 API

## 1. 背景

当前 `rs-tenant` 已经可以稳定回答：

```text
tenant + principal + permission -> decision / access scope
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

v0.5 的目标是补上最关键的一层：让接入方实现一个官方 Store trait，库负责提供通用管理 service，并把同一个 Store 适配给授权判定链路。

## 2. 目标

v0.5 需要让使用方的默认接入路径变成：

1. 实现 `TenantAuthStore`。
2. 用 `TenantIamService` 做角色、绑定、权限管理。
3. 用 `TenantAuthSource` 或直接由 service 暴露的 source 接入 `Engine`。
4. 在业务接口中继续使用 `Engine` 的 `can_tenant`、`can_access_scope`、`accessible_scope`。

核心目标：

- 提供租户内 IAM 管理的标准数据结构。
- 提供角色 CRUD、角色权限维护、角色绑定维护、角色继承维护的 service。
- 写入后统一执行缓存失效。
- 复用现有 `RoleId`、`PrincipalId`、`TenantId`、`Permission`、`GrantScope`、`RoleAssignment`。
- 让 `AuthorizationSource` 变成 Store 的只读视图，而不是接入方额外手写一套读取逻辑。

## 3. 非目标

v0.5 不做以下事情：

- 不绑定具体数据库、ORM 或迁移工具。
- 不提供完整后台页面。
- 不定义用户、员工、代理商、门店等业务身份模型。
- 不替应用决定 `AccountKind -> PrincipalId` 的映射。
- 不处理业务数据归属过滤，例如设备、销售单、耗材归属。
- 不引入动态策略语言或 Casbin policy 模型。
- 不把平台授权和租户授权混成一个 Store；平台侧可在后续版本做对应 Store。

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
    store.rs
    service.rs
    source.rs
    role.rs
    assignment.rs
    permission.rs
    error.rs
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

## 9. 事务边界

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

## 10. 错误模型

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

## 11. 接入示例

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

## 12. 测试清单

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

## 13. 实施阶段

### 阶段一：API 草案

- 新增 `iam` feature。
- 定义 `RoleRecord`、输入 DTO、错误类型。
- 定义 `TenantAuthStore`。
- 文档中标注这是高层管理模块，不替代 core。

### 阶段二：Source Adapter

- 实现 `TenantAuthSource<S>`。
- 补 adapter 单元测试。
- 更新生产接入文档，推荐新项目优先实现 Store。

### 阶段三：Service

- 实现 `TenantIamService` builder。
- 实现角色、绑定、权限、继承管理方法。
- 实现缓存失效编排。

### 阶段四：内存实现

- 可选提供 `MemoryTenantAuthStore`，用于集成测试和示例。
- 不替换现有 `MemorySource`；`MemorySource` 保持 core 示例用途。

### 阶段五：示例和迁移文档

- 增加“从 `AuthorizationSource` 迁移到 `TenantAuthStore`”章节。
- 增加最小 SQL 表建议。
- 增加 profile 权限读取示例。

## 14. 结论

v0.5 的重点不是让授权规则更复杂，而是把业务项目最常重复实现的 IAM 管理边界收敛成标准 API。

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

这样 `rs-tenant` 才开始从“判定内核”向“可接入框架”迈出第一步。

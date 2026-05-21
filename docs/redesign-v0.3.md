# rs-tenant v0.3 重构方案

> 状态：草案  
> 目标版本：`0.3.x`  
> 范围：概念重整、核心 API 重写、Store 边界调整、文档与迁移策略

## 1. 背景

当前 `rs-tenant` 的实现可以完成基础多租户 RBAC 判定，但概念边界不够稳定。主要表现是：

- `Scope` 与 `ScopePath` 名字接近，但职责不同。
- `scope(...)` 看起来像“查询可见范围”，实际只返回 `TenantOnly` 或 `None`。
- `GlobalRole` 与 `SuperAdmin` 都是平台级能力，但是否受租户内主体状态约束的语义不同。
- `Store` trait 同时承担数据读取与部分策略语义，边界偏模糊。
- `Permission` 的资源层级、wildcard、`ResourceName` 之间缺少严格模型。
- `casbin` feature 只是空开关，容易被理解为已有适配能力。

这份方案的目标不是修补现有 API，而是重新定义库的定位与领域模型，为一次 breaking change 提供设计依据。

## 2. 目标定位

`rs-tenant` v0.3 应定位为：

> 面向 Rust SaaS 系统的强类型多租户 RBAC 授权内核。

它应该提供：

- 明确的多租户授权流程。
- 强类型 ID、权限、角色、范围模型。
- 可插拔的数据读取接口。
- 可解释的授权结果。
- 查询前可用的数据范围输出。
- 可选的 Web 框架集成。

它不应该成为：

- 通用策略语言。
- Casbin 的完整替代品。
- ORM / 数据迁移框架。
- 用户、组织、租户、角色的完整管理系统。
- 业务资源权限的配置后台。

## 3. 当前问题清单

### 3.1 `Scope` 概念混乱

当前 `Scope` 是：

```rust
pub enum Scope {
    None,
    TenantOnly { tenant: TenantId },
}
```

问题：

- 名字像访问范围，表达能力却只有租户级。
- 与 `ScopePath` 容易混淆。
- `scope(tenant, principal, resource)` 无法返回 `principal_scope_path`。
- 文档说“查询前计算可访问范围”，但结果不足以完成层级数据过滤。

结论：`Scope` 需要重命名并扩展，或者直接被新的 `AccessScope` 取代。

### 3.2 `authorize_with_scope` 是点判定，不是范围计算

`authorize_with_scope(...)` 判断一个目标 `ScopePath` 是否可访问，适合操作单个对象。

它不适合查询场景，因为查询需要的是：

- 无权限。
- 整个租户。
- 某个层级路径及其子树。
- 将来可能还需要多个路径集合。

结论：点判定 API 与范围计算 API 必须分开。

### 3.3 平台角色与超级管理员边界不清

当前语义：

- `GlobalRole`：参与权限并集，但仍受 `principal_active(tenant, principal)` 约束。
- `SuperAdmin`：受 `tenant_active` 约束，但绕过 `principal_active`。

这可以成立，但必须有更明确的概念：

- `GlobalRole` 是平台定义的角色，不等于绕过租户成员状态。
- `SuperAdmin` 是平台级紧急/运维能力，可以绕过租户成员状态。

结论：这两个概念需要在类型、文档、解释结果里明确区分。

### 3.4 Store 边界不够纯粹

当前 `Store` 家族包括：

- `TenantStore`
- `RoleStore`
- `GlobalRoleStore`
- `ScopeStore`

问题：

- `Store` 听起来像存储，但 `is_super_admin`、`scope_allows` 已经带有策略判断。
- `scope_allows` 的默认实现是领域规则，却挂在 Store trait 上。
- 角色继承展开在 Engine 中完成，但继承数据读取在 Store 中完成，职责可接受，不过命名需要更明确。

结论：v0.3 应区分“读取接口”和“策略/规则对象”。

### 3.5 `Permission` 模型太松

当前 `Permission` 以最后一个 `:` 切分：

- `invoice:read` -> resource=`invoice`, action=`read`
- `billing:invoice:read` -> resource=`billing:invoice`, action=`read`

同时 wildcard 允许 `*`，但匹配逻辑主要覆盖：

- `*:*`
- `invoice:*`
- `*:read`

问题：

- 文档说 `resource:action`，但资源实际可以包含 `:`。
- resource 内部 wildcard 是否支持分段匹配不清楚。
- `ResourceName` 与 `Permission` 的 resource 部分没有同一个类型。

结论：需要把 `Resource`、`Action`、`Permission` 拆成明确值对象。

### 3.6 serde 边界绕过校验

当前强类型 ID 和 `Permission` 使用 `serde(transparent)` derive。反序列化时可能绕过 `new()` / `TryFrom` 的校验。

结论：v0.3 应手写反序列化，所有外部输入必须走同一套构造规则。

### 3.7 缓存失效语义不完整

当前缓存 key 是 `(tenant, principal)`，缓存内容是有效权限集合。

问题：

- 全局角色变更没有明确的失效 API。
- 角色继承变更、平台权限变更、租户成员状态变更都需要清晰失效策略。
- `Cache` 暴露在外，但 `Engine` 不提供统一失效入口。

结论：缓存应成为 Engine 管理下的实现细节，外部通过语义化 API 失效。

### 3.8 `casbin` feature 制造误导

当前 `casbin` feature 没有公开 API。

结论：v0.3 应二选一：

- 删除 `casbin` feature。
- 或实现明确的 Casbin adapter，并声明谁是最终决策源。

默认建议删除，直到有真实适配需求。

## 4. 新领域模型

### 4.1 租户与主体

```rust
pub struct TenantId(String);
pub struct PrincipalId(String);
```

保留强类型 ID，但所有构造路径必须校验。

新增：

```rust
pub struct TenantMembership {
    pub tenant: TenantId,
    pub principal: PrincipalId,
    pub status: MembershipStatus,
}

pub enum MembershipStatus {
    Active,
    Inactive,
}
```

意义：

- `principal_active` 不再只是布尔读取，而是租户成员关系的一部分。
- 平台主体是否属于租户，需要有明确语义。

### 4.2 角色

```rust
pub struct TenantRoleId(String);
pub struct PlatformRoleId(String);
```

建议将当前 `RoleId` / `GlobalRoleId` 改名：

- `RoleId` -> `TenantRoleId`
- `GlobalRoleId` -> `PlatformRoleId`

理由：

- `GlobalRole` 容易被理解为全局放行。
- `PlatformRole` 更准确：平台定义、跨租户复用，但是否能在某租户生效仍受授权流程约束。

### 4.3 权限

```rust
pub struct Resource(String);
pub struct Action(String);

pub struct Permission {
    resource: Resource,
    action: Action,
}
```

建议 API：

```rust
impl Permission {
    pub fn new(resource: Resource, action: Action) -> Self;
    pub fn parse(value: impl AsRef<str>) -> Result<Self>;
    pub fn resource(&self) -> &Resource;
    pub fn action(&self) -> &Action;
}
```

规则：

- 默认字符串格式仍为 `resource:action`。
- 如果 resource 允许层级，应明确使用 `/`，例如 `billing/invoice:read`。
- `:` 只用于分隔 resource 与 action。
- wildcard 初期只支持完整段：
  - `*:*`
  - `invoice:*`
  - `*:read`
- 不支持半开放分段 wildcard，除非未来引入 `PermissionPattern`。

### 4.4 范围

拆分两个概念：

```rust
pub struct ScopePath(String);

pub enum AccessScope {
    None,
    Tenant { tenant: TenantId },
    Path {
        tenant: TenantId,
        root: ScopePath,
    },
}
```

含义：

- `AccessScope::None`：没有查询权限。
- `AccessScope::Tenant`：可访问整个租户。
- `AccessScope::Path`：可访问某个路径及其子树。

`ScopePath` 只表示路径值对象，不代表最终访问范围。

### 4.5 授权决策

```rust
pub enum AccessDecision {
    Allow,
    Deny,
}
```

可以保留当前 `Decision`，但建议重命名为 `AccessDecision`，减少与其它业务决策混淆。

### 4.6 授权上下文

```rust
pub struct AuthSubject {
    pub tenant: TenantId,
    pub principal: PrincipalId,
}

pub struct AccessRequest {
    pub subject: AuthSubject,
    pub permission: Permission,
}

pub struct ScopedAccessRequest {
    pub subject: AuthSubject,
    pub permission: Permission,
    pub target: ScopePath,
}
```

理由：

- 减少函数参数过多。
- 为后续 explain、audit log、Web middleware 复用同一请求模型。

### 4.7 解释结果

新增：

```rust
pub struct AccessExplanation {
    pub decision: AccessDecision,
    pub reason: AccessReason,
    pub matched_permission: Option<Permission>,
    pub matched_role: Option<RoleRef>,
    pub effective_scope: Option<AccessScope>,
}

pub enum AccessReason {
    TenantInactive,
    PrincipalInactive,
    SuperAdmin,
    PermissionMatched,
    PermissionMissing,
    ScopeMatched,
    ScopeDenied,
    StoreError,
}
```

`explain(...)` 不应该替代 `can(...)`，而是用于调试、日志和测试。

## 5. 新 API 设计

### 5.1 核心判定 API

```rust
impl Engine {
    pub async fn can(&self, request: AccessRequest) -> Result<AccessDecision>;

    pub async fn can_ref(&self, request: &AccessRequest) -> Result<AccessDecision>;
}
```

语义：

1. 检查租户状态。
2. 检查超级管理员。
3. 检查租户成员状态。
4. 计算有效权限。
5. 匹配权限。

### 5.2 范围点判定 API

```rust
impl Engine {
    pub async fn can_access_scope(
        &self,
        request: ScopedAccessRequest,
    ) -> Result<AccessDecision>;
}
```

语义：

1. 先执行 `can(permission)`。
2. 若拒绝，直接拒绝。
3. 超级管理员返回允许。
4. 计算主体的 `AccessScope`。
5. 判断目标 `ScopePath` 是否被覆盖。

### 5.3 查询范围 API

```rust
impl Engine {
    pub async fn accessible_scope(
        &self,
        subject: AuthSubject,
        resource: Resource,
    ) -> Result<AccessScope>;
}
```

语义：

- 用于查询前过滤。
- 不接收 target scope。
- 返回可用于查询条件的 `AccessScope`。

示例：

```rust
match engine.accessible_scope(subject, Resource::parse("invoice")?).await? {
    AccessScope::None => return Ok(vec![]),
    AccessScope::Tenant { tenant } => repo.list_by_tenant(tenant).await,
    AccessScope::Path { tenant, root } => repo.list_by_scope_prefix(tenant, root).await,
}
```

### 5.4 解释 API

```rust
impl Engine {
    pub async fn explain(&self, request: AccessRequest) -> Result<AccessExplanation>;

    pub async fn explain_scope(
        &self,
        request: ScopedAccessRequest,
    ) -> Result<AccessExplanation>;
}
```

要求：

- 不暴露敏感内部错误。
- 能定位短路点。
- 能区分权限拒绝与范围拒绝。

### 5.5 缓存失效 API

```rust
impl Engine {
    pub async fn invalidate_principal(&self, tenant: &TenantId, principal: &PrincipalId);
    pub async fn invalidate_tenant_role(&self, tenant: &TenantId, role: &TenantRoleId);
    pub async fn invalidate_tenant(&self, tenant: &TenantId);
    pub async fn invalidate_platform_role(&self, role: &PlatformRoleId);
    pub async fn invalidate_platform_principal(&self, principal: &PrincipalId);
    pub async fn invalidate_all(&self);
}
```

说明：

- 外部不直接操作 `Cache`，除非实现自定义缓存。
- `invalidate_platform_role` 初期可以保守实现为全量失效。
- 文档必须说明每类数据变更应调用哪个失效 API。

## 6. Store / Repository 边界

### 6.1 新读取接口

建议拆成：

```rust
#[async_trait]
pub trait TenantReader {
    async fn tenant_status(&self, tenant: &TenantId) -> Result<TenantStatus, StoreError>;
}

#[async_trait]
pub trait MembershipReader {
    async fn membership_status(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> Result<MembershipStatus, StoreError>;

    async fn membership_scope(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> Result<Option<ScopePath>, StoreError>;
}

#[async_trait]
pub trait TenantRoleReader {
    async fn roles_for_principal(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> Result<Vec<TenantRoleId>, StoreError>;

    async fn permissions_for_role(
        &self,
        tenant: &TenantId,
        role: &TenantRoleId,
    ) -> Result<Vec<Permission>, StoreError>;

    async fn parent_roles(
        &self,
        tenant: &TenantId,
        role: &TenantRoleId,
    ) -> Result<Vec<TenantRoleId>, StoreError>;
}

#[async_trait]
pub trait PlatformRoleReader {
    async fn platform_roles_for_principal(
        &self,
        principal: &PrincipalId,
    ) -> Result<Vec<PlatformRoleId>, StoreError>;

    async fn permissions_for_platform_role(
        &self,
        role: &PlatformRoleId,
    ) -> Result<Vec<Permission>, StoreError>;
}

#[async_trait]
pub trait SuperAdminReader {
    async fn is_super_admin(&self, principal: &PrincipalId) -> Result<bool, StoreError>;
}
```

### 6.2 不再把规则放进 Store

`scope_allows` 不应是 Store 责任。

应移动到领域类型：

```rust
impl AccessScope {
    pub fn allows_path(&self, target: &ScopePath) -> bool;
}
```

这样 Store 只负责读取主体范围，范围匹配规则由库内领域类型统一维护。

### 6.3 Engine 组合接口

```rust
pub trait AuthorizationSource:
    TenantReader
    + MembershipReader
    + TenantRoleReader
    + PlatformRoleReader
    + SuperAdminReader
    + Send
    + Sync
{
}
```

这比 `Store` 更准确：它不是泛化存储，而是授权数据源。

## 7. 授权流程

### 7.1 `can(...)`

```text
can(request)
  -> tenant_status
    -> inactive: Deny(TenantInactive)
  -> super_admin
    -> true: Allow(SuperAdmin)
  -> membership_status
    -> inactive: Deny(PrincipalInactive)
  -> effective_permissions
  -> permission_match
    -> hit: Allow(PermissionMatched)
    -> miss: Deny(PermissionMissing)
```

注意：超级管理员是否绕过租户成员状态必须作为显式配置。

建议默认：

```rust
SuperAdminMode::BypassMembership
```

可选：

```rust
SuperAdminMode::RequireActiveMembership
```

### 7.2 `accessible_scope(...)`

```text
accessible_scope(subject, resource)
  -> tenant_status
  -> super_admin
    -> true: Tenant
  -> membership_status
  -> resource permission match
    -> miss: None
  -> membership_scope
    -> Some(path): Path
    -> None: Tenant
```

这里需要一个明确产品决策：

- 如果主体有资源权限但没有 `membership_scope`，是否代表整个租户？
- 建议默认代表整个租户，并允许通过配置改为 `None`。

```rust
MissingMembershipScopeMode::TenantWide
MissingMembershipScopeMode::Deny
```

### 7.3 `can_access_scope(...)`

```text
can_access_scope(request)
  -> can(permission)
    -> Deny: Deny
  -> accessible_scope(resource)
  -> allows_path(target)
```

## 8. 配置项

建议配置集中到一个结构体：

```rust
pub struct EngineConfig {
    pub role_hierarchy: RoleHierarchyMode,
    pub wildcard: WildcardMode,
    pub super_admin: SuperAdminMode,
    pub missing_scope: MissingMembershipScopeMode,
    pub max_role_depth: usize,
}
```

枚举代替多个 bool：

```rust
pub enum RoleHierarchyMode {
    Disabled,
    Enabled,
}

pub enum WildcardMode {
    Disabled,
    Simple,
}

pub enum SuperAdminMode {
    Disabled,
    BypassMembership,
    RequireActiveMembership,
}

pub enum MissingMembershipScopeMode {
    TenantWide,
    Deny,
}
```

理由：

- `enable_super_admin(true)` 无法表达是否绕过成员状态。
- `enable_wildcard(true)` 无法表达 wildcard 级别。
- 配置枚举更适合文档化和未来扩展。

## 9. Casbin 关系

### 9.1 不建议默认内置 Casbin

Casbin 的能力更通用，但抽象层级不同：

- Casbin 是策略引擎。
- `rs-tenant` 应是强类型多租户 RBAC 内核。

v0.3 默认应删除空的 `casbin` feature。

### 9.2 如果未来适配 Casbin

必须明确：

- Casbin 是最终决策源，还是只是权限数据源。
- `tenant_active`、`membership_status`、`super_admin` 这些语义由谁维护。
- `AccessScope` 如何从 Casbin 结果中得到。

建议的适配方向：

```rust
pub struct CasbinAuthorizationSource {
    // reads policy from Casbin,
    // but still returns typed rs-tenant values
}
```

不建议：

```rust
engine.can(...) && casbin.enforce(...)
```

同一请求链路里存在两个最终判定源，会制造语义漂移。

## 10. 模块结构建议

```text
src/
  lib.rs
  error.rs
  ids.rs
  permission.rs
  scope.rs
  decision.rs
  request.rs
  explanation.rs
  source.rs
  engine.rs
  cache.rs
  memory_source.rs
  axum.rs
```

职责：

- `ids.rs`：所有强类型 ID。
- `permission.rs`：`Resource`、`Action`、`Permission`、匹配规则。
- `scope.rs`：`ScopePath`、`AccessScope`、路径覆盖规则。
- `decision.rs`：`AccessDecision`、`AccessReason`。
- `request.rs`：`AuthSubject`、`AccessRequest`、`ScopedAccessRequest`。
- `source.rs`：授权数据读取接口。
- `engine.rs`：编排流程，不保存业务规则细节。
- `cache.rs`：缓存 trait、cache key、失效语义。
- `memory_source.rs`：测试和示例用内存实现。
- `axum.rs`：Web 集成。

## 11. API 迁移策略

这是 breaking change，建议使用 v0.3 完成，不在 v0.2 中渐进混用。

### 11.1 类型迁移

| v0.2 | v0.3 |
|---|---|
| `Decision` | `AccessDecision` |
| `Scope` | `AccessScope` |
| `RoleId` | `TenantRoleId` |
| `GlobalRoleId` | `PlatformRoleId` |
| `ResourceName` | `Resource` |
| `Store` | `AuthorizationSource` |
| `TenantStore` | `TenantReader` |
| `RoleStore` | `TenantRoleReader` |
| `GlobalRoleStore` | `PlatformRoleReader` |
| `ScopeStore` | `MembershipReader::membership_scope` |

### 11.2 方法迁移

| v0.2 | v0.3 |
|---|---|
| `authorize(...)` | `can(AccessRequest)` |
| `authorize_ref(...)` | `can_ref(&AccessRequest)` |
| `authorize_with_scope(...)` | `can_access_scope(ScopedAccessRequest)` |
| `scope(...)` | `accessible_scope(AuthSubject, Resource)` |
| `enable_wildcard(true)` | `EngineConfig { wildcard: WildcardMode::Simple }` |
| `enable_super_admin(true)` | `SuperAdminMode::BypassMembership` |
| `max_inherit_depth(n)` | `EngineConfig { max_role_depth: n }` |

### 11.3 兼容层

可以提供短期 feature：

```toml
[features]
compat-v02 = []
```

但不建议默认开启。

兼容层只做转发和 deprecated 标记：

```rust
#[deprecated(note = "use Engine::can with AccessRequest")]
pub async fn authorize(...) -> Result<Decision>;
```

## 12. 测试策略

### 12.1 领域测试

覆盖：

- ID 构造校验。
- serde 反序列化必须走校验。
- `Permission::parse` 切分规则。
- wildcard 匹配。
- `ScopePath::allows`.
- `AccessScope::allows_path`.

### 12.2 Engine 流程测试

覆盖：

- tenant inactive -> deny.
- super admin disabled -> normal flow.
- super admin bypass membership -> allow.
- super admin require membership -> inactive deny.
- tenant role permission allow.
- platform role permission allow.
- role inheritance allow.
- role cycle error.
- role depth exceeded.
- permission missing deny.

### 12.3 Scope 测试

覆盖：

- no resource permission -> `AccessScope::None`.
- super admin -> tenant wide.
- permission + no membership scope -> 根据配置返回 tenant wide 或 deny.
- permission + membership scope -> path scope.
- target path inside root -> allow.
- target path outside root -> deny.

### 12.4 Cache 测试

覆盖：

- 同一 principal 热缓存。
- 角色变更失效。
- 平台角色变更失效。
- 配置签名隔离。
- TTL 过期。
- 多 shard LRU 行为。

## 13. 文档策略

v0.3 文档应先讲概念，再讲 API。

建议文档目录：

```text
01-overview.md
02-concepts.md
03-permission-model.md
04-scope-model.md
05-authorization-flow.md
06-store-integration.md
07-cache-and-invalidation.md
08-axum-integration.md
09-migration-from-v02.md
10-casbin-boundary.md
```

现有文档里所有 `Scope` 相关描述需要重写，尤其是：

- `scope(...)` 的含义。
- `ScopePath` 的含义。
- 查询前过滤的示例。

## 14. 实施步骤

### Phase 1：概念和类型层

- 新增 `Resource`、`Action`、`Permission` 结构。
- 新增 `AccessScope`，废弃旧 `Scope`。
- 新增 `AuthSubject`、`AccessRequest`、`ScopedAccessRequest`。
- 新增 `AccessDecision`、`AccessExplanation`。
- 手写 serde 校验。

验收：

- 领域单测完整通过。
- 无 Engine 逻辑迁移。

### Phase 2：Source trait 重写

- 引入 `AuthorizationSource`。
- 拆分 reader trait。
- 移除 `ScopeStore::scope_allows`，把规则转入 `AccessScope`。
- 更新 `MemoryStore` 为 `MemoryAuthorizationSource` 或 `MemorySource`。

验收：

- 内存实现覆盖所有 reader。
- 旧 Store trait 可在 compat feature 下适配。

### Phase 3：Engine 重写

- 实现 `can(...)`。
- 实现 `accessible_scope(...)`。
- 实现 `can_access_scope(...)`。
- 实现 `explain(...)`。
- 引入 `EngineConfig`。

验收：

- 当前行为能通过迁移后的等价测试。
- 新 scope 能力有完整测试。

### Phase 4：Cache 重写

- 缓存 key 绑定配置签名。
- Engine 暴露语义化失效 API。
- 增加平台角色失效策略。

验收：

- 缓存测试覆盖 tenant role、platform role、principal、tenant。

### Phase 5：文档与迁移

- 重写 README。
- 更新 mdBook。
- 删除或实现 `casbin` feature。
- 增加 `migration-from-v02.md`。

验收：

- `cargo test --offline --features memory-store,memory-cache`
- `cargo clippy --all-targets --all-features -D warnings`
- mdBook 可构建。

## 15. 推荐决策

建议 v0.3 采用以下决策：

1. 删除空的 `casbin` feature。
2. 用 `AccessScope` 替代 `Scope`。
3. 把 `scope(...)` 改为 `accessible_scope(...)`。
4. 把 `authorize(...)` 改为 `can(...)`。
5. 把 `authorize_with_scope(...)` 改为 `can_access_scope(...)`。
6. 把 `GlobalRoleId` 改为 `PlatformRoleId`。
7. 把 `RoleId` 改为 `TenantRoleId`。
8. Store 改名为 `AuthorizationSource`。
9. Store 中不再放 `scope_allows` 这种规则方法。
10. 所有 serde 反序列化必须校验。

这些改动会破坏现有 API，但能显著降低概念歧义。对于一个权限库来说，概念稳定性比短期兼容性更重要。

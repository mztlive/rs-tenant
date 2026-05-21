# rs-tenant v0.3 重构方案

> 状态：v0.3.0 实现基线
> 目标版本：`0.3.x`
> 范围：租户内 RBAC 核心、权限模型、范围模型、Source 边界、缓存语义、文档重写
> 兼容策略：breaking rewrite，不提供 v0.2 兼容层

## 1. 背景

当前 `rs-tenant` 可以完成基础多租户 RBAC 判定，但概念边界不够稳定。主要表现是：

- `Scope` 与 `ScopePath` 名字接近，但职责不同。
- `scope(...)` 看起来像“查询可见范围”，实际只返回 `TenantOnly` 或 `None`。
- `authorize_with_scope(...)` 是目标点判定，不是查询范围计算。
- `GlobalRole` 混入租户授权流程，但没有明确业务语义。
- `SuperAdmin` 是平台级绕过能力，却和租户内授权耦合。
- `Store` trait 同时承担数据读取与部分策略语义，边界偏模糊。
- `Permission` 的资源层级、wildcard、`ResourceName` 之间缺少严格模型。
- `serde(transparent)` 会让外部反序列化绕过构造校验。
- `casbin` feature 只是空开关，容易被理解为已有适配能力。

v0.3 的目标不是把所有高级授权能力一次性塞进核心，而是先收敛出一个稳定、强类型、难误用的租户内授权内核。

## 2. 目标定位

`rs-tenant` v0.3 应定位为：

> 面向 Rust SaaS 系统的租户内 RBAC 授权内核。

它只回答一个核心问题：

> 某个主体在某个租户内，基于角色分配，是否拥有某个权限，以及该权限对应的数据范围是什么。

它应该提供：

- 明确的租户内授权流程。
- 强类型 ID、权限、角色、范围模型。
- 可插拔的授权数据读取接口。
- 查询前可用的数据范围输出。
- 安全的点判定 API。
- 可选的 Web 框架集成。

它不应该成为：

- 通用策略语言。
- Casbin 的完整替代品。
- 平台跨租户授权框架。
- super admin 绕过框架。
- ORM / 数据迁移框架。
- 用户、组织、租户、角色的完整管理系统。
- 业务资源权限的配置后台。
- 审计日志落库框架。

## 3. 核心边界

### 3.1 只保留租户内授权核心

v0.3 core 只处理租户上下文：

```text
tenant + principal + permission -> access scope
```

所有授权请求都必须有明确的 `TenantId` 和 `PrincipalId`。核心 Engine 不伪造租户，也不把平台主体自动转换成租户主体。

### 3.2 平台能力不进 core

以下能力暂不进入 v0.3 core：

- `PlatformSubject`
- `TenantSet`
- `PlatformAuthorizationSource`
- `can_platform(...)`
- `can_manage_tenant(...)`
- `RoleOwner::Platform`

如果应用需要平台管理能力，应在应用层先选择明确的目标租户，再决定如何把平台操作者映射为该租户内的授权主体。

未来如果确实需要平台能力，应作为独立 `platform` feature 设计，不能反向污染租户内核心模型。

### 3.3 SuperAdmin 不进 core

v0.3 core 不提供 `SuperAdminMode`，也不提供 `is_super_admin(...)`。

原因：

- super admin 本质是绕过策略，不是 RBAC 基础概念。
- 租户内 super admin 和平台 super admin 是不同风险面，不能用一个开关表达。
- 绕过 membership 会破坏 deny-by-default 的可解释性。
- 一旦进入核心，普通调用方很容易在不理解边界的情况下打开全局放行。

推荐做法：

- 租户内管理员：建普通角色，授予 `*:*`，并用 `GrantScope::Tenant` 分配。
- 运维救援能力：应用层显式创建临时租户成员、临时角色分配或一次性授权上下文。
- 平台超级管理员：留给未来独立 platform 设计，不作为租户 core 的副作用。

### 3.4 不兼容旧 API

v0.3 是 breaking rewrite：

- 删除 `authorize(...)`。
- 删除 `scope(...)`。
- 删除旧 `Scope`。
- 删除 `GlobalRoleId` / `GlobalRoleStore`。
- 删除 `ScopeStore`。
- 删除 `ResourceName`。
- 删除公开 unchecked constructor。
- 不提供 `compat-v02` feature。
- 不在核心实现里保留旧概念别名。

### 3.5 deny-by-default

任何缺省、缺失、无法解释的授权数据都应拒绝：

- 没有租户状态 -> 拒绝。
- 没有成员关系 -> 拒绝。
- 没有角色分配 -> 拒绝。
- 没有显式范围 -> 不推导为全租户。
- Source 读取失败 -> 返回错误，由调用方 fail closed。

### 3.6 范围必须绑定授权来源

查询范围不能只挂在 principal 或 membership 上。真实业务里同一个主体可能同时拥有：

- 对 `invoice:read` 的全租户权限。
- 对 `order:read` 的部分门店权限。
- 对 `customer:update` 的多个区域权限。

因此 v0.3 把 scope 绑定到 role assignment，而不是绑定到 membership。

### 3.7 避免裸权限判定误用

旧 `authorize(tenant, principal, permission)` 容易被误用于有业务数据范围的接口。

v0.3 不提供语义含糊的裸 `can(...)`。取而代之的是：

- `accessible_scope(...)`：查询权限对应的数据范围。
- `can_access_scope(...)`：判断目标路径是否可访问。
- `can_tenant(...)`：只用于没有下级业务范围的租户级操作。

`can_tenant(...)` 只有在权限命中并且最终范围是 `AccessScope::Tenant` 时才允许。路径级授权不会被当作租户级授权使用。

## 4. 领域模型

### 4.1 租户与主体

```rust
pub struct TenantId(String);
pub struct PrincipalId(String);

pub enum TenantStatus {
    Active,
    Inactive,
}

pub enum MembershipStatus {
    Active,
    Inactive,
}

pub struct AuthSubject {
    pub tenant: TenantId,
    pub principal: PrincipalId,
}
```

规则：

- `TenantStatus` 表达租户是否可参与授权。
- `MembershipStatus` 表达主体在租户内是否有效。
- `AuthSubject` 是 v0.3 core 唯一主体上下文。
- ID 构造必须 trim、校验非空、校验字符集和长度。
- serde 反序列化必须调用同一套构造规则。

### 4.2 权限

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
impl Resource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self>;
    pub fn as_str(&self) -> &str;
}

impl Action {
    pub fn parse(value: impl AsRef<str>) -> Result<Self>;
    pub fn as_str(&self) -> &str;
}

impl Permission {
    pub fn new(resource: Resource, action: Action) -> Self;
    pub fn parse(value: impl AsRef<str>) -> Result<Self>;
    pub fn resource(&self) -> &Resource;
    pub fn action(&self) -> &Action;
}
```

规则：

- 默认字符串格式为 `resource:action`。
- `:` 只作为 resource 与 action 的分隔符。
- resource 内不再允许 `:`。
- resource 层级使用 `/`，例如 `billing/invoice:read`。
- 默认 normalize 为 trim + lowercase。
- wildcard 初期只支持完整 resource/action：
  - `*:*`
  - `invoice:*`
  - `*:read`
- 不支持 `billing/*:read` 这类层级 wildcard，除非未来引入单独的 `PermissionPattern`。

### 4.3 授权范围

```rust
pub struct ScopePath(String);

pub enum GrantScope {
    Tenant,
    Paths(ScopeRoots),
}

pub struct ScopeRoots {
    // private fields
}

pub enum AccessScope {
    None,
    Tenant { tenant: TenantId },
    Paths {
        tenant: TenantId,
        roots: Vec<ScopePath>,
    },
}
```

含义：

- `ScopePath`：单个层级路径值对象，例如 `agent/123/store/456`。
- `GrantScope`：一次角色分配授予的范围，必须显式给出。
- `AccessScope`：某次权限查询合并后的最终范围。

规则：

- `GrantScope::Tenant` 明确表示全租户范围。
- `GrantScope::Paths` 支持多个路径根；公开 API 使用 `ScopeRoots` 保证非空和路径压缩不变量。
- 空 `Paths` 无意义，应在构造时拒绝。
- `AccessScope::Tenant` 覆盖所有路径。
- `AccessScope::Paths` 合并时应去重，并删除已经被祖先路径覆盖的子路径。
- 没有匹配授权时返回 `AccessScope::None`。
- `ScopePath::allows(target)` 使用相等或祖先路径规则。

### 4.4 角色与角色分配

```rust
pub struct RoleId(String);

pub struct RoleAssignment {
    pub role: RoleId,
    pub scope: GrantScope,
}
```

关键语义：

- scope 绑定到 role assignment。
- 角色继承得到的父角色权限，沿用当前 assignment 的 scope。
- `accessible_scope(permission)` 只合并权限命中的 assignments 的 scope。
- 不再使用 `membership_scope` 推导范围。

约束：

- 同一个 role 中的权限应共享同一种 scope 维度。
- 如果业务上 `invoice:read` 和 `store:manage` 需要完全不同的范围维度，应拆成不同 role。
- v0.3 不引入 permission-level grant scope，以避免核心模型过早复杂化。

示例：

```rust
RoleAssignment {
    role: RoleId::parse("invoice_reader")?,
    scope: GrantScope::Tenant,
}

RoleAssignment {
    role: RoleId::parse("store_order_reader")?,
    scope: GrantScope::paths(vec![
        ScopePath::parse("agent/123/store/456")?,
        ScopePath::parse("agent/123/store/789")?,
    ])?,
}
```

### 4.5 授权请求

```rust
pub struct TenantAccessRequest {
    pub subject: AuthSubject,
    pub permission: Permission,
}

pub struct ScopedAccessRequest {
    pub subject: AuthSubject,
    pub permission: Permission,
    pub target: ScopePath,
}

pub struct ScopeQuery {
    pub subject: AuthSubject,
    pub permission: Permission,
}
```

说明：

- `ScopeQuery` 必须带完整 `Permission`，不能只带 `Resource`。
- 查询范围按具体 action 计算，避免 `read` / `delete` / `manage` 共用范围。
- `TenantAccessRequest` 只适用于租户级操作，不适用于有下级目标路径的数据操作。

### 4.6 授权结果

```rust
pub enum AccessDecision {
    Allow,
    Deny,
}

pub enum DenyReason {
    TenantInactive,
    PrincipalInactive,
    PermissionMissing,
    TargetScopeRequired,
    ScopeDenied,
}
```

v0.3 core 的主 API 优先返回简单结果：

- `AccessDecision` 用于点判定。
- `AccessScope` 用于查询范围。
- `DenyReason` 只用于 explain / 测试 / 日志辅助，不作为业务分支的主要输入。

不在 v0.3 core 引入复杂的 `MatchedGrant` / `EffectiveAccess` 公开模型。内部可以保留 effective grant 结构用于计算和缓存。

## 5. 新 API 设计

### 5.1 查询范围 API

这是最核心的 API：

```rust
impl<S, C> Engine<S, C>
where
    S: AuthorizationSource,
    C: Cache,
{
    pub async fn accessible_scope(
        &self,
        query: ScopeQuery,
    ) -> Result<AccessScope>;
}
```

语义：

1. 检查租户状态。
2. 检查成员状态。
3. 读取角色分配。
4. 展开角色继承和权限。
5. 只收集权限命中的 role assignments。
6. 合并命中 assignments 的 scope。

示例：

```rust
let scope = engine
    .accessible_scope(ScopeQuery {
        subject,
        permission: Permission::parse("invoice:read")?,
    })
    .await?;

match scope {
    AccessScope::None => Ok(vec![]),
    AccessScope::Tenant { tenant } => repo.list_by_tenant(tenant).await,
    AccessScope::Paths { tenant, roots } => repo.list_by_scope_roots(tenant, roots).await,
}
```

### 5.2 范围点判定 API

```rust
impl<S, C> Engine<S, C>
where
    S: AuthorizationSource,
    C: Cache,
{
    pub async fn can_access_scope(
        &self,
        request: ScopedAccessRequest,
    ) -> Result<AccessDecision>;
}
```

语义：

1. 按 request.permission 计算 `AccessScope`。
2. 如果范围为 `None`，拒绝。
3. 如果范围为 `Tenant`，允许。
4. 如果范围为 `Paths`，判断 request.target 是否被覆盖。

这个 API 适合读取、更新、删除某个有明确层级路径的业务对象。

### 5.3 租户级判定 API

```rust
impl<S, C> Engine<S, C>
where
    S: AuthorizationSource,
    C: Cache,
{
    pub async fn can_tenant(
        &self,
        request: TenantAccessRequest,
    ) -> Result<AccessDecision>;
}
```

语义：

1. 先计算 `accessible_scope(request.subject, request.permission)`。
2. 只有结果是 `AccessScope::Tenant` 时返回 `Allow`。
3. 结果是 `AccessScope::Paths` 时返回 `Deny`。
4. 结果是 `AccessScope::None` 时返回 `Deny`。

这样可以避免路径级授权被误用于租户级操作。

适用场景：

- 租户设置。
- 租户级报表。
- 不绑定下级业务对象的操作。

不适用场景：

- 查看某个门店订单。
- 修改某个区域客户。
- 删除某个层级资源。

这些场景必须使用 `can_access_scope(...)` 或 `accessible_scope(...)`。

### 5.4 解释 API

v0.3 可以提供轻量解释 API，但不把解释模型做成复杂公开领域：

```rust
pub struct AccessExplanation {
    pub decision: AccessDecision,
    pub reason: Option<DenyReason>,
    pub scope: AccessScope,
}
```

要求：

- 能定位短路点。
- 能区分权限缺失、需要目标范围、范围拒绝。
- Source 错误通过 `Err` 返回，不放进 reason。
- 不暴露敏感内部错误。

不在 v0.3 core 公开 matched grant / effective access 解释明细。`EffectiveGrant` 仅服务公开 `Cache` trait 的扩展点，不应作为业务审计模型使用。需要审计明细的应用可以在未来 feature 中扩展。

## 6. Source 边界

### 6.1 单一授权数据源 trait

为了减少概念数量，v0.3 不再拆出多组公开 Store trait。核心只暴露一个 Source trait：

```rust
#[async_trait]
pub trait AuthorizationSource: Send + Sync {
    async fn tenant_status(
        &self,
        tenant: &TenantId,
    ) -> Result<TenantStatus, SourceError>;

    async fn membership_status(
        &self,
        subject: &AuthSubject,
    ) -> Result<MembershipStatus, SourceError>;

    async fn role_assignments(
        &self,
        subject: &AuthSubject,
    ) -> Result<Vec<RoleAssignment>, SourceError>;

    async fn role_permissions(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> Result<Vec<Permission>, SourceError>;

    async fn parent_roles(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> Result<Vec<RoleId>, SourceError>;
}
```

说明：

- Source 只读取授权数据。
- `parent_roles` 在 role hierarchy 关闭时不会被 Engine 调用。
- 如果接入方没有角色继承，可以返回空 vec。
- `scope_allows` 不再挂在 Source 上。
- permission 匹配、wildcard 匹配、role 展开、scope 合并都由库内确定性规则维护。

### 6.2 Source 不负责的事情

以下规则由领域类型或 Engine 维护：

- permission parse / match。
- wildcard match。
- role inheritance 展开。
- role cycle / max depth 检测。
- `GrantScope` 构造校验。
- `AccessScope` 合并。
- `AccessScope::allows_path(...)`。
- serde 校验。

Source 不应实现这些规则，避免不同接入方得到不同授权结果。

## 7. Engine 配置

建议配置集中到结构体：

```rust
pub struct EngineConfig {
    pub enable_role_hierarchy: bool,
    pub enable_wildcard: bool,
    pub max_role_depth: usize,
}
```

默认建议：

```rust
EngineConfig {
    enable_role_hierarchy: false,
    enable_wildcard: false,
    max_role_depth: 16,
}
```

不再提供：

- `enable_super_admin`
- `SuperAdminMode`
- `MissingMembershipScopeMode`
- `PlatformWriteAuditMode`

原因：

- super admin 是应用层策略。
- membership 缺失必须拒绝。
- 审计策略属于应用层。

## 8. 授权流程

### 8.1 `accessible_scope(...)`

```text
accessible_scope(query)
  -> tenant_status
    -> inactive: None
  -> membership_status
    -> inactive: None
  -> role_assignments
    -> empty: None
  -> expand role inheritance when enabled
  -> collect grants whose permission matches query.permission
  -> merge grant scopes
    -> no matched grant: None
    -> any Tenant grant: Tenant
    -> path grants only: Paths([...])
```

### 8.2 `can_access_scope(...)`

```text
can_access_scope(request)
  -> accessible_scope(subject, permission)
  -> None: Deny(PermissionMissing)
  -> Tenant: Allow
  -> Paths: allows_path(target)
    -> true: Allow
    -> false: Deny(ScopeDenied)
```

### 8.3 `can_tenant(...)`

```text
can_tenant(request)
  -> accessible_scope(subject, permission)
  -> Tenant: Allow
  -> Paths: Deny(TargetScopeRequired)
  -> None: Deny(PermissionMissing)
```

这个流程刻意不把 `Paths` 当成普通 Allow，避免调用方绕过目标范围。

## 9. SuperAdmin 处理方式

v0.3 core 不设计 `SuperAdminMode`。

### 9.1 租户内管理员

用普通 RBAC 表达：

```text
role: tenant_admin
permission: *:*
assignment scope: Tenant
```

这不会绕过：

- tenant active 检查。
- membership active 检查。

### 9.2 运维救援

如果需要平台运维人员临时进入租户，建议应用层显式建模：

- 创建临时租户成员。
- 创建临时角色分配。
- 使用有过期时间的运维授权记录。
- 在业务层记录审批和审计原因。

核心 Engine 仍只看到普通的 `AuthSubject` 和 `RoleAssignment`。

### 9.3 未来扩展原则

如果未来必须支持 super admin feature，应满足：

- 租户 super admin 与平台 super admin 分开配置。
- 不得用一个 `is_super_admin(principal)` 同时表达两种能力。
- 绕过 membership 必须是显式模式。
- explain 必须能清楚标记 super admin 短路。
- 默认关闭。

这些规则不进入 v0.3 core。

## 10. 平台能力处理方式

v0.3 core 不设计平台授权模型。

### 10.1 应用层处理平台上下文

平台后台通常有两类操作：

1. 平台自身资源：例如平台配置、全局套餐、租户列表。
2. 目标租户资源：例如代租户查看订单、处理客户问题。

第一类不属于 `rs-tenant` v0.3 core，应由应用自己的平台权限系统处理。

第二类必须先明确目标租户，再由应用层决定操作者如何成为该租户内授权主体。

### 10.2 不引入 `TenantSet`

不在 core 里提供 `TenantSet::All` / `TenantSet::Include`。

原因：

- 它会把跨租户可见性和租户内 RBAC 混在一起。
- 它会迫使 cache、explain、axum、Source 同时支持两套上下文。
- 绝大多数租户内业务授权不需要它。

未来如果要做平台 feature，应独立设计并独立测试。

## 11. 缓存设计

### 11.1 缓存内容

v0.3 不缓存裸 `Vec<Permission>`，而是缓存 effective grants。

由于 `Cache` trait 是公开扩展点，缓存值类型需要能被自定义 cache 实现命名；它仍然不作为业务授权解释模型使用，文档中隐藏其业务语义。

```rust
#[doc(hidden)]
pub struct EffectiveGrant {
    pub role: RoleId,
    pub permission: Permission,
    pub scope: GrantScope,
}
```

缓存 key 必须包含：

- tenant。
- principal。
- Engine 配置签名。
- role hierarchy / wildcard / max depth 等影响结果的配置。

建议不缓存 tenant status / membership status，或至少在每次授权前重新校验它们。否则成员禁用后容易命中过期授权。

### 11.2 失效 API

```rust
impl Engine {
    pub async fn invalidate_principal(&self, tenant: &TenantId, principal: &PrincipalId);
    pub async fn invalidate_role(&self, tenant: &TenantId, role: &RoleId);
    pub async fn invalidate_tenant(&self, tenant: &TenantId);
    pub async fn invalidate_all(&self);
}
```

正确性要求：

- 失效 API 返回后，受影响授权不得再命中过期 grant。
- `invalidate_role` 如果不能精确找到受影响主体，必须退化为 `invalidate_tenant` 或 `invalidate_all`。
- 角色继承关系变更时，应失效当前角色及受影响子角色；如果没有反向索引，必须粗粒度失效。
- role assignment 变化必须失效对应 principal。
- role permission 变化必须失效对应 role。
- membership 状态变化如果 status 不缓存，可以不失效 grant；如果缓存了 status，必须失效 principal。

这里不能使用 best-effort 语义。缓存可以慢一点，但不能让授权结果过期。

## 12. Axum 集成边界

v0.3 的 Web 集成只基于租户上下文。

建议提供两类能力：

- 租户级 layer：从 request extension 读取 `AuthSubject`，调用 `can_tenant(...)`。
- 范围级 helper：由调用方显式提供 `ScopePath`，调用 `can_access_scope(...)`。

不建议在通用 middleware 中自动从 path/query/body 推断 `ScopePath`，因为不同业务的路径编码和资源归属规则不同。

如果要做 scoped route 集成，应让应用提供一个 extractor / resolver：

```rust
pub trait ScopePathResolver {
    async fn resolve_scope_path(&self, request: &Request<Body>) -> Result<ScopePath>;
}
```

这个 resolver 属于 Web 集成层，不属于 core Engine。

JWT 集成也只负责提取：

- tenant id。
- principal id。

它不负责解释平台身份，也不负责生成 super admin 授权。

## 13. Casbin 关系

v0.3 删除空的 `casbin` feature。没有真实 adapter 前，不保留误导性开关。

未来如果适配 Casbin，必须明确：

- Casbin 是最终决策源，还是只是授权数据源。
- tenant status / membership status 由谁维护。
- role assignment scope 如何从 Casbin policy 映射。
- cache 失效由谁触发。

不建议同一请求链路里做：

```rust
engine.can_tenant(...) && casbin.enforce(...)
```

两个最终判定源会制造语义漂移。

## 14. 模块结构建议

```text
src/
  lib.rs
  error.rs
  ids.rs
  permission.rs
  scope.rs
  role.rs
  decision.rs
  request.rs
  source.rs
  engine.rs
  cache.rs
  memory_source.rs
  axum.rs
```

职责：

- `ids.rs`：所有强类型 ID。
- `permission.rs`：`Resource`、`Action`、`Permission`、匹配规则。
- `scope.rs`：`ScopePath`、`GrantScope`、`AccessScope`。
- `role.rs`：`RoleId`、`RoleAssignment`。
- `decision.rs`：`AccessDecision`、`DenyReason`。
- `request.rs`：`AuthSubject`、`TenantAccessRequest`、`ScopedAccessRequest`、`ScopeQuery`。
- `source.rs`：授权数据读取接口。
- `engine.rs`：编排流程。
- `cache.rs`：缓存 trait、cache key、失效语义。
- `memory_source.rs`：测试和示例用内存实现。
- `axum.rs`：Web 集成。

## 15. 测试策略

### 15.1 领域测试

覆盖：

- ID 构造校验。
- serde 反序列化必须走校验。
- `Permission::parse` 切分规则。
- wildcard 匹配。
- `ScopePath::allows`.
- `GrantScope` 构造校验。
- `AccessScope` 合并和路径覆盖。

### 15.2 Engine 流程测试

覆盖：

- tenant inactive -> none / deny。
- membership inactive -> none / deny。
- no role assignment -> none / deny。
- tenant role permission allow。
- permission missing deny。
- role inheritance allow。
- role cycle error。
- role depth exceeded。
- assignment tenant scope -> tenant accessible scope。
- assignment path scopes -> path accessible scope。
- multiple path assignments -> merged paths。
- scoped target inside root -> allow。
- scoped target outside roots -> deny。
- `can_tenant` with path-only grant -> deny。
- `can_tenant` with tenant grant -> allow。

### 15.3 Cache 测试

覆盖：

- tenant principal 热缓存。
- 配置签名隔离。
- assignment scope 变化失效。
- role permission 变化失效。
- role inheritance 变化失效。
- TTL 过期。
- 多 shard LRU 行为。
- role 精确失效不可用时的 tenant/all 退化行为。

### 15.4 非目标测试

v0.3 core 不写这些测试：

- platform tenant set。
- platform super admin。
- tenant bypass membership super admin。
- platform write audit mode。

这些能力不在 core 范围内。

## 16. 文档策略

v0.3 文档应先讲核心概念，再讲 API。

建议文档目录：

```text
01-overview.md
02-concepts.md
03-permission-model.md
04-scope-model.md
05-role-assignment.md
06-authorization-flow.md
07-source-integration.md
08-cache-and-invalidation.md
09-axum-integration.md
10-casbin-boundary.md
11-faq-troubleshooting.md
```

现有文档需要整体重写，尤其是：

- `scope(...)` 的含义。
- `ScopePath` / `GrantScope` / `AccessScope` 的差异。
- 查询前过滤必须按 `Permission` 计算。
- `can_tenant(...)` 不等于“有任意范围权限”。
- `can_access_scope(...)` 用于目标路径点判定。
- 旧 `GlobalRole` 不再存在。
- core 不提供 SuperAdmin。
- core 不提供平台跨租户授权。
- 没有 v0.2 兼容层。

## 17. 实施步骤

### Phase 1：领域类型

- 新增 `TenantStatus`、`MembershipStatus`。
- 新增 `Resource`、`Action`、`Permission`。
- 新增 `ScopePath`、`GrantScope`、`AccessScope`。
- 新增 `RoleAssignment`。
- 新增 request / decision 类型。
- 手写 serde 校验。
- 删除公开 unchecked constructor。

验收：

- 领域单测通过。
- `cargo test --offline --features serde` 通过。

### Phase 2：Source trait 重写

- 删除旧 `Store` / `TenantStore` / `RoleStore` / `GlobalRoleStore` / `ScopeStore`。
- 引入 `AuthorizationSource`。
- 更新内存实现为 `MemorySource`。
- 删除 `GlobalRoleId`。
- 删除 `ResourceName`。

验收：

- 内存实现覆盖 v0.3 source。
- 无旧 Store trait 公开导出。

### Phase 3：Engine 重写

- 实现 `accessible_scope(ScopeQuery)`。
- 实现 `can_access_scope(...)`。
- 实现 `can_tenant(...)`。
- 实现轻量 `explain_*`。
- 引入 `EngineConfig`。
- 实现 role inheritance 展开和 scope 合并。
- 删除 `authorize(...)`。
- 删除旧 `scope(...)`。

验收：

- 新租户授权流程测试完整通过。
- 路径级授权不会通过 `can_tenant(...)`。
- 不保留旧 API。

### Phase 4：Cache 重写

- 缓存 effective grants。
- 缓存 key 绑定配置签名。
- Engine 暴露语义化失效 API。
- 明确失效正确性测试。

验收：

- 缓存测试覆盖 assignment scope、principal、tenant、role、配置签名。

### Phase 5：Axum 与文档

- 更新 axum/JWT 设计，只提取 tenant subject。
- 移除平台/super admin 文档入口。
- 重写 README。
- 重写 mdBook。
- 删除空 `casbin` feature。
- 删除旧 API 文档和示例。

验收：

- `cargo test --offline --features memory-store,memory-cache,serde`
- `cargo clippy --all-targets --all-features -D warnings`
- mdBook 可构建。

## 18. 推荐决策

v0.3 建议采用以下最终决策：

1. 删除旧 API，不提供兼容层。
2. 核心只处理租户内授权。
3. 删除 `GlobalRoleId` / `GlobalRoleStore`。
4. 删除 `SuperAdminMode` 和 `is_super_admin(...)`。
5. 删除空的 `casbin` feature。
6. 用 `GrantScope` / `AccessScope` 替代旧 `Scope`。
7. 把范围绑定到 role assignment，而不是 membership。
8. `accessible_scope(...)` 必须接收完整 `Permission`。
9. `can_access_scope(...)` 通过 `accessible_scope(...)` 判断目标路径。
10. `can_tenant(...)` 只允许 tenant-wide grant，不接受 path-only grant。
11. Source 只读数据，确定性规则放在领域类型和 Engine 中。
12. 所有 serde 反序列化必须校验。
13. 缓存保存 effective grants，而不是裸 permission list。
14. 平台跨租户授权留给应用层或未来独立 feature。
15. super admin 留给应用层显式建模或未来独立 feature。

这些改动会破坏现有 API，但能把 v0.3 收敛成一个更小、更稳定、更难误用的授权内核。

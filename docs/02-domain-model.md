# 02. 领域模型与权限语义

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](01-overview.md) | [下一章](03-authorization-flow.md)

## 租户与主体

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

- `AuthSubject` 是 core 唯一主体上下文。
- 所有授权请求都必须有明确 `TenantId` 和 `PrincipalId`。
- ID 构造会 trim、校验非空、校验长度和字符集。
- serde 反序列化必须走同一套构造校验，不能绕过领域规则。

## 权限

```rust
pub struct Resource(String);
pub struct Action(String);

pub struct Permission {
    resource: Resource,
    action: Action,
}
```

默认字符串格式是 `resource:action`。

规则：

- `:` 只作为 `resource` 与 `action` 的分隔符。
- `resource` 内不再允许 `:`。
- 资源层级使用 `/`，例如 `billing/invoice:read`。
- 默认 normalize 为 trim + lowercase。
- wildcard 初期只支持完整 resource/action：
  - `*:*`
  - `invoice:*`
  - `*:read`
- 不支持 `billing/*:read` 这类层级 wildcard。

推荐 API：

```rust
let permission = Permission::parse("billing/invoice:read")?;
let resource = permission.resource();
let action = permission.action();
```

## 授权范围

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

三者含义不同：

- `ScopePath`：单个层级路径值对象，例如 `agent/123/store/456`。
- `GrantScope`：一次角色分配授予的范围，必须显式给出。
- `AccessScope`：某次权限查询合并后的最终范围。

范围规则：

- `GrantScope::Tenant` 明确表示全租户范围。
- `GrantScope::Paths` 支持多个路径根；公开 API 使用 `ScopeRoots` 保证空 paths 被拒绝，并在构造时完成去重和祖先覆盖压缩。
- `AccessScope::Tenant` 覆盖所有路径。
- 多个 `AccessScope::Paths` 合并时会去重，并删除已被祖先路径覆盖的子路径。
- 没有匹配授权时返回 `AccessScope::None`。
- `ScopePath::allows(target)` 使用相等或祖先路径规则。

## 角色与角色分配

```rust
pub struct RoleId(String);

pub struct RoleAssignment {
    pub role: RoleId,
    pub scope: GrantScope,
}
```

关键语义：

- 范围绑定到 role assignment。
- 角色继承得到的父角色权限，沿用当前 assignment 的 scope。
- `accessible_scope(permission)` 只合并权限命中的 assignments 的 scope。
- 不再使用 membership scope 推导范围。

建议同一个 role 中的权限共享同一种范围维度。如果 `invoice:read` 和 `store:manage` 需要完全不同的范围维度，应拆成不同 role。

## 授权请求

```rust
pub struct ScopeQuery {
    pub subject: AuthSubject,
    pub permission: Permission,
}

pub struct ScopedAccessRequest {
    pub subject: AuthSubject,
    pub permission: Permission,
    pub target: ScopePath,
}

pub struct TenantAccessRequest {
    pub subject: AuthSubject,
    pub permission: Permission,
}
```

选择规则：

- 查询列表或搜索：`ScopeQuery`。
- 判断某个有层级归属的业务对象：`ScopedAccessRequest`。
- 租户设置、租户级报表这类无下级目标路径操作：`TenantAccessRequest`。

`ScopeQuery` 必须带完整 `Permission`，不能只带 `Resource`，避免 `read` / `delete` / `manage` 共用错误范围。

## 授权结果

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

主流程优先使用：

- `AccessScope` 表示可访问范围。
- `AccessDecision` 表示是否允许。
- `DenyReason` 只用于 explain、测试和日志辅助，不建议作为业务分支的主要输入。

## 平台领域模型

v0.4.0 在 `platform` feature 下新增平台领域模型。它复用 `Permission`、`TenantId`、`ScopePath`、`ScopeRoots`、`AccessDecision` 和 `SourceError`，但不复用租户内主体、角色、Source 或范围结果。

### 平台主体

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

规则：

- `PlatformSubject` 不携带 `TenantId`。
- 平台主体不是租户成员，也不会绕过租户内 `membership_status`。
- 平台主体状态由 `PlatformAuthorizationSource::platform_principal_status` 读取。

### 平台角色

```rust
pub struct PlatformRoleId(String);

pub struct PlatformRoleAssignment {
    pub role: PlatformRoleId,
    pub scope: PlatformGrantScope,
}
```

规则：

- `PlatformRoleId` 与租户内 `RoleId` 隔离。
- 平台角色只服务平台主体。
- 平台角色不作为租户角色的父角色，也不参与租户内 `Engine` 的角色继承。

### 平台授权范围

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

### 平台授权结果

```rust
pub enum TenantDataAccessScope {
    None,
    AllTenants,
    Tenants { tenants: Vec<TenantId> },
    TenantPaths { entries: Vec<TenantScopedRoots> },
}
```

`TenantDataAccessScope` 是平台账号管理租户数据时的查询前过滤结果，不是租户内 `AccessScope` 的扩展。业务仓储应把它下推到 SQL、ORM 或搜索条件。

同一 permission 下不要同时授予 `Tenants` 与 `TenantPaths`。v0.4.0 的 `TenantDataAccessScope` 不表达“部分租户全量 + 部分租户路径”的混合结果，`PlatformEngine` 会把这种组合视为无效范围配置。

平台自身资源、指定租户、指定租户路径的点判定仍返回 `AccessDecision`。

## 继续阅读

- [上一章：01. 项目总览](01-overview.md)
- [下一章：03. 授权流程详解](03-authorization-flow.md)
- [11. 平台授权](11-platform-authorization.md)
- [返回目录](SUMMARY.md)

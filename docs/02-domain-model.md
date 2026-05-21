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

## 继续阅读

- [上一章：01. 项目总览](01-overview.md)
- [下一章：03. 授权流程详解](03-authorization-flow.md)
- [返回目录](SUMMARY.md)

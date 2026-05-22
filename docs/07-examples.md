# 07. 常见业务场景

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](06-axum-integration.md) | [下一章](08-testing-benchmark.md)

本章把常见 SaaS 授权场景翻译成 `rs-tenant` 的建模方式。

## 租户管理员

租户管理员不是特殊用户，而是普通角色 + 全租户范围。

```rust
use rs_tenant::{EngineBuilder, GrantScope, Permission};

source.add_role_assignment(
    tenant.clone(),
    principal.clone(),
    role.clone(),
    GrantScope::tenant(),
);
source.add_role_permission(tenant, role, Permission::parse("*:*")?);

let engine = EngineBuilder::new(source).enable_wildcard(true).build();
```

它仍然受租户状态和成员状态限制。租户禁用或成员禁用后不会放行。

## 门店员工只能看本门店订单

角色分配写门店路径：

```rust
use rs_tenant::{GrantScope, Permission, ScopePath};

source.add_role_assignment(
    tenant.clone(),
    principal.clone(),
    role.clone(),
    GrantScope::paths(vec![ScopePath::parse("agent/1/store/9")?])?,
);
source.add_role_permission(tenant, role, Permission::parse("order:read")?);
```

订单路径示例：

```text
agent/1/store/9/order/10001
```

`can_access_scope` 会允许这个订单，拒绝 `agent/1/store/10/order/10002`。

## 区域经理看多个门店

直接给多个路径：

```rust
GrantScope::paths(vec![
    ScopePath::parse("agent/1/store/9")?,
    ScopePath::parse("agent/1/store/10")?,
])?
```

如果你的业务天然有区域层级，也可以给区域根：

```text
region/east
```

只要业务对象路径在这个根下面，就会被覆盖。

## 同一个人不同权限不同范围

给同一个主体分配多个角色：

```text
role: order_reader
permission: order:read
scope: agent/1

role: order_editor
permission: order:update
scope: agent/1/store/9
```

查询 `order:read` 会得到更大的范围；查询 `order:update` 会得到更小的范围。`ScopeQuery` 必须传完整 `Permission`，不能只按 resource 查询。

## 租户级设置不能用路径级权限

如果某人只有：

```text
permission: tenant/settings:update
scope: agent/1/store/9
```

调用 `can_tenant` 仍然会拒绝。租户级操作必须授予 `GrantScope::tenant()`。

这是为了避免路径级员工通过“没有目标路径”的接口绕过范围控制。

## 列表接口返回空还是 403

`accessible_scope` 返回 `AccessScope::None` 时，两种处理都可以：

- 列表、搜索、报表：通常返回空列表。
- 明确需要权限才能进入的页面：可以返回 403。

关键是不要继续执行无范围查询。

## 数据库如何保存路径

常见做法：

- 在业务表保存 `scope_path` 字符串。
- 用组织树闭包表查询目标是否在 roots 下。
- PostgreSQL 使用 `ltree`。
- 搜索引擎里保存可过滤的路径前缀字段。

`rs-tenant` 不限制存储方式，只要求你能把 `AccessScope::Paths { roots }` 转成安全的查询条件。

## 平台客服查看指定租户数据

启用 `platform` 后，平台客服不需要伪装成租户成员。

```rust
use rs_tenant::{
    Permission, TenantId,
    platform::{
        MemoryPlatformSource, PlatformEngineBuilder, PlatformGrantScope, PlatformPrincipalId,
        PlatformPrincipalStatus, PlatformRoleId, PlatformSubject, TenantDataAccessRequest,
    },
};

let principal = PlatformPrincipalId::parse("support_1")?;
let role = PlatformRoleId::parse("tenant_support")?;
let tenant = TenantId::parse("tenant_a")?;
let permission = Permission::parse("tenant/order:read")?;

let source = MemoryPlatformSource::new();
source.set_principal_status(principal.clone(), PlatformPrincipalStatus::Active);
source.add_role_assignment(
    principal.clone(),
    role.clone(),
    PlatformGrantScope::tenants(vec![tenant.clone()])?,
);
source.add_role_permission(role, permission.clone());

let engine = PlatformEngineBuilder::new(source).build();
let decision = engine
    .can_access_tenant(TenantDataAccessRequest {
        subject: PlatformSubject::new(principal),
        permission,
        tenant,
    })
    .await?;
```

平台授权不等于 super admin。平台范围仍然绑定到具体 permission。

## 运维救援怎么做

运维救援通常还需要审批、原因、过期时间、审计日志和风险提示。建议由应用层单独建模，再选择一种授权接入方式：

- 启用 `platform`，把救援人员建成平台主体，并授予有时效的平台角色。
- 不启用 `platform`，由应用层临时写入租户内 membership 和 role assignment。

不要在业务代码里加“如果是运维人员就直接 allow”的全局分支。

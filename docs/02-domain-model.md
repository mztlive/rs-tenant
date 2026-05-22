# 02. 把业务概念映射到授权模型

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](01-overview.md) | [下一章](03-authorization-flow.md)

接入前先把业务系统里的概念映射到 `rs-tenant` 的类型。映射清楚后，后面的数据库表、接口调用和测试都会简单很多。

## 租户和主体

业务里的租户 id 对应 `TenantId`，登录后的用户、员工或账号 id 对应 `PrincipalId`。

```rust
use rs_tenant::{AuthSubject, PrincipalId, TenantId};

let subject = AuthSubject::new(
    TenantId::parse("tenant_a")?,
    PrincipalId::parse("user_1")?,
);
```

`AuthSubject` 总是同时包含租户和主体。租户内授权没有“脱离租户的用户权限”。

你的数据源还需要返回两个状态：

- `TenantStatus::Active | Inactive`
- `MembershipStatus::Active | Inactive`

任意一个不是 `Active`，授权都会拒绝。

## 权限

权限使用 `resource:action` 字符串。

```rust
use rs_tenant::Permission;

let permission = Permission::parse("order:read")?;
let settings = Permission::parse("tenant/settings:update")?;
```

建议规则：

- resource 写业务资源，如 `order`、`customer`、`tenant/settings`。
- action 写动作，如 `read`、`create`、`update`、`delete`、`export`。
- 使用小写语义。
- 资源层级用 `/`，不要用额外的 `:`。

如果开启 wildcard，只支持完整 resource/action 级别：

- `*:*`
- `order:*`
- `*:read`

不支持 `order/*:read` 这类层级 wildcard。

## 角色分配和授权范围

角色本身只代表一组权限；范围绑定在“某人被分配某角色”这件事上。

```rust
use rs_tenant::{GrantScope, RoleAssignment, RoleId, ScopePath};

let tenant_admin = RoleAssignment::new(
    RoleId::parse("tenant_admin")?,
    GrantScope::tenant(),
);

let store_reader = RoleAssignment::new(
    RoleId::parse("store_reader")?,
    GrantScope::paths(vec![ScopePath::parse("agent/1/store/9")?])?,
);
```

这样同一个人可以在一个权限上拥有全租户范围，在另一个权限上只拥有某些门店范围。

## ScopePath 怎么设计

`ScopePath` 是业务对象归属的层级路径。它不要求固定层级，只要求你的业务保持一致。

常见设计：

```text
agent/1
agent/1/store/9
agent/1/store/9/order/10001
region/east/city/shanghai/store/9
org/finance/team/ap
```

路径匹配规则很简单：授权根路径等于目标路径，或者是目标路径的祖先，就允许。

```text
grant:  agent/1/store/9
target: agent/1/store/9/order/10001
result: allow
```

不要直接信任客户端传入的路径。访问订单时，应先从数据库查出订单所属门店或组织路径，再把真实路径传给 `can_access_scope`。

## 三种范围类型

| 类型 | 出现在哪里 | 含义 |
|---|---|---|
| `ScopePath` | 业务对象归属 | 一个层级路径值 |
| `GrantScope` | 角色分配 | 这次分配授予全租户还是部分路径 |
| `AccessScope` | 引擎结果 | 某次权限查询最终可访问范围 |

`GrantScope::paths` 会拒绝空列表，并压缩重复或被祖先覆盖的路径。

## 数据源需要提供什么

租户内数据源实现 `AuthorizationSource`，它只读取五类数据：

1. 租户状态。
2. 主体在租户内的成员状态。
3. 主体的角色分配和每个分配的 `GrantScope`。
4. 角色拥有的权限。
5. 角色继承关系。

不要把权限匹配、wildcard、角色继承展开、范围合并写进数据源。这些是引擎的工作。

## 平台模型什么时候用

如果你的系统有平台员工，例如客服、运营、内部管理员，需要访问平台后台资源或跨租户数据，就启用 `platform` feature。

平台模型不要复用租户内类型：

| 租户内 | 平台 |
|---|---|
| `AuthSubject` | `PlatformSubject` |
| `RoleId` | `PlatformRoleId` |
| `GrantScope` | `PlatformGrantScope` |
| `AuthorizationSource` | `PlatformAuthorizationSource` |
| `Engine` | `PlatformEngine` |

平台授权详见 [11. 平台授权：平台员工和跨租户数据](11-platform-authorization.md)。

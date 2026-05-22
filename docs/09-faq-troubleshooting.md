# 09. FAQ 与故障排查

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](08-testing-benchmark.md) | [下一章](10-rs-tenant-vs-casbin.md)

## 有角色为什么还是 `AccessScope::None`？

按这个顺序查：

1. `tenant_status` 是否返回 `TenantStatus::Active`。
2. `membership_status` 是否返回 `MembershipStatus::Active`。
3. `role_assignments` 是否返回了该主体的角色分配。
4. 每个 assignment 是否有正确的 `GrantScope`。
5. `role_permissions` 是否包含请求的完整 `Permission`。
6. 如果权限使用 `*:*`、`order:*` 或 `*:read`，是否开启了 `enable_wildcard(true)`。
7. 如果权限来自父角色，是否开启了 `enable_role_hierarchy(true)`。

## 为什么列表接口不能只按 resource 查范围？

同一个 resource 的不同 action 可以有不同范围：

```text
order:read   -> agent/1
order:update -> agent/1/store/9
order:delete -> none
```

所以 `ScopeQuery` 必须传完整 `Permission`。

## 为什么 `can_tenant` 拒绝了路径级授权？

`can_tenant` 只接受最终范围为 `AccessScope::Tenant` 的授权。路径级范围不能执行租户级操作。

如果操作对象有路径，用 `can_access_scope`。如果是列表，用 `accessible_scope`。

## 怎么做租户管理员？

使用普通 RBAC：

```text
role: tenant_admin
permission: *:*
assignment scope: GrantScope::tenant()
```

然后构建引擎时开启 wildcard：

```rust
let engine = EngineBuilder::new(source).enable_wildcard(true).build();
```

这不是 super admin。租户禁用或成员禁用仍然拒绝。

## 数据库错误应该返回 403 吗？

不应该。`AuthorizationSource` 错误是系统错误，通常映射为 500，并记录日志。

403 表示认证通过但授权拒绝。数据库超时、连接失败、反序列化失败都不是“没有权限”。

## 权限更新后为什么没生效？

如果启用了缓存，先确认是否调用了正确的失效入口：

- 成员状态或角色分配变化：`invalidate_principal`。
- 角色权限变化：`invalidate_role`。
- 租户禁用：`invalidate_tenant`。
- 影响范围不清楚：`invalidate_all`。

排查时可以先临时禁用缓存，确认数据源和引擎规则本身正确。

## wildcard 为什么没匹配？

检查两点：

1. `EngineBuilder::enable_wildcard(true)` 是否开启。
2. wildcard 是否是支持的形态：`*:*`、`resource:*`、`*:action`。

`billing/*:read` 这类层级 wildcard 不支持。

## 角色继承为什么没生效？

检查：

- 是否开启 `enable_role_hierarchy(true)`。
- `parent_roles` 是否返回直接父角色。
- 父角色权限是否能通过 `role_permissions` 读取。
- 是否超过 `max_role_depth`。
- 是否存在角色环。

父角色权限继承后，范围仍然使用当前 assignment 的 `GrantScope`。

## Axum 里 401、403、500 怎么分？

| 状态码 | 含义 |
|---|---|
| 401 | 没有认证信息，或 token/session 无效 |
| 403 | 认证成功，但 `rs-tenant` 返回拒绝 |
| 500 | 数据源、业务查询或系统错误 |

## 平台主体能不能直接调用租户内 `Engine`？

不能。租户内 `Engine` 需要 `AuthSubject { tenant, principal }`，并且会检查租户成员状态。

平台员工应使用 `PlatformEngine`。如果你不启用 `platform`，应用层必须显式创建有时效的租户 membership 和 role assignment，再按普通租户成员授权。

## 为什么 v0.4 仍然没有 SuperAdmin？

super admin 是绕过策略，不是 RBAC 范围。把它放进 core 会模糊三种不同场景：

- 租户内管理员。
- 平台后台管理员。
- 运维救援。

推荐做法：

- 租户内管理员：普通角色 + `GrantScope::tenant()`。
- 平台后台：`platform` feature + `PlatformGrantScope::platform()`。
- 跨租户数据：`platform` feature + `AllTenants`、`Tenants` 或 `TenantPaths`。
- 运维救援：应用层维护审批、原因、过期时间和审计日志。

## 日志建议记录什么？

租户内授权至少记录：

- tenant
- principal
- permission
- request kind
- decision
- access scope
- deny reason
- wildcard / role hierarchy / max depth
- cache hit / miss

平台授权额外记录：

- platform principal
- platform request kind
- tenant data access scope
- platform engine config

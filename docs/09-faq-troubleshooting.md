# 09. FAQ 与故障排查

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](08-testing-benchmark.md) | [下一章](10-rs-tenant-vs-casbin.md)

## Q1: 为什么有角色还是返回 `AccessScope::None`？

按顺序检查：

1. `tenant_status` 是否返回 `TenantStatus::Active`。
2. `membership_status` 是否返回 `MembershipStatus::Active`。
3. `role_assignments` 是否返回了至少一个 assignment。
4. assignment 是否带了显式 `GrantScope`。
5. `role_permissions` 是否包含请求的完整 `Permission`。
6. wildcard 是否已开启。
7. 角色继承是否已开启，以及父角色是否能正确读取。

## Q2: 为什么列表接口不能只按 resource 查询范围？

因为不同 action 可能有不同授权范围：

- `order:read` 允许两个门店。
- `order:update` 只允许一个门店。
- `order:delete` 完全不允许。

所以 `ScopeQuery` 必须带完整 `Permission`，不能只带 `Resource`。

## Q3: 为什么 `can_tenant` 拒绝了路径级授权？

这是 v0.3 的刻意设计。`can_tenant` 只允许 `AccessScope::Tenant`，不接受 `AccessScope::Paths`。

如果操作目标是某个门店、区域、客户、订单，请使用 `can_access_scope`。如果是列表或搜索，请使用 `accessible_scope`。

## Q4: 如何做租户内管理员？

用普通 RBAC 表达：

```text
role: tenant_admin
permission: *:*
assignment scope: GrantScope::Tenant
```

然后在 `EngineBuilder` 开启 wildcard。这样管理员仍然受 tenant active 和 membership active 约束。

## Q5: v0.4 有了 platform，为什么仍然没有 SuperAdmin？

super admin 是绕过策略，不是 RBAC 授权范围。把它放进 core 会模糊平台 super admin、租户 super admin、运维救援三种不同风险面。

推荐：

- 租户内管理员：普通角色 + `GrantScope::Tenant`。
- 平台后台权限：启用 `platform` feature，用 `PlatformGrantScope::Platform`。
- 平台跨租户数据管理：启用 `platform` feature，用 `AllTenants`、`Tenants` 或 `TenantPaths` 表达数据范围。
- 运维救援：应用层记录审批、原因、过期时间和审计日志。

`PlatformGrantScope::AllTenants` 不是全局绕过；它只对某个 `Permission` 表示平台主体的租户数据范围。

## Q6: 权限更新后为什么行为没变化？

高概率是缓存未失效。按变更范围调用：

- `invalidate_principal(tenant, principal)`
- `invalidate_role(tenant, role)`
- `invalidate_tenant(tenant)`
- `invalidate_all()`

如果暂时没有可靠失效入口，先禁用缓存，确认授权正确性后再启用。

## Q7: `AuthorizationSource` 应该实现哪些规则？

只实现数据读取：

- tenant status
- membership status
- role assignments
- role permissions
- parent roles

不要在 Source 里实现 permission match、wildcard、role 展开、scope 合并或 path allows。这些规则属于领域类型和 Engine。

## Q8: Axum 中的 `401`、`403`、`500` 怎么区分？

- `401`：认证信息缺失或无效，无法构造 `AuthSubject`。
- `403`：认证通过，但授权结果是拒绝。
- `500`：`AuthorizationSource` 或业务数据读取异常。

## Q9: 平台主体能不能直接调用租户内 `Engine`？

不能。租户内 `Engine` 的主体是 `AuthSubject { tenant, principal }`，必须经过 tenant status 和 membership status。平台主体是 `PlatformSubject { principal }`，应交给 `PlatformEngine`。

如果业务确实要让平台客服临时代查某个租户内数据，有两种清晰做法：

1. 启用 `platform` feature，用 `accessible_tenants` 或 `can_access_tenant_scope` 计算平台数据范围。
2. 不启用 `platform` 时，由应用层显式创建有时效的租户内 membership / role assignment，再按普通 `AuthSubject` 调用租户内 `Engine`。

不要在 `Engine` 外层加“如果是平台用户就直接 allow”的全局分支。

## Q10: `Platform`、`AllTenants`、`Tenants`、`TenantPaths` 有什么区别？

- `Platform`：只访问平台自身资源，例如平台角色管理。
- `AllTenants`：访问所有租户的数据管理范围。
- `Tenants`：访问明确租户集合的数据。
- `TenantPaths`：访问指定租户下指定路径 roots 及其子孙。

`can_platform` 只接受 `Platform`。`can_access_tenant` 不会把 `TenantPaths` 当成租户级权限。`can_access_tenant_scope` 才会检查路径覆盖。

## 排查日志建议

至少记录：

- tenant
- principal
- permission
- request kind：`scope_query` / `scoped_access` / `tenant_access`
- decision
- access scope
- deny reason
- engine config：wildcard、role hierarchy、max depth
- cache hit / miss

平台授权还应记录：

- platform principal
- platform request kind：`platform_access` / `tenant_data_scope` / `tenant_data_access` / `tenant_scoped_data_access`
- tenant data access scope
- platform engine config：wildcard、role hierarchy、max depth

## 继续阅读

- [上一章：08. 测试与性能基准](08-testing-benchmark.md)
- [下一章：10. Casbin 边界](10-rs-tenant-vs-casbin.md)
- [11. 平台授权](11-platform-authorization.md)
- [返回目录](SUMMARY.md)

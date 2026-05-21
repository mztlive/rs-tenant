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

## Q5: v0.3 为什么没有 SuperAdmin？

super admin 是绕过策略，不是租户内 RBAC 基础概念。把它放进 core 会模糊平台 super admin、租户 super admin、运维救援三种不同风险面。

推荐：

- 租户内管理员：普通角色 + `GrantScope::Tenant`。
- 运维救援：应用层显式创建临时 membership / role assignment，并记录审批与审计。
- 平台超级管理员：应用层平台权限系统处理，未来如有需要也应是独立 feature。

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

## 继续阅读

- [上一章：08. 测试与性能基准](08-testing-benchmark.md)
- [下一章：10. Casbin 边界](10-rs-tenant-vs-casbin.md)
- [返回目录](SUMMARY.md)

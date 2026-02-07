# 02. 领域模型与权限语义

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](01-overview.md) | [下一章](03-authorization-flow.md)

## 核心对象

- `Tenant`：租户
- `Principal`：主体（用户/服务账号）
- `Role`：租户内角色
- `GlobalRole`：平台级全局角色
- `Permission`：权限字符串，格式 `resource:action`
- `Decision`：`Allow` 或 `Deny`
- `Scope`：`TenantOnly { tenant }` 或 `None`

这些对象在库内都用强类型 ID 包装（如 `TenantId`、`PrincipalId`），减少字符串混用导致的授权错误。

## 权限字符串规则

默认校验器要求：

1. 必须是 `resource:action` 格式
2. 默认会 `trim + 小写化`
3. 不允许空段
4. 默认字符集：`a-z`、`0-9`、`_`、`-`、`:`

示例：

- 合法：`invoice:read`
- 合法：`billing:export`
- 非法：`invoice`（缺少 action）
- 非法：`invoice:READ?`（非法字符）

## 通配权限语义（可选）

只有在 `enable_wildcard(true)` 时才生效：

- `*:*`：任意资源任意动作
- `invoice:*`：`invoice` 资源任意动作
- `*:read`：任意资源的 `read`

关闭 wildcard 时，带 `*` 的权限不会命中授权。

## 租户角色与平台角色如何合并

授权时会收集两类权限并取并集：

1. 租户角色权限（来自 `RoleStore`）
2. 全局角色权限（来自 `GlobalRoleStore`）

只要任意一条权限命中，就 `Allow`。

## 超级管理员语义

超级管理员是平台级能力，不按租户存储；但仍受租户启用状态约束。

生效条件：

1. `EngineBuilder::enable_super_admin(true)` 已开启
2. `Store::is_super_admin(principal)` 返回 `true`
3. 当前租户 `tenant_active == true`

命中后会跳过普通角色权限计算，直接 `Allow`（或 `scope` 返回 `TenantOnly`）。

## 继续阅读

- [上一页：01. 项目总览](01-overview.md)
- [下一页：03. 授权流程详解](03-authorization-flow.md)
- [返回目录](SUMMARY.md)

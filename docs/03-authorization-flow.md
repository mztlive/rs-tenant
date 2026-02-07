# 03. 授权流程详解

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](02-domain-model.md) | [下一章](04-quickstart.md)

本章解释引擎执行时的真实顺序，方便你定位“为什么被拒绝”。

## `authorize` 执行顺序

调用：`authorize(tenant, principal, permission)`

1. 检查 `tenant_active(tenant)`
2. 如果开启 super-admin，检查 `is_super_admin(principal)`
3. 检查 `principal_active(tenant, principal)`
4. 计算有效权限（租户角色 + 全局角色，必要时展开继承）
5. 按 wildcard / normalize 配置匹配权限
6. 命中返回 `Allow`，否则 `Deny`

其中第 1、2、3 步是短路逻辑：前置失败时不会继续执行后续步骤。

## `scope` 执行顺序

调用：`scope(tenant, principal, resource)`

前置检查与 `authorize` 一致，区别是匹配维度变为“资源级”：

- 若有资源可访问权限，返回 `Scope::TenantOnly { tenant }`
- 否则返回 `Scope::None`

## 角色继承展开规则

当 `enable_role_hierarchy(true)` 打开时，引擎会递归展开继承关系。

- `max_inherit_depth(n)`：最大展开深度，超出会报 `RoleDepthExceeded`
- 遇到环依赖会报 `RoleCycleDetected`

建议：
- 深度配置在 `8~32` 范围
- 对角色关系做变更审核，避免线上出现继承环

## 缓存参与点

当配置 `cache(...)` 后，引擎会缓存“主体在租户下的有效权限集”。

- 命中缓存：跳过 Store 读取，直接匹配
- 未命中：读取 Store 并回填缓存

权限变更后需主动失效：

- `invalidate_principal(tenant, principal)`
- `invalidate_role(tenant, role)`
- `invalidate_tenant(tenant)`

## 常见拒绝原因排查

1. `tenant_active = false`
2. `principal_active = false`
3. wildcard 规则未开启
4. 权限字符串格式错误（不是 `resource:action`）
5. 角色继承深度过小或存在环
6. 缓存未失效导致读取旧权限

## 继续阅读

- [上一页：02. 领域模型与权限语义](02-domain-model.md)
- [下一页：04. 5 分钟快速接入](04-quickstart.md)
- [返回目录](SUMMARY.md)

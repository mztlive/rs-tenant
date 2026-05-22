# 01. 先理解它解决什么问题

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [下一章](02-domain-model.md)

`rs-tenant` 适合已经有认证系统和业务数据表的 Rust SaaS 服务。它不负责登录，也不负责维护角色；它只负责在请求进来后，根据你的授权数据算出“能不能访问”和“能访问哪里”。

## 一个典型请求

假设你的接口是“查询订单列表”：

1. Web 层完成认证，得到 `tenant_id = tenant_a` 和 `principal_id = user_1`。
2. 业务代码要检查 `order:read`。
3. `rs-tenant` 从你的数据源读取租户状态、成员状态、角色分配和角色权限。
4. 引擎返回 `AccessScope`。
5. 订单仓储把 `AccessScope` 转成查询条件。

结果可能是：

```rust
AccessScope::None
AccessScope::Tenant { tenant }
AccessScope::Paths { tenant, roots }
```

调用方负责决定怎么处理：

- `None`：返回空列表或拒绝。
- `Tenant`：查询该租户下所有数据。
- `Paths`：只查询指定层级路径下的数据。

## 它和认证的关系

认证回答：“这个请求是谁发的？”

授权回答：“这个主体在这个租户里能做什么？”

所以接入时的边界应该是：

1. 你的认证层解析 Session、JWT 或网关注入信息。
2. 认证层构造 `AuthSubject`。
3. 业务层调用 `Engine`。
4. 业务仓储按授权结果查询数据。

`rs-tenant` 不解析密码，不签发 token，也不替你决定 HTTP 401。

## 租户内授权和平台授权

默认核心是租户内授权：

```text
tenant + principal + permission -> access scope / decision
```

它使用：

- `AuthSubject`
- `RoleId`
- `GrantScope`
- `AuthorizationSource`
- `Engine`

启用 `platform` feature 后，可以额外使用平台授权：

```text
platform principal + permission -> platform decision / tenant data scope
```

它使用：

- `PlatformSubject`
- `PlatformRoleId`
- `PlatformGrantScope`
- `PlatformAuthorizationSource`
- `PlatformEngine`

平台授权是并列能力，不是租户内授权的 bypass。平台员工不会自动成为所有租户成员。

## Deny by default

默认拒绝是这个 crate 的基本安全假设。以下情况都会拒绝或返回空范围：

- 租户不存在或不是 `Active`。
- 主体不是该租户的 `Active` 成员。
- 没有角色分配。
- 角色没有请求的权限。
- 角色分配没有覆盖目标范围。
- 数据源读取失败。

数据源读取失败会以错误返回，调用方应 fail closed，不要在错误时放行。

## 什么时候不适合用

不要把 `rs-tenant` 当作以下系统：

- 通用策略语言。
- ORM 或数据库迁移工具。
- 用户、租户、角色管理后台。
- 审计日志系统。
- super admin 全局绕过框架。
- Casbin 的完整替代品。

如果你的核心需求是复杂 ABAC、动态 matcher 或跨语言策略统一，应该先评估 Casbin 或其他策略引擎。

## 下一步

继续看 [02. 把业务概念映射到授权模型](02-domain-model.md)，先把你的租户、用户、角色、权限和数据范围对应到 crate 的类型。

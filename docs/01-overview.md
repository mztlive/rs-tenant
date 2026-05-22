# 01. 项目总览

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [下一章](02-domain-model.md)

## 项目定位

`rs-tenant` v0.4.0 是面向 Rust SaaS 系统的 RBAC 授权内核。默认核心仍是 v0.3.0 的租户内 RBAC；启用 `platform` feature 后，新增平台授权子域。

租户内授权的输入是：

```text
tenant + principal + permission -> access scope
```

它的输出是：

- `AccessScope`：用于查询前过滤。
- `AccessDecision`：用于目标路径或租户级操作的点判定。
- `AccessExplanation`：用于排查和测试的轻量解释。

平台授权的输入是：

```text
platform principal + permission -> platform decision / tenant data scope
```

它的输出是：

- `AccessDecision`：用于平台自身资源、指定租户或指定租户路径的点判定。
- `TenantDataAccessScope`：用于平台账号跨租户查询前过滤。

核心不提供 super admin 绕过，也不承担用户、租户、角色的后台管理职责。

## 核心能力

租户内核心聚焦以下能力：

1. 强类型租户内主体：`AuthSubject { tenant, principal }`
2. 强类型权限：`Permission = Resource + Action`
3. 角色分配范围：`RoleAssignment { role, scope: GrantScope }`
4. 查询前范围计算：`Engine::accessible_scope(ScopeQuery)`
5. 路径目标判定：`Engine::can_access_scope(ScopedAccessRequest)`
6. 租户级判定：`Engine::can_tenant(TenantAccessRequest)`
7. 单一授权数据源：`AuthorizationSource`
8. 可选内存实现与缓存：`MemorySource`、`MemoryCache`

v0.4.0 在 `platform` feature 下新增以下能力：

1. 平台主体：`PlatformSubject { principal }`
2. 平台角色：`PlatformRoleId`、`PlatformRoleAssignment`
3. 平台范围：`PlatformGrantScope::Platform | AllTenants | Tenants | TenantPaths`
4. 平台自身资源判定：`PlatformEngine::can_platform`
5. 租户数据范围查询：`PlatformEngine::accessible_tenants`
6. 指定租户判定：`PlatformEngine::can_access_tenant`
7. 指定租户路径判定：`PlatformEngine::can_access_tenant_scope`
8. 独立平台数据源：`PlatformAuthorizationSource`

## 适用场景

- SaaS 系统里的租户内角色授权。
- 平台后台自身资源授权。
- 平台客服、运营或运维账号管理明确租户集合或租户路径下的数据。
- 门店、区域、代理、组织树等层级数据范围控制。
- 列表接口需要先计算可见范围，再下推到数据查询。
- 服务已有自己的身份认证和业务数据表，只需要授权内核。

## 不适用场景

- 平台或租户 super admin 直接绕过授权。
- 把平台主体自动伪装成所有租户成员。
- 让平台角色继承租户角色，或让租户角色继承平台角色。
- Casbin matcher 这类通用策略语言。
- ORM、数据库迁移、审计日志落库。

平台授权是独立 `platform` feature，不混入租户内 `Engine`。

## 设计原则

### 1) Deny by default

任何缺省、缺失、无法解释的授权数据都拒绝：

- 租户不存在或不可用：拒绝。
- 主体不是有效租户成员：拒绝。
- 没有角色分配：拒绝。
- 权限未命中：拒绝。
- 角色分配没有显式范围：拒绝。
- `AuthorizationSource` 读取失败：返回错误，由调用方 fail closed。

### 2) 范围绑定到角色分配

同一个主体可能拥有多个权限和多个范围。例如：

- `invoice:read` 是全租户。
- `order:read` 只覆盖两个门店。
- `customer:update` 只覆盖某个区域。

因此 v0.3 把范围绑定到 `RoleAssignment`，而不是绑定到 membership 或 principal。

### 3) 避免裸权限判定误用

v0.3 不提供语义含糊的裸 `can(...)`。调用方必须选择：

- 查询列表：用 `accessible_scope(...)`。
- 访问某个层级对象：用 `can_access_scope(...)`。
- 执行租户级操作：用 `can_tenant(...)`。

`can_tenant(...)` 只有在最终范围是 `AccessScope::Tenant` 时才允许，路径级授权不会被当成全租户授权。

### 4) 平台授权与租户授权保持 sibling 关系

v0.4.0 共享 `Permission`、`TenantId`、`ScopePath`、`ScopeRoots`、`AccessDecision` 等基础值对象，但隔离运行时语义：

- 租户内授权：`Engine` / `AuthSubject` / `AuthorizationSource` / `AccessScope`
- 平台授权：`PlatformEngine` / `PlatformSubject` / `PlatformAuthorizationSource` / `TenantDataAccessScope`

`PlatformGrantScope::AllTenants` 只表示平台数据管理范围，不是 `Engine` 的全局 bypass。

## v0.3 删除项

v0.3.0 不保留以下旧 API 或兼容层：

- `authorize(...)`
- `scope(...)`
- 旧 `Scope`
- `Store` / `TenantStore` / `RoleStore` / `GlobalRoleStore` / `ScopeStore`
- `GlobalRoleId` / `GlobalRole`
- `SuperAdminMode` / `enable_super_admin(...)` / `is_super_admin(...)`
- 空的 `casbin` feature
- 公开 unchecked constructor

## v0.4 新增项

v0.4.0 新增 `platform` feature：

- 新增 `rs_tenant::platform` 模块。
- 新增 `PlatformEngine` 和 `PlatformEngineConfig`。
- 新增 `PlatformSubject`、`PlatformPrincipalId`、`PlatformPrincipalStatus`。
- 新增 `PlatformRoleId`、`PlatformRoleAssignment`。
- 新增 `PlatformGrantScope`、`TenantSet`、`TenantScopeRoots`、`TenantDataAccessScope`。
- 新增 `PlatformAuthorizationSource`。
- 在 `memory-store + platform` 下提供平台内存数据源，用于示例和测试。
- 在 `axum + platform` 下提供 `PlatformAuthorizeLayer` 和 `PlatformAuthContext`，用于平台自身资源路由授权。

这些新增项不改变 v0.3 租户内 API 的含义。

## 继续阅读

- [下一章：02. 领域模型与权限语义](02-domain-model.md)
- [11. 平台授权](11-platform-authorization.md)
- [返回目录](SUMMARY.md)

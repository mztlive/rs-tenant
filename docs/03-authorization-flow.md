# 03. 授权流程详解

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](02-domain-model.md) | [下一章](04-quickstart.md)

本章说明租户内 `Engine` 和平台 `PlatformEngine` 的核心调用如何执行。未启用 `platform` feature 时，只需要关注租户内流程。

## `accessible_scope(...)`

调用：

```rust
engine.accessible_scope(ScopeQuery { subject, permission }).await
```

执行顺序：

1. 读取 `tenant_status(subject.tenant)`。
2. 租户不是 `Active`：返回 `AccessScope::None`。
3. 读取 `membership_status(subject)`。
4. 成员不是 `Active`：返回 `AccessScope::None`。
5. 读取 `role_assignments(subject)`。
6. 没有角色分配：返回 `AccessScope::None`。
7. 按配置展开角色继承。
8. 读取角色权限，匹配 `query.permission`。
9. 只收集权限命中的 role assignments 的 `GrantScope`。
10. 合并命中范围：
    - 没有命中：`AccessScope::None`
    - 任一命中是 `GrantScope::Tenant`：`AccessScope::Tenant`
    - 只有 path grants：`AccessScope::Paths`

这个 API 适合列表、搜索、导出等需要把范围下推到数据查询的接口。

## `can_access_scope(...)`

调用：

```rust
engine
    .can_access_scope(ScopedAccessRequest {
        subject,
        permission,
        target,
    })
    .await
```

执行顺序：

1. 按 `request.permission` 调用 `accessible_scope(...)`。
2. `AccessScope::None`：返回 `AccessDecision::Deny`。
3. `AccessScope::Tenant`：返回 `AccessDecision::Allow`。
4. `AccessScope::Paths`：检查 `target` 是否被任一 root 覆盖。
5. 覆盖则 `Allow`，否则 `Deny`。

这个 API 适合读取、更新、删除某个有明确层级路径的业务对象。

## `can_tenant(...)`

调用：

```rust
engine
    .can_tenant(TenantAccessRequest { subject, permission })
    .await
```

执行顺序：

1. 按 `request.permission` 调用 `accessible_scope(...)`。
2. 只有 `AccessScope::Tenant` 返回 `AccessDecision::Allow`。
3. `AccessScope::Paths` 返回 `AccessDecision::Deny`。
4. `AccessScope::None` 返回 `AccessDecision::Deny`。

这个 API 刻意不把路径级授权当成租户级授权，避免调用方绕过目标范围。

适用：

- 租户设置。
- 租户级报表。
- 不绑定下级业务对象的操作。

不适用：

- 查看某个门店订单。
- 修改某个区域客户。
- 删除某个层级资源。

## `explain_*`

解释 API 与主 API 语义一致，但返回轻量解释：

```rust
pub struct AccessExplanation {
    pub decision: AccessDecision,
    pub reason: Option<DenyReason>,
    pub scope: AccessScope,
}
```

要求：

- 能定位短路点。
- 能区分权限缺失、需要目标范围、范围拒绝。
- `AuthorizationSource` 错误通过 `Err` 返回，不塞进 `DenyReason`。
- 不暴露敏感内部错误或数据库细节。

## 角色继承

当 `EngineBuilder` 开启角色继承时：

- Engine 负责展开父角色。
- 当前 assignment 的 `GrantScope` 会沿用到父角色权限。
- Engine 负责角色环检测和最大深度限制。
- `AuthorizationSource::parent_roles(...)` 只提供数据，不实现策略。

## 缓存参与点

`MemoryCache` 或自定义缓存应缓存 effective grants，而不是裸 `Vec<Permission>`。`EffectiveGrant` 只服务 `Cache` 扩展点，不作为业务解释模型。缓存 key 需要包含：

- tenant
- principal
- Engine 配置签名
- role hierarchy / wildcard / max depth 等影响结果的配置

建议每次授权前重新校验 tenant status 和 membership status，避免成员禁用后继续命中过期授权。

## `PlatformEngine::can_platform(...)`

调用：

```rust
platform_engine
    .can_platform(PlatformAccessRequest { subject, permission })
    .await
```

执行顺序：

1. 读取 `platform_principal_status(subject)`。
2. 平台主体不是 `Active`：返回 `AccessDecision::Deny`。
3. 读取 `platform_role_assignments(subject)`。
4. 没有平台角色分配：返回 `AccessDecision::Deny`。
5. 按 `PlatformEngineConfig` 展开平台角色继承。
6. 读取平台角色权限，匹配 `request.permission`。
7. 只允许命中 `PlatformGrantScope::Platform` 的 assignment。
8. 命中则 `Allow`，否则 `Deny`。

`AllTenants`、`Tenants`、`TenantPaths` 不会被当作平台自身资源权限。

## `PlatformEngine::accessible_tenants(...)`

调用：

```rust
platform_engine
    .accessible_tenants(TenantDataScopeQuery { subject, permission })
    .await
```

执行顺序：

1. 读取平台主体状态。
2. 主体不是 `Active`：返回 `TenantDataAccessScope::None`。
3. 读取平台角色分配。
4. 展开平台角色继承。
5. 读取平台角色权限，匹配 `query.permission`。
6. 收集权限命中的租户数据范围。
7. 合并命中范围：
   - 没有命中：`TenantDataAccessScope::None`
   - 任一命中是 `AllTenants`：`TenantDataAccessScope::AllTenants`
   - 命中租户集合：`TenantDataAccessScope::Tenants`
   - 命中路径集合：`TenantDataAccessScope::TenantPaths`
   - 同一 permission 同时命中租户集合与路径集合：返回错误；v0.4.0 的 `TenantDataAccessScope` 不表达混合结果。

这个 API 适合平台租户列表、跨租户业务列表、导出等需要把范围下推到业务查询的接口。

## `PlatformEngine::can_access_tenant(...)`

调用：

```rust
platform_engine
    .can_access_tenant(TenantDataAccessRequest {
        subject,
        permission,
        tenant,
    })
    .await
```

执行顺序：

1. 按 `request.permission` 调用 `accessible_tenants(...)`。
2. `TenantDataAccessScope::AllTenants`：返回 `AccessDecision::Allow`。
3. `TenantDataAccessScope::Tenants` 包含目标租户：返回 `AccessDecision::Allow`。
4. `TenantDataAccessScope::TenantPaths`：返回 `AccessDecision::Deny`，因为目标路径必需。
5. `TenantDataAccessScope::None` 或租户不匹配：返回 `AccessDecision::Deny`。

路径级平台授权不会被当成租户级平台授权。

## `PlatformEngine::can_access_tenant_scope(...)`

调用：

```rust
platform_engine
    .can_access_tenant_scope(TenantScopedDataAccessRequest {
        subject,
        permission,
        tenant,
        target,
    })
    .await
```

执行顺序：

1. 按 `request.permission` 调用 `accessible_tenants(...)`。
2. `TenantDataAccessScope::AllTenants`：返回 `AccessDecision::Allow`。
3. `TenantDataAccessScope::Tenants` 包含目标租户：返回 `AccessDecision::Allow`。
4. `TenantDataAccessScope::TenantPaths`：检查目标租户下的 roots 是否覆盖 `target`。
5. 覆盖则 `Allow`，否则 `Deny`。

平台路径判定复用 `ScopePath` 的相等或祖先路径规则。

## 平台角色继承

当 `PlatformEngineConfig` 开启角色继承时：

- `PlatformEngine` 负责展开父平台角色。
- 当前 `PlatformRoleAssignment` 的 `PlatformGrantScope` 会沿用到父角色权限。
- `PlatformEngine` 负责平台角色环检测和最大深度限制。
- `PlatformAuthorizationSource::platform_parent_roles(...)` 只提供数据，不实现策略。

平台角色继承不参与租户内 `Engine`。

## 继续阅读

- [上一章：02. 领域模型与权限语义](02-domain-model.md)
- [下一章：04. 5 分钟快速接入](04-quickstart.md)
- [11. 平台授权](11-platform-authorization.md)
- [返回目录](SUMMARY.md)

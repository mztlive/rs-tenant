# 05. 生产环境集成指南

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](04-quickstart.md) | [下一章](06-axum-integration.md)

生产接入的核心是实现 `AuthorizationSource`，然后把 `AccessScope` 下推到业务查询。启用 `platform` feature 时，平台授权需要额外实现独立的 `PlatformAuthorizationSource`，并把 `TenantDataAccessScope` 下推到平台业务查询。

## Step 1: 设计授权数据表

推荐最小逻辑表：

- `tenants(id, status)`
- `tenant_memberships(tenant_id, principal_id, status)`
- `tenant_role_assignments(tenant_id, principal_id, role_id, scope_kind, scope_paths)`
- `tenant_role_permissions(tenant_id, role_id, permission)`
- `tenant_role_inherits(tenant_id, role_id, parent_role_id)`

索引建议：

- 租户内表统一以 `(tenant_id, ...)` 作为前导索引。
- membership 使用 `(tenant_id, principal_id)` 唯一索引。
- role assignment 使用 `(tenant_id, principal_id)` 和 `(tenant_id, role_id)` 索引。
- role permission 使用 `(tenant_id, role_id)` 索引。

不再需要：

- 全局角色表。
- 平台 super admin 表作为 core 输入。
- membership scope 表。
- Casbin policy 表作为 core 兼容层。

v0.4.0 的平台管理能力通过 `platform` feature 接入 `PlatformEngine`，但仍不应该复用租户内表来表达平台主体。推荐额外设计平台授权表：

- `platform_principals(id, status)`
- `platform_role_assignments(principal_id, role_id, scope_kind, tenants, tenant_scope_roots)`
- `platform_role_permissions(role_id, permission)`
- `platform_role_inherits(role_id, parent_role_id)`

这些表不需要 `tenant_id` 前导索引，因为平台主体不属于某个租户。

## Step 2: 实现 `AuthorizationSource`

```rust
use async_trait::async_trait;
use rs_tenant::{
    AuthSubject, AuthorizationSource, MembershipStatus, Permission, RoleAssignment, RoleId,
    SourceError, TenantId, TenantStatus,
};

pub struct DbAuthorizationSource {
    // 例如数据库连接池
}

#[async_trait]
impl AuthorizationSource for DbAuthorizationSource {
    async fn tenant_status(
        &self,
        tenant: &TenantId,
    ) -> Result<TenantStatus, SourceError> {
        let _ = tenant;
        todo!()
    }

    async fn membership_status(
        &self,
        subject: &AuthSubject,
    ) -> Result<MembershipStatus, SourceError> {
        let _ = subject;
        todo!()
    }

    async fn role_assignments(
        &self,
        subject: &AuthSubject,
    ) -> Result<Vec<RoleAssignment>, SourceError> {
        let _ = subject;
        todo!()
    }

    async fn role_permissions(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> Result<Vec<Permission>, SourceError> {
        let _ = (tenant, role);
        todo!()
    }

    async fn parent_roles(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> Result<Vec<RoleId>, SourceError> {
        let _ = (tenant, role);
        Ok(vec![])
    }
}
```

`AuthorizationSource` 只读取授权数据，不实现 wildcard、角色继承、范围合并、路径覆盖等规则。这些确定性规则由领域类型和 Engine 维护。

## Step 2b: 可选实现 `PlatformAuthorizationSource`

启用 `platform` feature 后，平台 Source 仍然只读取授权数据：

```rust
use async_trait::async_trait;
use rs_tenant::{
    platform::{
        PlatformAuthorizationSource, PlatformPrincipalStatus, PlatformRoleAssignment,
        PlatformRoleId, PlatformSubject,
    },
    Permission, SourceError,
};

pub struct DbPlatformAuthorizationSource {
    // 例如数据库连接池
}

#[async_trait]
impl PlatformAuthorizationSource for DbPlatformAuthorizationSource {
    async fn platform_principal_status(
        &self,
        subject: &PlatformSubject,
    ) -> Result<PlatformPrincipalStatus, SourceError> {
        let _ = subject;
        todo!()
    }

    async fn platform_role_assignments(
        &self,
        subject: &PlatformSubject,
    ) -> Result<Vec<PlatformRoleAssignment>, SourceError> {
        let _ = subject;
        todo!()
    }

    async fn platform_role_permissions(
        &self,
        role: &PlatformRoleId,
    ) -> Result<Vec<Permission>, SourceError> {
        let _ = role;
        todo!()
    }

    async fn platform_parent_roles(
        &self,
        role: &PlatformRoleId,
    ) -> Result<Vec<PlatformRoleId>, SourceError> {
        let _ = role;
        Ok(vec![])
    }
}
```

`PlatformAuthorizationSource` 不负责创建租户、更新平台角色、绑定平台主体、写审计日志或检查业务数据是否存在。平台角色继承、环检测、最大深度限制由 `PlatformEngine` 负责。

## Step 3: 构建 Engine

```rust
use rs_tenant::{Engine, EngineBuilder, MemoryCache};

fn build_engine(
    source: DbAuthorizationSource,
) -> Engine<DbAuthorizationSource, MemoryCache> {
    EngineBuilder::new(source)
        .enable_role_hierarchy(true)
        .enable_wildcard(true)
        .max_role_depth(16)
        .cache(MemoryCache::new(100_000))
        .build()
}
```

平台授权使用 sibling `PlatformEngine`：

```rust
use rs_tenant::platform::{PlatformEngine, PlatformEngineBuilder};

fn build_platform_engine(
    source: DbPlatformAuthorizationSource,
) -> PlatformEngine<DbPlatformAuthorizationSource> {
    PlatformEngineBuilder::new(source).build()
}
```

平台配置项与租户内 `Engine` 独立：`enable_role_hierarchy`、`enable_wildcard`、`max_role_depth`。不要直接复用租户内 `EngineConfig`，避免后续平台配置和租户配置互相牵制。

默认建议：

- `enable_role_hierarchy(false)`：先关闭，除非业务确实需要继承。
- `enable_wildcard(false)`：先关闭，避免 `*:*` 过早扩散。
- `max_role_depth(16)`：开启继承时的默认上限。

## Step 4: 查询前过滤

```rust
use rs_tenant::{AccessScope, Permission, ScopeQuery};

pub async fn list_orders(
    engine: &Engine<DbAuthorizationSource, MemoryCache>,
    repo: &OrderRepo,
    subject: rs_tenant::AuthSubject,
) -> rs_tenant::Result<Vec<Order>> {
    let scope = engine
        .accessible_scope(ScopeQuery {
            subject,
            permission: Permission::parse("order:read")?,
        })
        .await?;

    match scope {
        AccessScope::None => Ok(vec![]),
        AccessScope::Tenant { tenant } => repo.list_by_tenant(tenant).await,
        AccessScope::Paths { tenant, roots } => repo.list_by_scope_roots(tenant, roots).await,
    }
}
```

## Step 5: 目标点判定

```rust
use rs_tenant::{AccessDecision, Permission, ScopePath, ScopedAccessRequest};

pub async fn update_order(
    engine: &Engine<DbAuthorizationSource, MemoryCache>,
    subject: rs_tenant::AuthSubject,
    order_scope: ScopePath,
) -> rs_tenant::Result<bool> {
    let decision = engine
        .can_access_scope(ScopedAccessRequest {
            subject,
            permission: Permission::parse("order:update")?,
            target: order_scope,
        })
        .await?;

    Ok(decision == AccessDecision::Allow)
}
```

业务对象的 `ScopePath` 应来自可信数据，例如订单所属门店、区域或组织树，不建议直接相信客户端传入的路径。

## Step 5b: 平台租户数据查询前过滤

平台列表、导出或跨租户查询应使用 `TenantDataScopeQuery`：

```rust
use rs_tenant::{
    platform::{PlatformEngine, PlatformSubject, TenantDataAccessScope, TenantDataScopeQuery},
    Permission,
};

pub async fn platform_list_orders(
    engine: &PlatformEngine<DbPlatformAuthorizationSource>,
    repo: &OrderRepo,
    subject: PlatformSubject,
) -> rs_tenant::Result<Vec<Order>> {
    let scope = engine
        .accessible_tenants(TenantDataScopeQuery {
            subject,
            permission: Permission::parse("tenant/order:read")?,
        })
        .await?;

    match scope {
        TenantDataAccessScope::None => Ok(vec![]),
        TenantDataAccessScope::AllTenants => repo.list_all_tenants().await,
        TenantDataAccessScope::Tenants { tenants } => repo.list_by_tenants(tenants).await,
        TenantDataAccessScope::TenantPaths { entries } => {
            repo.list_by_tenant_scope_roots(entries).await
        }
    }
}
```

`TenantDataAccessScope::AllTenants` 不是全局绕过。它只表示该平台主体对当前 permission 的租户数据范围是所有租户；业务层仍应做分页、审计、操作类型区分和必要的目标存在性检查。

## Step 6: 缓存失效

权限数据变更后按影响范围失效：

- membership 或 role assignment 变化：`invalidate_principal(tenant, principal)`
- role permission 变化：`invalidate_role(tenant, role)`
- tenant 级批量变更：`invalidate_tenant(tenant)`
- 无法精确识别影响面：`invalidate_all()`

正确性要求高于性能。如果 `invalidate_role` 无法精确找到受影响主体，必须退化为 `invalidate_tenant` 或 `invalidate_all`。

平台缓存如果未来启用，cache key 应以 `platform principal + PlatformEngineConfig` 为核心，不应复用租户内 `tenant + principal + config` 的 cache key。

## 继续阅读

- [上一章：04. 5 分钟快速接入](04-quickstart.md)
- [下一章：06. Axum 与 JWT 集成](06-axum-integration.md)
- [11. 平台授权](11-platform-authorization.md)
- [返回目录](SUMMARY.md)

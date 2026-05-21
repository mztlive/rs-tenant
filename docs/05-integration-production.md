# 05. 生产环境集成指南

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](04-quickstart.md) | [下一章](06-axum-integration.md)

生产接入的核心是实现 `AuthorizationSource`，然后把 `AccessScope` 下推到业务查询。

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

如果应用层需要平台管理能力，请单独设计平台权限系统；进入 `rs-tenant` 前必须先明确目标租户和租户内主体。

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

## Step 6: 缓存失效

权限数据变更后按影响范围失效：

- membership 或 role assignment 变化：`invalidate_principal(tenant, principal)`
- role permission 变化：`invalidate_role(tenant, role)`
- tenant 级批量变更：`invalidate_tenant(tenant)`
- 无法精确识别影响面：`invalidate_all()`

正确性要求高于性能。如果 `invalidate_role` 无法精确找到受影响主体，必须退化为 `invalidate_tenant` 或 `invalidate_all`。

## 继续阅读

- [上一章：04. 5 分钟快速接入](04-quickstart.md)
- [下一章：06. Axum 与 JWT 集成](06-axum-integration.md)
- [返回目录](SUMMARY.md)

# 05. 接入生产数据源

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](04-quickstart.md) | [下一章](06-axum-integration.md)

生产接入的核心工作只有两件：

1. 实现 `AuthorizationSource`，让引擎能读到授权数据。
2. 把 `AccessScope` 转成你的业务查询条件。

## Step 1: 准备最小数据模型

你不需要按 crate 的类型逐字建表，但至少要能回答这些问题：

| 问题 | 示例表 |
|---|---|
| 租户是否可用 | `tenants(id, status)` |
| 主体是否是租户成员 | `tenant_memberships(tenant_id, principal_id, status)` |
| 主体有哪些角色分配 | `tenant_role_assignments(tenant_id, principal_id, role_id, scope_kind, scope_paths)` |
| 角色有哪些权限 | `tenant_role_permissions(tenant_id, role_id, permission)` |
| 角色继承关系是什么 | `tenant_role_inherits(tenant_id, role_id, parent_role_id)` |

索引建议：

- 租户内表以 `tenant_id` 作为前导索引。
- membership 使用 `(tenant_id, principal_id)` 唯一索引。
- role assignment 至少有 `(tenant_id, principal_id)` 和 `(tenant_id, role_id)` 索引。
- role permission 使用 `(tenant_id, role_id)` 索引。

不需要为租户内 core 准备 `super_admin`、`global_role` 或 Casbin policy 表。

## Step 2: 实现 `AuthorizationSource`

```rust
use async_trait::async_trait;
use rs_tenant::{
    AuthSubject, AuthorizationSource, GrantScope, MembershipStatus, Permission, RoleAssignment,
    RoleId, SourceError, TenantId, TenantStatus,
};

#[derive(Clone)]
pub struct DbAuthorizationSource {
    // pool: PgPool,
}

#[async_trait]
impl AuthorizationSource for DbAuthorizationSource {
    async fn tenant_status(&self, tenant: &TenantId) -> Result<TenantStatus, SourceError> {
        let _ = tenant;
        // SELECT status FROM tenants WHERE id = $1
        Ok(TenantStatus::Active)
    }

    async fn membership_status(
        &self,
        subject: &AuthSubject,
    ) -> Result<MembershipStatus, SourceError> {
        let _ = subject;
        // SELECT status FROM tenant_memberships WHERE tenant_id = $1 AND principal_id = $2
        Ok(MembershipStatus::Active)
    }

    async fn role_assignments(
        &self,
        subject: &AuthSubject,
    ) -> Result<Vec<RoleAssignment>, SourceError> {
        let _ = subject;
        // SELECT role_id, scope_kind, scope_paths FROM tenant_role_assignments ...
        let role = RoleId::parse("reader").map_err(|err| Box::new(err) as SourceError)?;
        Ok(vec![RoleAssignment::new(role, GrantScope::tenant())])
    }

    async fn role_permissions(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> Result<Vec<Permission>, SourceError> {
        let _ = (tenant, role);
        // SELECT permission FROM tenant_role_permissions WHERE tenant_id = $1 AND role_id = $2
        let permission =
            Permission::parse("order:read").map_err(|err| Box::new(err) as SourceError)?;
        Ok(vec![permission])
    }

    async fn parent_roles(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> Result<Vec<RoleId>, SourceError> {
        let _ = (tenant, role);
        // SELECT parent_role_id FROM tenant_role_inherits ...
        Ok(Vec::new())
    }
}
```

实现时保持数据源“薄”：

- 只读数据。
- 缺失数据返回 inactive、空列表或错误，按你的业务语义决定。
- 不在这里做 permission match。
- 不在这里合并 scope。
- 不在这里展开角色继承。

## Step 3: 构建 Engine

```rust
use rs_tenant::{Engine, EngineBuilder, MemoryCache};

type AppEngine = Engine<DbAuthorizationSource, MemoryCache>;

fn build_engine(source: DbAuthorizationSource) -> AppEngine {
    EngineBuilder::new(source)
        .enable_role_hierarchy(true)
        .enable_wildcard(true)
        .max_role_depth(16)
        .cache(MemoryCache::new(100_000))
        .build()
}
```

如果暂时不想启用缓存：

```rust
let engine = EngineBuilder::new(source).build();
```

建议先把无缓存链路测准，再加缓存和失效。

## Step 4: 把 `AccessScope` 下推到查询

```rust
use rs_tenant::{AccessScope, AuthSubject, Permission, ScopeQuery};

pub async fn list_orders(
    engine: &AppEngine,
    repo: &OrderRepo,
    subject: AuthSubject,
) -> rs_tenant::Result<Vec<Order>> {
    let scope = engine
        .accessible_scope(ScopeQuery {
            subject,
            permission: Permission::parse("order:read")?,
        })
        .await?;

    match scope {
        AccessScope::None => Ok(Vec::new()),
        AccessScope::Tenant { tenant } => repo.list_by_tenant(tenant).await,
        AccessScope::Paths { tenant, roots } => repo.list_by_scope_roots(tenant, roots).await,
    }
}
```

仓储层通常需要提供两类查询：

- 按 `tenant_id` 查询。
- 按 `tenant_id + scope roots` 查询。

`scope_path` 可以是物化路径、组织树闭包表、ltree、搜索索引字段，或你自己的层级查询方案。

## Step 5: 单个对象先查真实归属

```rust
use rs_tenant::{AccessDecision, AuthSubject, Permission, ScopedAccessRequest};

pub async fn update_order(
    engine: &AppEngine,
    repo: &OrderRepo,
    subject: AuthSubject,
    order_id: OrderId,
) -> rs_tenant::Result<bool> {
    let order = repo.load(order_id).await?;
    let decision = engine
        .can_access_scope(ScopedAccessRequest {
            subject,
            permission: Permission::parse("order:update")?,
            target: order.scope_path(),
        })
        .await?;

    Ok(decision == AccessDecision::Allow)
}
```

不要用 URL 或请求体里的路径代替 `order.scope_path()`，否则客户端可以伪造目标归属。

## Step 6: 缓存和失效

启用 `memory-cache` 后，可以使用内置缓存：

```toml
rs-tenant = { version = "0.4.0", features = ["memory-cache"] }
```

失效入口：

```rust
engine.invalidate_principal(&tenant, &principal).await;
engine.invalidate_role(&tenant, &role).await;
engine.invalidate_tenant(&tenant).await;
engine.invalidate_all().await;
```

建议：

- 成员状态变更：失效 principal。
- 某人的角色分配变更：失效 principal。
- 角色权限变更：失效 role。
- 租户禁用：失效 tenant。
- 无法精确判断影响范围：失效 tenant 或 all。

缓存不能牺牲正确性。失效返回后，受影响主体不应继续命中过期授权。

## Step 7: 数据源错误怎么处理

`AuthorizationSource` 返回 `SourceError` 时，`Engine` 会返回 `Err`。Web 层通常映射为 500，并记录错误。

不要把数据库错误吞掉后返回 `Active` 或伪造权限。授权链路出错时应 fail closed。

## 平台数据源

如果启用 `platform`，额外实现 `PlatformAuthorizationSource`。平台表建议单独建模：

- `platform_principals(id, status)`
- `platform_role_assignments(principal_id, role_id, scope_kind, tenants, tenant_scope_roots)`
- `platform_role_permissions(role_id, permission)`
- `platform_role_inherits(role_id, parent_role_id)`

平台 Source 和租户 Source 不要混用。详见 [11. 平台授权：平台员工和跨租户数据](11-platform-authorization.md)。

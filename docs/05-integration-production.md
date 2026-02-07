# 05. 生产环境集成指南

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](04-quickstart.md) | [下一章](06-axum-integration.md)

本章给出一套可落地的生产接入步骤。

## Step 1: 设计权限数据模型

推荐最小逻辑表：

- `tenants(id, active)`
- `tenant_principals(tenant_id, principal_id, active)`
- `tenant_principal_roles(tenant_id, principal_id, role_id)`
- `tenant_role_permissions(tenant_id, role_id, permission)`
- `tenant_role_inherits(tenant_id, role_id, parent_role_id)`
- `global_principal_roles(principal_id, global_role_id)`
- `global_role_permissions(global_role_id, permission)`
- `platform_super_admins(principal_id)`

索引建议：
- 租户相关表统一以 `(tenant_id, ...)` 作为前导索引
- `platform_super_admins(principal_id)` 唯一索引

## Step 2: 实现 Store trait

你需要实现三组接口：`TenantStore`、`RoleStore`、`GlobalRoleStore`。

```rust
use async_trait::async_trait;
use rs_tenant::{
    GlobalRoleId, GlobalRoleStore, Permission, PrincipalId, RoleId, RoleStore, StoreError,
    TenantId, TenantStore,
};

pub struct DbStore {
    // 例如：数据库连接池
}

#[async_trait]
impl TenantStore for DbStore {
    async fn tenant_active(&self, tenant: TenantId) -> Result<bool, StoreError> {
        let _ = tenant;
        todo!()
    }

    async fn principal_active(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
    ) -> Result<bool, StoreError> {
        let _ = (tenant, principal);
        todo!()
    }
}

#[async_trait]
impl RoleStore for DbStore {
    async fn principal_roles(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
    ) -> Result<Vec<RoleId>, StoreError> {
        let _ = (tenant, principal);
        todo!()
    }

    async fn role_permissions(
        &self,
        tenant: TenantId,
        role: RoleId,
    ) -> Result<Vec<Permission>, StoreError> {
        let _ = (tenant, role);
        todo!()
    }

    async fn role_inherits(
        &self,
        tenant: TenantId,
        role: RoleId,
    ) -> Result<Vec<RoleId>, StoreError> {
        let _ = (tenant, role);
        todo!()
    }
}

#[async_trait]
impl GlobalRoleStore for DbStore {
    async fn global_roles(&self, principal: PrincipalId) -> Result<Vec<GlobalRoleId>, StoreError> {
        let _ = principal;
        todo!()
    }

    async fn global_role_permissions(
        &self,
        role: GlobalRoleId,
    ) -> Result<Vec<Permission>, StoreError> {
        let _ = role;
        todo!()
    }

    async fn is_super_admin(&self, principal: PrincipalId) -> Result<bool, StoreError> {
        let _ = principal;
        Ok(false)
    }
}
```

## Step 3: 构建 Engine

```rust
use rs_tenant::{EngineBuilder, MemoryCache};
use std::time::Duration;

fn build_engine(store: DbStore) -> rs_tenant::Engine<DbStore, MemoryCache> {
    EngineBuilder::new(store)
        .enable_role_hierarchy(true)
        .enable_wildcard(true)
        .enable_super_admin(true)
        .max_inherit_depth(16)
        .permission_normalize(true)
        .cache(MemoryCache::new(10_000).with_ttl(Duration::from_secs(30)))
        .build()
}
```

## Step 4: 在业务服务调用授权

```rust
use rs_tenant::{Decision, Permission, PrincipalId, TenantId};

pub async fn can_read_invoice(
    engine: &rs_tenant::Engine<DbStore, rs_tenant::MemoryCache>,
    tenant: TenantId,
    principal: PrincipalId,
) -> rs_tenant::Result<bool> {
    let p = Permission::try_from("invoice:read")?;
    let d = engine.authorize(tenant, principal, p).await?;
    Ok(matches!(d, Decision::Allow))
}
```

## Step 5: 处理缓存失效

权限关系更新后，按变更范围失效缓存：

- 主体维度：`invalidate_principal(tenant, principal)`
- 角色维度：`invalidate_role(tenant, role)`
- 租户维度：`invalidate_tenant(tenant)`

如果你暂时没有可靠的统一失效入口，建议先禁用缓存，确认授权正确性后再启用。

## 继续阅读

- [上一页：04. 5 分钟快速接入](04-quickstart.md)
- [下一页：06. Axum 与 JWT 集成](06-axum-integration.md)
- [返回目录](SUMMARY.md)

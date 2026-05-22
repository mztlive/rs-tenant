# 04. 5 分钟快速接入

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](03-authorization-flow.md) | [下一章](05-integration-production.md)

本章用 `MemorySource` 跑通租户内 RBAC 主链路，并在最后给出 `memory-store + platform` 的平台授权入口。

## Step 1: 添加依赖

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["memory-store", "memory-cache"] }
```

如果你的项目只接生产数据源，可以不启用内存实现；本章为了演示使用 `MemorySource`。

## Step 2: 准备主体、角色和权限

```rust
use rs_tenant::{
    AuthSubject, GrantScope, MembershipStatus, MemorySource, Permission, PrincipalId, RoleId,
    ScopePath, TenantId, TenantStatus,
};

async fn seed() -> rs_tenant::Result<(MemorySource, AuthSubject)> {
    let source = MemorySource::new();

    let tenant = TenantId::parse("tenant_a")?;
    let principal = PrincipalId::parse("user_1")?;
    let subject = AuthSubject {
        tenant: tenant.clone(),
        principal: principal.clone(),
    };
    let role = RoleId::parse("store_order_reader")?;

    source.set_tenant_status(tenant.clone(), TenantStatus::Active);
    source.set_membership_status(
        tenant.clone(),
        principal.clone(),
        MembershipStatus::Active,
    );
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        role.clone(),
        GrantScope::paths(vec![
            ScopePath::parse("agent/123/store/456")?,
            ScopePath::parse("agent/123/store/789")?,
        ])?,
    );
    source.add_role_permission(tenant, role, Permission::parse("order:read")?);

    Ok((source, subject))
}
```

重点是 `RoleAssignment.scope`：v0.3 不会从 membership 自动推导范围。

## Step 3: 查询可访问范围

```rust
use rs_tenant::{AccessScope, EngineBuilder, Permission, ScopeQuery};

async fn query_scope() -> rs_tenant::Result<()> {
    let (source, subject) = seed().await?;
    let engine = EngineBuilder::new(source).build();

    let scope = engine
        .accessible_scope(ScopeQuery {
            subject,
            permission: Permission::parse("order:read")?,
        })
        .await?;

    assert!(matches!(scope, AccessScope::Paths { .. }));
    Ok(())
}
```

列表接口应把 `AccessScope` 转成业务查询条件：

- `AccessScope::None`：直接返回空列表。
- `AccessScope::Tenant { tenant }`：查询整个租户。
- `AccessScope::Paths { tenant, roots }`：按 roots 下推过滤。

## Step 4: 判断目标路径

```rust
use rs_tenant::{AccessDecision, EngineBuilder, Permission, ScopePath, ScopedAccessRequest};

async fn can_read_target() -> rs_tenant::Result<()> {
    let (source, subject) = seed().await?;
    let engine = EngineBuilder::new(source).build();

    let decision = engine
        .can_access_scope(ScopedAccessRequest {
            subject,
            permission: Permission::parse("order:read")?,
            target: ScopePath::parse("agent/123/store/456/order/9001")?,
        })
        .await?;

    assert_eq!(decision, AccessDecision::Allow);
    Ok(())
}
```

## Step 5: 租户级操作

```rust
use rs_tenant::{AccessDecision, EngineBuilder, Permission, TenantAccessRequest};

async fn can_manage_tenant_settings() -> rs_tenant::Result<()> {
    let (source, subject) = seed().await?;
    let engine = EngineBuilder::new(source).build();

    let decision = engine
        .can_tenant(TenantAccessRequest {
            subject,
            permission: Permission::parse("tenant/settings:update")?,
        })
        .await?;

    assert_eq!(decision, AccessDecision::Deny);
    Ok(())
}
```

上面的主体只有路径级 `order:read` 授权，所以租户级设置操作会拒绝。

## Step 6: 常用 Engine 配置

```rust
use rs_tenant::{EngineBuilder, MemoryCache};

let engine = EngineBuilder::new(source)
    .enable_role_hierarchy(true)
    .enable_wildcard(true)
    .max_role_depth(16)
    .cache(MemoryCache::new(10_000))
    .build();
```

v0.3 不提供 `enable_super_admin`。租户内管理员应建普通角色，授予 `*:*`，并使用 `GrantScope::Tenant` 分配。

## Step 7: 可选平台授权

平台授权是 v0.4.0 的可选能力，需要启用 `platform` feature：

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["memory-store", "platform"] }
```

示例：

```rust
use rs_tenant::{
    platform::{
        MemoryPlatformSource, PlatformEngineBuilder, PlatformGrantScope, PlatformPrincipalId,
        PlatformPrincipalStatus, PlatformRoleId, PlatformSubject, TenantDataAccessScope,
        TenantDataScopeQuery,
    },
    Permission, TenantId,
};

async fn query_platform_tenant_scope() -> rs_tenant::Result<()> {
    let source = MemoryPlatformSource::new();
    let subject = PlatformSubject {
        principal: PlatformPrincipalId::parse("ops_1")?,
    };
    let role = PlatformRoleId::parse("tenant_support")?;

    source.set_principal_status(
        subject.principal.clone(),
        PlatformPrincipalStatus::Active,
    );
    source.add_role_assignment(
        subject.principal.clone(),
        role.clone(),
        PlatformGrantScope::tenants(vec![TenantId::parse("tenant_a")?])?,
    );
    source.add_role_permission(role, Permission::parse("tenant:read")?);

    let platform_engine = PlatformEngineBuilder::new(source).build();
    let scope = platform_engine
        .accessible_tenants(TenantDataScopeQuery {
            subject,
            permission: Permission::parse("tenant:read")?,
        })
        .await?;

    assert!(matches!(scope, TenantDataAccessScope::Tenants { .. }));
    Ok(())
}
```

这里的 `PlatformGrantScope::Tenants` 只表示平台账号可管理的租户数据范围，不会绕过租户内 `Engine`，也不会把平台主体变成租户成员。

## 继续阅读

- [上一章：03. 授权流程详解](03-authorization-flow.md)
- [下一章：05. 生产环境集成指南](05-integration-production.md)
- [11. 平台授权](11-platform-authorization.md)
- [返回目录](SUMMARY.md)

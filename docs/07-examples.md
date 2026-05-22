# 07. 典型案例

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](06-axum-integration.md) | [下一章](08-testing-benchmark.md)

本章用租户内 API 和 v0.4.0 `platform` feature 描述常见授权场景。案例 A-D 是租户内 RBAC 示例，保持 v0.3 语义不变。

## 案例 A：租户级管理员

租户内管理员不是特殊主体，而是普通角色：

```text
role: tenant_admin
permission: *:*
assignment scope: GrantScope::Tenant
```

示例：

```rust
use rs_tenant::{
    AccessDecision, AuthSubject, EngineBuilder, GrantScope, MembershipStatus, MemorySource,
    Permission, PrincipalId, RoleId, TenantAccessRequest, TenantId, TenantStatus,
};

async fn tenant_admin_can_update_settings() -> rs_tenant::Result<()> {
    let source = MemorySource::new();
    let tenant = TenantId::parse("tenant_a")?;
    let principal = PrincipalId::parse("admin_1")?;
    let subject = AuthSubject { tenant: tenant.clone(), principal };
    let role = RoleId::parse("tenant_admin")?;

    source.set_tenant_status(tenant.clone(), TenantStatus::Active);
    source.set_membership_status(
        tenant.clone(),
        subject.principal.clone(),
        MembershipStatus::Active,
    );
    source.add_role_assignment(
        tenant.clone(),
        subject.principal.clone(),
        role.clone(),
        GrantScope::Tenant,
    );
    source.add_role_permission(tenant, role, Permission::parse("*:*")?);

    let engine = EngineBuilder::new(source).enable_wildcard(true).build();
    let decision = engine
        .can_tenant(TenantAccessRequest {
            subject,
            permission: Permission::parse("tenant/settings:update")?,
        })
        .await?;

    assert_eq!(decision, AccessDecision::Allow);
    Ok(())
}
```

这仍然受租户状态和 membership 状态约束，不是 super admin 绕过。

## 案例 B：门店订单只读

```rust
use rs_tenant::{
    AccessDecision, AuthSubject, EngineBuilder, GrantScope, MembershipStatus, MemorySource,
    Permission, PrincipalId, RoleId, ScopePath, ScopedAccessRequest, TenantId, TenantStatus,
};

async fn store_reader_can_read_inside_store() -> rs_tenant::Result<()> {
    let source = MemorySource::new();
    let tenant = TenantId::parse("tenant_a")?;
    let principal = PrincipalId::parse("user_1")?;
    let subject = AuthSubject { tenant: tenant.clone(), principal };
    let role = RoleId::parse("store_order_reader")?;

    source.set_tenant_status(tenant.clone(), TenantStatus::Active);
    source.set_membership_status(
        tenant.clone(),
        subject.principal.clone(),
        MembershipStatus::Active,
    );
    source.add_role_assignment(
        tenant.clone(),
        subject.principal.clone(),
        role.clone(),
        GrantScope::paths(vec![ScopePath::parse("agent/123/store/456")?])?,
    );
    source.add_role_permission(tenant, role, Permission::parse("order:read")?);

    let engine = EngineBuilder::new(source).build();
    let decision = engine
        .can_access_scope(ScopedAccessRequest {
            subject,
            permission: Permission::parse("order:read")?,
            target: ScopePath::parse("agent/123/store/456/order/1")?,
        })
        .await?;

    assert_eq!(decision, AccessDecision::Allow);
    Ok(())
}
```

目标路径在授权 root 下方，因此允许。

## 案例 C：同一主体拥有多个范围

```rust
use rs_tenant::{AccessScope, Permission, ScopeQuery};

let scope = engine
    .accessible_scope(ScopeQuery {
        subject,
        permission: Permission::parse("order:read")?,
    })
    .await?;

match scope {
    AccessScope::Paths { roots, .. } => {
        // 根路径已经去重，并删除被祖先覆盖的子路径
        assert!(!roots.is_empty());
    }
    _ => {}
}
```

如果多个 role assignment 都命中 `order:read`：

- 任一 assignment 是 `GrantScope::Tenant`，最终就是 `AccessScope::Tenant`。
- 全部是 `GrantScope::Paths`，最终合并为 `AccessScope::Paths`。
- 没有命中，返回 `AccessScope::None`。

## 案例 D：路径级授权不能执行租户级操作

```rust
use rs_tenant::{AccessDecision, Permission, TenantAccessRequest};

let decision = engine
    .can_tenant(TenantAccessRequest {
        subject,
        permission: Permission::parse("order:read")?,
    })
    .await?;

assert_eq!(decision, AccessDecision::Deny);
```

即使主体拥有 `order:read` 的某些路径范围，只要最终不是 `AccessScope::Tenant`，`can_tenant` 就拒绝。

## 案例 E：平台账号查询可管理租户

启用 `memory-store + platform` 后，可以用 `PlatformEngine` 计算平台账号可管理的租户数据范围：

```rust
use rs_tenant::{
    platform::{
        MemoryPlatformSource, PlatformEngineBuilder, PlatformGrantScope, PlatformPrincipalId,
        PlatformPrincipalStatus, PlatformRoleId, PlatformSubject, TenantDataAccessScope,
        TenantDataScopeQuery,
    },
    Permission, TenantId,
};

async fn platform_support_can_list_one_tenant() -> rs_tenant::Result<()> {
    let source = MemoryPlatformSource::new();
    let subject = PlatformSubject {
        principal: PlatformPrincipalId::parse("support_1")?,
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

    let engine = PlatformEngineBuilder::new(source).build();
    let scope = engine
        .accessible_tenants(TenantDataScopeQuery {
            subject,
            permission: Permission::parse("tenant:read")?,
        })
        .await?;

    assert!(matches!(scope, TenantDataAccessScope::Tenants { .. }));
    Ok(())
}
```

业务仓储应把 `TenantDataAccessScope::Tenants` 下推为 `tenant_id IN (...)`。这不是 super admin 绕过，也不会把 `support_1` 变成租户内成员。

## 案例 F：未启用 platform 时的平台客服代查

如果没有启用 v0.4.0 `platform` feature，租户内 core 仍不提供平台主体。应用层需要显式完成映射：

1. 平台权限系统确认客服可以代查目标租户。
2. 应用创建或选择一个租户内 `PrincipalId`。
3. 在该租户下写入有时效的 membership 和 role assignment。
4. 调用 `rs-tenant` 时只传 `AuthSubject { tenant, principal }`。
5. 审批、原因、过期时间和审计日志由应用层维护。

这样 core 仍保持租户内 RBAC，不引入跨租户绕过语义。

## 继续阅读

- [上一章：06. Axum 与 JWT 集成](06-axum-integration.md)
- [下一章：08. 测试与性能基准](08-testing-benchmark.md)
- [返回目录](SUMMARY.md)

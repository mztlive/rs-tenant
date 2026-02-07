# 07. 典型案例

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](06-axum-integration.md) | [下一章](08-testing-benchmark.md)

本章给出四个最常见的授权场景，便于你对照业务快速映射。

## 案例 A：平台员工通过 GlobalRole 获得权限

```rust
use rs_tenant::{
    Decision, EngineBuilder, GlobalRoleId, MemoryStore, Permission, PrincipalId, TenantId,
};

async fn case_a() -> rs_tenant::Result<()> {
    let store = MemoryStore::new();
    let tenant = TenantId::try_from("tenant_a")?;
    let principal = PrincipalId::try_from("staff_platform_1")?;
    let global_role = GlobalRoleId::try_from("platform_billing_reader")?;

    store.set_tenant_active(tenant.clone(), true);
    store.set_principal_active(tenant.clone(), principal.clone(), true);
    store.add_global_role(principal.clone(), global_role.clone());
    store.add_global_role_permission(global_role, Permission::try_from("billing:read")?);

    let engine = EngineBuilder::new(store).build();
    let d = engine
        .authorize(tenant, principal, Permission::try_from("billing:read")?)
        .await?;
    assert_eq!(d, Decision::Allow);
    Ok(())
}
```

## 案例 B：租户员工通过 Tenant Role 获得权限

```rust
use rs_tenant::{Decision, EngineBuilder, MemoryStore, Permission, PrincipalId, RoleId, TenantId};

async fn case_b() -> rs_tenant::Result<()> {
    let store = MemoryStore::new();
    let tenant = TenantId::try_from("tenant_a")?;
    let principal = PrincipalId::try_from("staff_tenant_1")?;
    let role = RoleId::try_from("tenant_invoice_reader")?;

    store.set_tenant_active(tenant.clone(), true);
    store.set_principal_active(tenant.clone(), principal.clone(), true);
    store.add_principal_role(tenant.clone(), principal.clone(), role.clone());
    store.add_role_permission(tenant.clone(), role, Permission::try_from("invoice:read")?);

    let engine = EngineBuilder::new(store).build();
    let d = engine
        .authorize(tenant, principal, Permission::try_from("invoice:read")?)
        .await?;
    assert_eq!(d, Decision::Allow);
    Ok(())
}
```

## 案例 C：平台角色 + 租户角色权限并集

同一主体同时拥有租户角色与全局角色时，最终权限是两者并集，只要任意命中就放行。

```rust
use rs_tenant::{
    Decision, EngineBuilder, GlobalRoleId, MemoryStore, Permission, PrincipalId, RoleId, TenantId,
};

async fn case_c() -> rs_tenant::Result<()> {
    let store = MemoryStore::new();
    let tenant = TenantId::try_from("tenant_a")?;
    let principal = PrincipalId::try_from("staff_mix_1")?;

    let tenant_role = RoleId::try_from("tenant_invoice_reader")?;
    let global_role = GlobalRoleId::try_from("platform_report_exporter")?;

    store.set_tenant_active(tenant.clone(), true);
    store.set_principal_active(tenant.clone(), principal.clone(), true);

    store.add_principal_role(tenant.clone(), principal.clone(), tenant_role.clone());
    store.add_role_permission(
        tenant.clone(),
        tenant_role,
        Permission::try_from("invoice:read")?,
    );

    store.add_global_role(principal.clone(), global_role.clone());
    store.add_global_role_permission(global_role, Permission::try_from("report:export")?);

    let engine = EngineBuilder::new(store).build();
    let d1 = engine
        .authorize(
            tenant.clone(),
            principal.clone(),
            Permission::try_from("invoice:read")?,
        )
        .await?;
    let d2 = engine
        .authorize(tenant, principal, Permission::try_from("report:export")?)
        .await?;

    assert_eq!(d1, Decision::Allow);
    assert_eq!(d2, Decision::Allow);
    Ok(())
}
```

## 案例 D：超级管理员开关对比

```rust
use rs_tenant::{Decision, EngineBuilder, MemoryStore, Permission, PrincipalId, TenantId};

async fn case_d() -> rs_tenant::Result<()> {
    let store = MemoryStore::new();
    let tenant = TenantId::try_from("tenant_a")?;
    let principal = PrincipalId::try_from("platform_admin")?;

    store.set_tenant_active(tenant.clone(), true);
    store.add_super_admin(principal.clone());

    let on = EngineBuilder::new(store.clone()).enable_super_admin(true).build();
    let off = EngineBuilder::new(store).enable_super_admin(false).build();

    let p = Permission::try_from("any_resource:any_action")?;
    let d1 = on.authorize(tenant.clone(), principal.clone(), p.clone()).await?;
    let d2 = off.authorize(tenant, principal, p).await?;

    assert_eq!(d1, Decision::Allow);
    assert_eq!(d2, Decision::Deny);
    Ok(())
}
```

## 案例选择建议

- 你是租户业务系统：先套用案例 B
- 你有平台统一职能账号：加上案例 A
- 你有紧急运维兜底账号：按案例 D 设计超级管理员

## 继续阅读

- [上一页：06. Axum 与 JWT 集成](06-axum-integration.md)
- [下一页：08. 测试与性能基准](08-testing-benchmark.md)
- [返回目录](SUMMARY.md)

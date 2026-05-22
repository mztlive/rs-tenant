# 04. 5 分钟跑通第一个授权

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](03-authorization-flow.md) | [下一章](05-integration-production.md)

本章用内存数据源跑通最小链路：创建租户、成员、角色、权限和范围，然后判断一个目标路径是否允许访问。

## Step 1: 添加依赖

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["memory-store"] }
futures = "0.3"
```

`memory-store` 只用于示例和测试。生产环境通常实现自己的 `AuthorizationSource`。

## Step 2: 准备授权数据

```rust
use rs_tenant::{
    AuthSubject, GrantScope, MembershipStatus, MemorySource, Permission, PrincipalId, RoleId,
    ScopePath, TenantId, TenantStatus,
};

fn seed() -> rs_tenant::Result<(MemorySource, AuthSubject, Permission)> {
    let tenant = TenantId::parse("tenant_demo")?;
    let principal = PrincipalId::parse("user_demo")?;
    let role = RoleId::parse("store_reader")?;
    let permission = Permission::parse("invoice:read")?;

    let source = MemorySource::new();
    source.set_tenant_status(tenant.clone(), TenantStatus::Active);
    source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        role.clone(),
        GrantScope::paths(vec![ScopePath::parse("agent/1")?])?,
    );
    source.add_role_permission(tenant.clone(), role, permission.clone());

    Ok((source, AuthSubject::new(tenant, principal), permission))
}
```

这段数据表达的是：`user_demo` 在 `tenant_demo` 内通过 `store_reader` 角色获得 `invoice:read`，但范围只覆盖 `agent/1` 及其子路径。

## Step 3: 判断一个对象路径

```rust
use rs_tenant::{AccessDecision, EngineBuilder, ScopePath, ScopedAccessRequest};

async fn check_one_object() -> rs_tenant::Result<()> {
    let (source, subject, permission) = seed()?;
    let engine = EngineBuilder::new(source).build();

    let decision = engine
        .can_access_scope(ScopedAccessRequest {
            subject,
            permission,
            target: ScopePath::parse("agent/1/store/9/invoice/10001")?,
        })
        .await?;

    assert_eq!(decision, AccessDecision::Allow);
    Ok(())
}
```

因为目标路径在 `agent/1` 下，所以允许。

## Step 4: 查询列表范围

列表接口通常不应该先查出所有数据再逐条过滤，而是先拿范围。

```rust
use rs_tenant::{AccessScope, EngineBuilder, ScopeQuery};

async fn list_scope() -> rs_tenant::Result<()> {
    let (source, subject, permission) = seed()?;
    let engine = EngineBuilder::new(source).build();

    let scope = engine
        .accessible_scope(ScopeQuery {
            subject,
            permission,
        })
        .await?;

    match scope {
        AccessScope::None => {}
        AccessScope::Tenant { tenant } => {
            println!("query all rows in tenant {}", tenant);
        }
        AccessScope::Paths { tenant, roots } => {
            println!("query tenant {} under {:?}", tenant, roots);
        }
    }

    Ok(())
}
```

实际项目里，`AccessScope::Paths` 应该被转换成 SQL、ORM 或搜索引擎条件。

## Step 5: 跑官方示例

```bash
cargo run --example quickstart --features memory-store
```

如果你想看生产数据源的 trait 实现骨架：

```bash
cargo run --example production_source
```

## 常见改法

全租户管理员：

```rust
source.add_role_assignment(
    tenant.clone(),
    principal.clone(),
    role.clone(),
    GrantScope::tenant(),
);
source.add_role_permission(tenant, role, Permission::parse("*:*")?);

let engine = EngineBuilder::new(source).enable_wildcard(true).build();
```

多个路径：

```rust
GrantScope::paths(vec![
    ScopePath::parse("agent/1/store/9")?,
    ScopePath::parse("agent/1/store/10")?,
])?
```

角色继承：

```rust
let engine = EngineBuilder::new(source)
    .enable_role_hierarchy(true)
    .max_role_depth(16)
    .build();
```

## 下一步

内存示例跑通后，继续看 [05. 接入生产数据源](05-integration-production.md)，把 `AuthorizationSource` 接到你的数据库。

# 04. 5 分钟快速接入

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](03-authorization-flow.md) | [下一章](05-integration-production.md)

本章目标：用内存存储在本地快速跑通 `authorize`。

## Step 1: 启用依赖 feature

在你的 `Cargo.toml` 中启用 `memory-store`：

```toml
[dependencies]
rs-tenant = { version = "0.1", features = ["memory-store"] }
```

## Step 2: 写最小授权示例

```rust
use rs_tenant::{
    Decision, EngineBuilder, MemoryStore, Permission, PrincipalId, RoleId, TenantId,
};

async fn quick_start() -> rs_tenant::Result<()> {
    let store = MemoryStore::new();

    let tenant = TenantId::try_from("tenant_a")?;
    let principal = PrincipalId::try_from("user_1")?;
    let role = RoleId::try_from("invoice_reader")?;
    let permission = Permission::try_from("invoice:read")?;

    store.set_tenant_active(tenant.clone(), true);
    store.set_principal_active(tenant.clone(), principal.clone(), true);
    store.add_principal_role(tenant.clone(), principal.clone(), role.clone());
    store.add_role_permission(tenant.clone(), role, permission.clone());

    let engine = EngineBuilder::new(store).build();
    let decision = engine.authorize(tenant, principal, permission).await?;

    assert_eq!(decision, Decision::Allow);
    Ok(())
}
```

## Step 3: 运行验证

在仓库中可以直接运行：

```bash
cargo test --offline --features memory-store
```

如果你在自己的服务中接入，确认 `quick_start()` 返回 `Allow` 即表示主链路可用。

## Step 4: 打开常用增强配置（可选）

```rust
let engine = EngineBuilder::new(store)
    .enable_role_hierarchy(true)
    .enable_wildcard(true)
    .enable_super_admin(true)
    .max_inherit_depth(16)
    .permission_normalize(true)
    .build();
```

建议先最小配置跑通，再按业务需求逐项开启，避免调试面过大。

## 继续阅读

- [上一页：03. 授权流程详解](03-authorization-flow.md)
- [下一页：05. 生产环境集成指南](05-integration-production.md)
- [返回目录](SUMMARY.md)

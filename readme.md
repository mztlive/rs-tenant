# rs-tenant

多租户 RBAC 权限库（Rust）。

它专注做三件事：
- 在租户上下文内做可靠授权（`authorize`）
- 在资源查询前给出可执行的作用域（`scope`）
- 通过可插拔 `Store` 对接你自己的数据库/缓存

默认策略是 `Deny by default`。

## 1. 适用场景

适合：
- SaaS 多租户系统
- 需要“租户角色 + 平台全局角色”混合授权
- 希望把权限引擎和业务存储解耦

不适合：
- 纯 ABAC（复杂属性策略）为主的系统
- 希望库内直接托管数据库模型/迁移的场景

## 2. 当前能力

- 强类型 ID：`TenantId`、`PrincipalId`、`RoleId`、`GlobalRoleId`
- 权限模型：`resource:action`（如 `invoice:read`）
- 可选角色继承（层级展开、环检测、深度限制）
- 可选通配权限（`*:*`、`invoice:*` 等）
- 全局角色（跨租户）
- 平台级超级管理员（可选开关）
- 内存 `Store`（测试/演示）
- 内存 `Cache`（TTL + LRU + 分片锁）
- Axum 中间件集成（可选 JWT 解析）

## 3. 核心概念

- `Tenant`：租户
- `Principal`：主体（用户/服务账号）
- `Role`：租户内角色
- `GlobalRole`：平台级全局角色
- `Permission`：权限字符串，格式 `resource:action`
- `Decision`：`Allow` 或 `Deny`
- `Scope`：`None` 或 `TenantOnly { tenant }`

### 平台超级管理员语义

这是你当前库中的明确设计：
- 超级管理员是平台级能力，只按 `principal` 判断，不按租户存储。
- 需要显式启用：`EngineBuilder::enable_super_admin(true)`。
- 即使是超级管理员，也仍受 `tenant_active` 约束。
- 一旦命中超级管理员，跳过 `principal_active(tenant, principal)` 和角色权限计算。

## 4. 授权流程（实际执行顺序）

`authorize(tenant, principal, permission)`：

1. 检查 `tenant_active(tenant)`，否则 `Deny`
2. 如果开启 super-admin 且 `is_super_admin(principal)`，直接 `Allow`
3. 检查 `principal_active(tenant, principal)`，否则 `Deny`
4. 收集权限：租户角色权限 + 全局角色权限
5. 进行权限匹配
6. 命中则 `Allow`，否则 `Deny`

`scope(tenant, principal, resource)` 类似，返回 `TenantOnly` 或 `None`。

## 5. 依赖与 Feature

```toml
[dependencies]
# 如果你从 crates.io 使用
rs-tenant = { version = "0.1", features = [] }

# 如果你在 monorepo 或本地路径使用
# rs-tenant = { path = "../rs-tenant", features = [] }
```

常用 feature：
- `memory-store`：内存存储实现
- `memory-cache`：内存缓存实现
- `serde`：类型序列化支持
- `axum`：Axum 授权中间件
- `axum-jwt`：JWT 解析 + Axum（依赖 `axum` 与 `serde`）
- `criterion-bench`：Criterion 基准测试

说明：
- `casbin` feature 当前仅保留开关，暂未提供公开 Casbin 适配 API。

## 6. 快速开始（5 分钟）

下面示例使用内存存储，快速跑通授权。

### Step 1: 打开内存 feature

```toml
[dependencies]
rs-tenant = { version = "0.1", features = ["memory-store"] }
```

### Step 2: 构造测试数据并授权

```rust
use rs_tenant::{
    Decision, EngineBuilder, MemoryStore, Permission, PrincipalId, RoleId, TenantId,
};

async fn demo() -> rs_tenant::Result<()> {
    let store = MemoryStore::new();

    let tenant = TenantId::try_from("tenant_a").unwrap();
    let principal = PrincipalId::try_from("user_1").unwrap();
    let role = RoleId::try_from("invoice_reader").unwrap();
    let permission = Permission::try_from("invoice:read").unwrap();

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

### Step 3: 启用平台超级管理员（可选）

```rust
use rs_tenant::{Decision, EngineBuilder, MemoryStore, Permission, PrincipalId, TenantId};

async fn demo() -> rs_tenant::Result<()> {
    let store = MemoryStore::new();

    let tenant = TenantId::try_from("tenant_a").unwrap();
    let principal = PrincipalId::try_from("platform_admin").unwrap();

    store.set_tenant_active(tenant.clone(), true);
    store.add_super_admin(principal.clone());

    let engine = EngineBuilder::new(store)
        .enable_super_admin(true)
        .build();

    let decision = engine.authorize(
        tenant,
        principal,
        Permission::try_from("any_resource:any_action").unwrap(),
    )
    .await?;

    assert_eq!(decision, Decision::Allow);
    Ok(())
}
```

## 7. 生产接入指南（循序渐进）

### Step 1: 先设计数据模型

推荐至少有这些逻辑表：
- `tenants(id, active)`
- `tenant_principals(tenant_id, principal_id, active)`
- `tenant_principal_roles(tenant_id, principal_id, role_id)`
- `tenant_role_permissions(tenant_id, role_id, permission)`
- `tenant_role_inherits(tenant_id, role_id, parent_role_id)`
- `global_principal_roles(principal_id, global_role_id)`
- `global_role_permissions(global_role_id, permission)`
- `platform_super_admins(principal_id)`

索引建议：
- 所有以 `tenant_id` 查询的表，建立前导 `tenant_id` 组合索引
- 角色关系表至少有 `(tenant_id, role_id)` 索引
- `platform_super_admins(principal_id)` 建唯一索引

### Step 2: 实现 `Store` trait

你只需实现三组 trait：`TenantStore`、`RoleStore`、`GlobalRoleStore`。

```rust
use async_trait::async_trait;
use rs_tenant::{
    GlobalRoleId, GlobalRoleStore, Permission, PrincipalId, RoleId, RoleStore,
    StoreError, TenantId, TenantStore,
};

pub struct DbStore {
    // 你的数据库连接池
}

#[async_trait]
impl TenantStore for DbStore {
    async fn tenant_active(&self, tenant: TenantId) -> Result<bool, StoreError> {
        // SELECT active FROM tenants WHERE id = ?
        let _ = tenant;
        todo!()
    }

    async fn principal_active(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
    ) -> Result<bool, StoreError> {
        // SELECT active FROM tenant_principals WHERE tenant_id = ? AND principal_id = ?
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
        // SELECT 1 FROM platform_super_admins WHERE principal_id = ?
        let _ = principal;
        todo!()
    }
}
```

### Step 3: 构建 `Engine`

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

配置建议：
- `enable_role_hierarchy`：有角色继承就开
- `enable_wildcard`：你允许 `*` 权限时再开
- `enable_super_admin`：你确实实现平台超管时再开
- `max_inherit_depth`：建议 8~32
- `permission_normalize`：通常保持 `true`

### Step 4: 在应用服务里调用

```rust
use rs_tenant::{Decision, Permission, PrincipalId, TenantId};

pub async fn check_invoice_read(
    engine: &rs_tenant::Engine<DbStore, rs_tenant::MemoryCache>,
    tenant: TenantId,
    principal: PrincipalId,
) -> rs_tenant::Result<bool> {
    let permission = Permission::try_from("invoice:read")?;
    let decision = engine.authorize(tenant, principal, permission).await?;

    match decision {
        Decision::Allow => Ok(true),
        Decision::Deny => Ok(false),
    }
}
```

### Step 5: 做好缓存失效

当权限数据变更后，调用缓存失效接口：
- `invalidate_principal(tenant, principal)`
- `invalidate_role(tenant, role)`
- `invalidate_tenant(tenant)`

如果你没有统一变更入口，建议先不用缓存，确认行为正确后再打开。

### Step 6: Axum 接入（可选）

1. 打开 feature：`axum`（或 `axum-jwt`）
2. 请求进入后先放入 `AuthContext { tenant, principal }`
3. 使用 `AuthorizeLayer` 为路由声明权限

使用 JWT 时，推荐顺序：
- `.layer(AuthorizeLayer::new(...))`
- `.layer(JwtAuthLayer::new(...))`

这样 `JwtAuthLayer` 会先执行，先把 `AuthContext` 注入请求扩展，再执行授权层。

## 8. 权限字符串规则

默认 `Permission` 规则：
- 格式必须是 `resource:action`
- 默认会 `trim + 小写化`
- 不允许空段
- 默认字符集：`a-z`、`0-9`、`_`、`-`、`:`
- 通配符 `*` 仅在开启 `enable_wildcard(true)` 后参与匹配

## 9. 测试与性能

### 运行单元测试

```bash
cargo test --offline
cargo test --offline --features memory-store,memory-cache
```

### 运行手工性能测试

```bash
cargo test --offline --features memory-store,memory-cache --test perf -- --ignored --nocapture
```

### 运行 Criterion 基准

```bash
cargo bench --features criterion-bench,memory-store,memory-cache
```

## 10. 设计边界与注意事项

- 当前 `scope` 只有租户级结果：`TenantOnly` / `None`。
- 超级管理员是平台级，但依然要求 `tenant_active = true`。
- 库本身不做 ORM 绑定，不生成迁移，不托管业务模型。
- 生产环境请优先实现你自己的 `Store`，`MemoryStore` 仅用于测试/演示。

## 11. 版本与兼容性建议

在接入方项目中建议：
- 固定小版本（如 `0.1.x`）
- 把权限规则和 `EngineBuilder` 参数做成集中配置
- 升级版本前先跑回归：授权案例、越权案例、缓存失效案例

---

如果你准备把它接入真实业务，我可以继续帮你补一版“数据库表结构 + SQL 查询模板 + Store 实现骨架（按你现有 ORM）”。

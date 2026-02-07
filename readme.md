# rs-tenant

Rust 多租户 RBAC 授权库。

`rs-tenant` 提供一个可插拔的授权引擎，目标是将“权限决策”与“业务存储实现”解耦：

- 在租户上下文内进行授权判定：`authorize`
- 在查询前计算可访问范围：`scope`
- 通过 `Store` trait 接入任意数据库/缓存

默认策略为 `Deny by default`。

## 适用场景

适合：

- SaaS 多租户系统
- 同时存在租户角色与平台全局角色的系统
- 希望将权限判断从业务逻辑中抽离的服务

不适合：

- 以复杂 ABAC 为主的授权模型
- 需要库内直接托管 ORM 模型与迁移

## 核心能力

- 强类型 ID：`TenantId`、`PrincipalId`、`RoleId`、`GlobalRoleId`
- 权限模型：`resource:action`（如 `invoice:read`）
- 租户角色 + 平台角色并集授权
- 可选角色继承（含环检测、深度限制）
- 可选 wildcard（如 `*:*`、`invoice:*`）
- 可选超级管理员短路授权
- 可选内存缓存（TTL、LRU、分片）
- Axum 中间件集成（可选 JWT 解析）

## 快速开始

### 1) 添加依赖

```toml
[dependencies]
rs-tenant = { version = "0.1", features = ["memory-store"] }
```

### 2) 最小授权示例

```rust
use rs_tenant::{
    Decision, EngineBuilder, MemoryStore, Permission, PrincipalId, RoleId, TenantId,
};

async fn demo() -> rs_tenant::Result<()> {
    let store = MemoryStore::new();

    let tenant = TenantId::try_from("tenant_a")?;
    let principal = PrincipalId::try_from_parts("employee", "user_1")?;
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

### 3) 本地验证

```bash
cargo test --offline --features memory-store,memory-cache
```

## 授权语义（摘要）

`authorize(tenant, principal, permission)` 执行顺序：

1. `tenant_active(tenant)`
2. （可选）`is_super_admin(principal)`
3. `principal_active(tenant, principal)`
4. 收集权限（租户角色 + 全局角色）
5. 按配置匹配权限，返回 `Allow` 或 `Deny`

`scope(tenant, principal, resource)` 返回：

- `Scope::TenantOnly { tenant }`
- `Scope::None`

超级管理员为平台级能力，但仍受 `tenant_active` 约束。

## Feature 开关

```toml
[features]
default = []
serde = []
memory-store = []
memory-cache = []
axum = []
axum-jwt = []
criterion-bench = []
casbin = []
```

说明：

- `axum-jwt` 依赖 `axum` 和 `serde`
- `casbin` 当前仅保留 feature 开关，未提供公开适配 API

## 生产集成建议

生产环境通常按以下步骤接入：

1. 设计权限数据模型（租户、主体、角色、权限、继承、全局角色、超级管理员）
2. 实现三组 Store 接口：`TenantStore`、`RoleStore`、`GlobalRoleStore`
3. 使用 `EngineBuilder` 组装引擎（继承/wildcard/super-admin/缓存）
4. 在权限数据变更后执行缓存失效

缓存失效接口：

- `invalidate_principal(tenant, principal)`
- `invalidate_role(tenant, role)`
- `invalidate_tenant(tenant)`

## Axum 集成

- 启用 `axum` 或 `axum-jwt` feature
- 在请求扩展中注入 `AuthContext { tenant, principal }`
- 使用 `AuthorizeLayer` 为路由绑定权限
- 使用 JWT 时可配合 `JwtAuthLayer`

详细示例见 `docs/06-axum-integration.md`。

## 文档目录

完整中文文档见 `docs/`：

- `docs/README.md`：文档首页
- `docs/01-overview.md`：项目总览
- `docs/02-domain-model.md`：领域模型与权限语义
- `docs/03-authorization-flow.md`：授权流程详解
- `docs/04-quickstart.md`：5 分钟接入
- `docs/05-integration-production.md`：生产集成指南
- `docs/06-axum-integration.md`：Axum/JWT 集成
- `docs/07-examples.md`：典型案例
- `docs/08-testing-benchmark.md`：测试与基准
- `docs/09-faq-troubleshooting.md`：FAQ 与排查

## 文档发布（mdBook + GitHub Pages）

仓库已内置 `book.toml` 与 `.github/workflows/mdbook.yml`，文档会在 `main` 分支的文档变更后自动发布到 GitHub Pages。

本地预览：

```bash
cargo install mdbook --locked
mdbook serve
```

首次启用 GitHub Pages：

1. 打开仓库 `Settings -> Pages`
2. `Build and deployment` 的 `Source` 选择 `GitHub Actions`
3. 合并并推送本次配置后，等待 `Deploy mdBook to GitHub Pages` 工作流完成

## 开发与测试命令

```bash
# 默认测试
cargo test --offline

# 带内存存储/缓存测试
cargo test --offline --features memory-store,memory-cache

# 手工性能测试
cargo test --offline --features memory-store,memory-cache --test perf -- --ignored --nocapture

# Criterion 基准
cargo bench --features criterion-bench,memory-store,memory-cache
```

## 设计边界

- 本库只负责授权决策与作用域计算，不负责业务数据模型
- 不内置数据库迁移管理
- `MemoryStore` 主要用于测试与演示

## 贡献说明

欢迎提交 Issue 和 PR。提交前建议：

1. 补充或更新相关测试
2. 运行上述测试命令
3. 若涉及性能路径，附上基准对比结果

仓库贡献约定见 `AGENTS.md`。

# 08. 测试、缓存和性能验证

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](07-examples.md) | [下一章](09-faq-troubleshooting.md)

授权代码的测试重点不是覆盖率数字，而是防止误放行。每个新权限路径都应该同时覆盖 allow 和 deny。

## 推荐命令

基础测试：

```bash
cargo test --offline
```

带内存数据源、缓存和 serde：

```bash
cargo test --offline --features memory-store,memory-cache,serde
```

平台授权：

```bash
cargo test --offline --features memory-store,platform
```

完整 lint：

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -D warnings
```

## 租户内授权测试清单

每个业务权限至少覆盖：

- 租户 active + 成员 active + 权限命中 + 范围覆盖 -> allow。
- 租户 inactive -> deny 或 `AccessScope::None`。
- 成员 inactive -> deny 或 `AccessScope::None`。
- 没有角色分配 -> deny。
- 角色没有该 permission -> deny。
- 目标路径在授权根外 -> deny。
- 路径级授权调用 `can_tenant` -> deny。
- 全租户授权调用 `can_tenant` -> allow。

示例结构：

```rust
#[tokio::test]
async fn store_reader_can_read_order_inside_store() {
    // seed tenant/member/role/permission/scope
    // call can_access_scope
    // assert Allow
}

#[tokio::test]
async fn store_reader_cannot_read_sibling_store_order() {
    // same seed
    // target is outside granted root
    // assert Deny
}
```

## 范围和权限值对象测试

如果你修改 crate 本身，重点覆盖：

- `TenantId`、`PrincipalId`、`RoleId` 非空、长度和字符校验。
- `Permission::parse("resource:action")` 的切分规则。
- wildcard 匹配是否受 `enable_wildcard` 控制。
- `ScopePath::allows` 的相等和祖先路径。
- `GrantScope::paths` 拒绝空 roots。
- roots 去重和祖先覆盖压缩。

## 角色继承测试

开启 `enable_role_hierarchy(true)` 时，至少覆盖：

- 子角色继承父角色权限。
- 继承权限仍使用当前 assignment 的 `GrantScope`。
- 角色环返回错误。
- 超过 `max_role_depth` 返回错误。

不要在 `AuthorizationSource::parent_roles` 里做展开；它只返回直接父角色。

## 缓存测试

如果启用 `memory-cache` 或实现自定义缓存，要验证失效后不会命中过期授权。

覆盖：

- 成员状态变化后 principal 失效。
- 角色分配变化后 principal 失效。
- 角色权限变化后 role 失效。
- 租户禁用后 tenant 失效。
- TTL 过期。
- 不同 engine 配置不会共用错误缓存。

常用失效入口：

```rust
engine.invalidate_principal(&tenant, &principal).await;
engine.invalidate_role(&tenant, &role).await;
engine.invalidate_tenant(&tenant).await;
engine.invalidate_all().await;
```

## 性能验证

手动性能测试：

```bash
cargo test --offline --features memory-store,memory-cache --test perf -- --ignored --nocapture
```

Criterion：

```bash
cargo bench --features criterion-bench,memory-store,memory-cache
```

记录性能时，至少写下：

- commit 或 tag。
- `rustc --version`。
- CPU、内存、系统版本。
- 是否开启缓存。
- 角色数量、权限数量、继承深度。

历史记录见 [12. 性能基线记录](12-perf-baseline.md)。

## 平台授权测试清单

启用 `platform` 后，额外覆盖：

- 平台主体 inactive -> deny。
- 没有平台角色分配 -> deny。
- `PlatformGrantScope::Platform` 可以访问平台自身资源。
- `Platform` scope 不能访问租户数据。
- `AllTenants` 可以访问任意租户数据。
- `Tenants([a])` 允许 tenant a，拒绝 tenant b。
- `TenantPaths` 只允许指定租户下的指定路径。
- `TenantPaths` 不能被 `can_access_tenant` 当成租户级权限。
- 同一 permission 混用 `Tenants` 和 `TenantPaths` 会返回错误。

平台测试不应该改变租户内 `Engine` 的语义。

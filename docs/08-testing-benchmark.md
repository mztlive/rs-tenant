# 08. 测试与性能基准

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](07-examples.md) | [下一章](09-faq-troubleshooting.md)

测试重点是领域规则、Engine 流程、缓存正确性和可解释性。启用 v0.4.0 `platform` feature 时，还需要覆盖平台主体、平台范围和 `PlatformEngine` 流程。

## 功能测试命令

```bash
cargo test --offline
cargo test --offline --features memory-store,memory-cache,serde
cargo test --offline --features memory-store,platform
```

文档或示例变更通常不需要全量测试；涉及源码、feature 或 public API 时建议至少运行带内存实现和 serde 的测试。

## 领域测试建议

覆盖：

- `TenantId` / `PrincipalId` / `RoleId` 构造校验。
- serde 反序列化必须走构造校验。
- `Permission::parse` 的 `resource:action` 切分规则。
- wildcard 匹配：`*:*`、`invoice:*`、`*:read`。
- `ScopePath::allows` 的相等和祖先路径规则。
- `GrantScope::Paths` 拒绝空 roots。
- `AccessScope` 合并、去重和祖先覆盖。

## Engine 流程测试建议

覆盖：

- tenant inactive -> `AccessScope::None` / `AccessDecision::Deny`
- membership inactive -> `AccessScope::None` / `AccessDecision::Deny`
- no role assignment -> none / deny
- tenant role permission allow
- permission missing deny
- role inheritance allow
- role cycle error
- role depth exceeded
- assignment tenant scope -> tenant accessible scope
- assignment path scopes -> path accessible scope
- multiple path assignments -> merged paths
- scoped target inside root -> allow
- scoped target outside roots -> deny
- `can_tenant` with path-only grant -> deny
- `can_tenant` with tenant grant -> allow

## Cache 测试建议

覆盖：

- tenant principal 热缓存。
- 配置签名隔离。
- assignment scope 变化失效。
- role permission 变化失效。
- role inheritance 变化失效。
- TTL 过期。
- 多 shard LRU 行为。
- role 精确失效不可用时退化为 tenant/all 失效。

缓存不能使用 best-effort 正确性。失效 API 返回后，受影响授权不得再命中过期 grant。

## 性能基准命令

```bash
cargo test --offline --features memory-store,memory-cache --test perf -- --ignored --nocapture
cargo bench --features criterion-bench,memory-store,memory-cache
```

建议记录：

- `accessible_scope` 无缓存 P50/P95。
- `accessible_scope` 热缓存 P50/P95。
- `can_access_scope` path allow / deny。
- 角色继承深度对延迟的影响。
- 角色扇出对延迟的影响。
- 缓存失效后的恢复耗时。

## 非目标测试

租户内 core 不应新增以下测试：

- platform super admin。
- tenant bypass membership super admin。
- platform write audit mode。
- Casbin 最终决策链路。

平台授权测试应放在 `platform` feature 下，并验证它不改变租户内 `Engine` 语义。

## PlatformEngine 测试建议

覆盖：

- 平台主体 inactive 时拒绝。
- 无平台角色分配时拒绝。
- 平台角色拥有 `Platform` scope 时可以访问平台资源。
- `Platform` scope 不能访问租户数据。
- `AllTenants` 可以访问任意租户数据。
- `Tenants([a])` 可以访问 tenant a，拒绝 tenant b。
- `TenantPaths` 可以访问目标路径子孙，拒绝兄弟路径。
- `TenantPaths` 不能被 `can_access_tenant` 当成租户级权限。
- 多个平台角色分配可以合并租户范围。
- 同一租户下的路径 roots 会被压缩。
- 同一 permission 下混用 `Tenants` 与 `TenantPaths` 会返回错误，且范围查询与点判定保持一致。
- 平台角色继承支持父角色权限。
- 平台角色继承能检测 cycle。
- 平台角色继承能限制最大深度。
- wildcard 仍然受 `enable_wildcard` 控制。
- 租户 `Engine` 的现有测试不需要为了 `platform` feature 改语义。

## 继续阅读

- [上一章：07. 典型案例](07-examples.md)
- [下一章：09. FAQ 与故障排查](09-faq-troubleshooting.md)
- [11. 平台授权](11-platform-authorization.md)
- [返回目录](SUMMARY.md)

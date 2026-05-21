# 08. 测试与性能基准

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](07-examples.md) | [下一章](09-faq-troubleshooting.md)

v0.3.0 的测试重点是领域规则、Engine 流程、缓存正确性和可解释性。

## 功能测试命令

```bash
cargo test --offline
cargo test --offline --features memory-store,memory-cache,serde
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

v0.3 core 不应新增以下测试：

- platform tenant set。
- platform super admin。
- tenant bypass membership super admin。
- platform write audit mode。
- Casbin 最终决策链路。

这些能力不在 core 范围内，测试它们会把已删除边界重新带回核心。

## 继续阅读

- [上一章：07. 典型案例](07-examples.md)
- [下一章：09. FAQ 与故障排查](09-faq-troubleshooting.md)
- [返回目录](SUMMARY.md)

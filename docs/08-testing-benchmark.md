# 08. 测试与性能基准

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](07-examples.md) | [下一章](09-faq-troubleshooting.md)

本章说明如何验证功能正确性与性能回归。

## 1) 功能测试

默认测试：

```bash
cargo test --offline
```

开启内存存储/缓存相关测试：

```bash
cargo test --offline --features memory-store,memory-cache
```

建议在提交前至少跑一轮带 feature 的测试，确保角色继承、通配符、缓存路径都覆盖。

## 2) 手工性能测试

仓库内提供了忽略态性能测试（`tests/perf.rs`）：

```bash
cargo test --offline --features memory-store,memory-cache --test perf -- --ignored --nocapture
```

输出包含中位耗时、`ns/op` 和 `ops/s`，适合快速对比优化前后。

## 3) Criterion 基准

```bash
cargo bench --features criterion-bench,memory-store,memory-cache
```

该基准覆盖：
- 平面角色授权（有/无缓存）
- 角色继承深度影响
- 角色扇出规模影响
- `scope` 允许/拒绝路径

## 4) 回归验证建议

每次权限相关改动后，建议至少验证以下场景：

1. 基础 allow/deny
2. wildcard 开关生效差异
3. super-admin 开关生效差异
4. 角色继承深度和环检测
5. 缓存开启后授权结果一致性

## 5) 性能观察指标

- 单次授权延迟（P50/P95）
- 高并发下缓存命中率
- 角色继承深度对吞吐影响
- 缓存失效后的恢复耗时

如果业务对时延敏感，优先保证缓存失效策略正确，再做参数优化（TTL、分片数、容量）。

## 继续阅读

- [上一页：07. 典型案例](07-examples.md)
- [下一页：09. FAQ 与故障排查](09-faq-troubleshooting.md)
- [返回目录](SUMMARY.md)

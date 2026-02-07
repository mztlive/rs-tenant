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

### 当前基准数据（2026-02-07）

测试环境：

- 系统：macOS 26.2（arm64）
- 芯片：Apple M4 Pro（12 核：8P + 4E）
- 内存：24 GB
- Rust：`rustc 1.91.0` / `cargo 1.91.0`
- 命令：`cargo test --offline --features memory-store,memory-cache --test perf -- --ignored --nocapture`

结果（中位数）：

| 场景 | 中位耗时 | ns/op | ops/s | 参数 |
|---|---:|---:|---:|---|
| `authorize_flat_no_cache` | 730.550 ms | 3652.8 | 273,766 | iters=200000 |
| `authorize_flat_hot_cache` | 537.541 ms | 2687.7 | 372,065 | iters=200000 |
| `scope_flat_hot_cache` | 503.921 ms | 2519.6 | 396,888 | iters=200000 |
| `authorize_hierarchy_depth8_no_cache` | 1292.247 ms | 25844.9 | 38,692 | iters=50000 |
| `authorize_flat_hot_cache_parallel_single_shard` | 1184.560 ms | 2961.4 | 337,678 | threads=8, total_ops=400000 |
| `authorize_flat_hot_cache_parallel_sharded` | 1173.053 ms | 2932.6 | 340,991 | threads=8, total_ops=400000 |

简要解读：

1. 热缓存下 `authorize` 吞吐较无缓存提升明显（约 35.9%）。
2. `scope` 热缓存路径略快于 `authorize` 热缓存路径。
3. 深度继承（depth=8）会显著拉高单次授权开销。
4. 并发下分片缓存相比单分片有小幅提升（本次约 1%）。

说明：以上数据用于基线对比，不同机器、编译参数、系统负载下数值会变化。

### Release 基准数据（2026-02-07）

命令：

```bash
cargo test --release --offline --features memory-store,memory-cache --test perf -- --ignored --nocapture
```

结果（中位数）：

| 场景 | 中位耗时 | ns/op | ops/s | 参数 |
|---|---:|---:|---:|---|
| `authorize_flat_no_cache` | 99.149 ms | 495.7 | 2,017,172 | iters=200000 |
| `authorize_flat_hot_cache` | 77.860 ms | 389.3 | 2,568,708 | iters=200000 |
| `scope_flat_hot_cache` | 75.475 ms | 377.4 | 2,649,893 | iters=200000 |
| `authorize_hierarchy_depth8_no_cache` | 149.460 ms | 2989.2 | 334,537 | iters=50000 |
| `authorize_flat_hot_cache_parallel_single_shard` | 498.317 ms | 1245.8 | 802,702 | threads=8, total_ops=400000 |
| `authorize_flat_hot_cache_parallel_sharded` | 484.717 ms | 1211.8 | 825,224 | threads=8, total_ops=400000 |

与前面的 Debug 基线相比（按 `ns/op`）：

- `authorize_flat_no_cache`：约 `7.37x` 加速
- `authorize_flat_hot_cache`：约 `6.90x` 加速
- `scope_flat_hot_cache`：约 `6.68x` 加速
- `authorize_hierarchy_depth8_no_cache`：约 `8.65x` 加速
- 并发热缓存场景：约 `2.38x ~ 2.42x` 加速

额外观察：

- Release 下分片缓存对并发吞吐提升约 `2.8%`（`825,224` vs `802,702` ops/s）。

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

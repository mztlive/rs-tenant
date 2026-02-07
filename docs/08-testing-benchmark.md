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
| `authorize_flat_no_cache` | 753.822 ms | 3769.1 | 265,315 | iters=200000 |
| `authorize_flat_hot_cache` | 480.928 ms | 2404.6 | 415,862 | iters=200000 |
| `scope_flat_hot_cache` | 430.631 ms | 2153.2 | 464,435 | iters=200000 |
| `authorize_hierarchy_depth8_no_cache` | 1304.109 ms | 26082.2 | 38,340 | iters=50000 |
| `authorize_flat_hot_cache_parallel_single_shard` | 158.165 ms | 395.4 | 2,529,003 | threads=8, total_ops=400000 |
| `authorize_flat_hot_cache_parallel_sharded` | 156.065 ms | 390.2 | 2,563,038 | threads=8, total_ops=400000 |

简要解读：

1. 热缓存下 `authorize` 吞吐较无缓存提升明显（约 56.7%）。
2. `scope` 热缓存路径略快于 `authorize` 热缓存路径。
3. 深度继承（depth=8）会显著拉高单次授权开销。
4. 并发热缓存场景吞吐达 `2.54M ops/s` 量级，分片与单分片接近（该压测为单 key 热点）。

说明：以上数据用于基线对比，不同机器、编译参数、系统负载下数值会变化。

### Release 基准数据（2026-02-07）

命令：

```bash
cargo test --release --offline --features memory-store,memory-cache --test perf -- --ignored --nocapture
```

结果（中位数）：

| 场景 | 中位耗时 | ns/op | ops/s | 参数 |
|---|---:|---:|---:|---|
| `authorize_flat_no_cache` | 101.428 ms | 507.1 | 1,971,836 | iters=200000 |
| `authorize_flat_hot_cache` | 68.966 ms | 344.8 | 2,899,967 | iters=200000 |
| `scope_flat_hot_cache` | 65.979 ms | 329.9 | 3,031,271 | iters=200000 |
| `authorize_hierarchy_depth8_no_cache` | 151.994 ms | 3039.9 | 328,961 | iters=50000 |
| `authorize_flat_hot_cache_parallel_single_shard` | 169.730 ms | 424.3 | 2,356,688 | threads=8, total_ops=400000 |
| `authorize_flat_hot_cache_parallel_sharded` | 141.443 ms | 353.6 | 2,827,998 | threads=8, total_ops=400000 |

与上方 Debug 数据相比（按 `ns/op`）：

- `authorize_flat_no_cache`：约 `7.34x` 加速
- `authorize_flat_hot_cache`：约 `6.97x` 加速
- `scope_flat_hot_cache`：约 `6.53x` 加速
- `authorize_hierarchy_depth8_no_cache`：约 `8.58x` 加速
- 并发热缓存场景：约 `0.93x ~ 1.10x`（该项波动较大，建议看多轮中位数）

### 优化前后关键对比（以 2026-02-07 早先基线为参照）

本轮不改对外 API 的内部优化后，核心收益集中在并发热缓存路径：

- Debug `parallel_single_shard`：`337,678 -> 2,529,003 ops/s`（约 `7.49x`）
- Debug `parallel_sharded`：`340,991 -> 2,563,038 ops/s`（约 `7.52x`）
- Release `parallel_single_shard`：`802,702 -> 2,356,688 ops/s`（约 `2.94x`）
- Release `parallel_sharded`：`825,224 -> 2,827,998 ops/s`（约 `3.43x`）

同时单线程热缓存也有改进：

- Debug `authorize_flat_hot_cache`：`2687.7 -> 2404.6 ns/op`（约 `10.5%` 提升）
- Release `authorize_flat_hot_cache`：`389.3 -> 344.8 ns/op`（约 `11.4%` 提升）

代价与取舍：

- 无缓存路径和深继承路径存在小幅波动（约 `1%~3%`），本轮优化主要换取并发热缓存吞吐。

## 3) Criterion 基准

```bash
cargo bench --features criterion-bench,memory-store,memory-cache
```

该基准覆盖：
- 平面角色授权（有/无缓存）
- 角色继承深度影响
- 角色扇出规模影响
- `scope` 允许/拒绝路径

### Criterion 当前数据（2026-02-07）

命令：

```bash
cargo bench --features criterion-bench,memory-store,memory-cache --bench criterion_engine -- --noplot
```

平面授权与 scope：

| 场景 | time 区间 | thrpt 区间 |
|---|---|---|
| `authorize_flat/authorize_no_cache` | `468.21 ns ~ 470.80 ns` | `2.1240 ~ 2.1358 Melem/s` |
| `authorize_flat/scope_no_cache` | `456.95 ns ~ 459.24 ns` | `2.1775 ~ 2.1884 Melem/s` |
| `authorize_flat/authorize_cache_single_shard` | `299.62 ns ~ 301.69 ns` | `3.3146 ~ 3.3375 Melem/s` |
| `authorize_flat/scope_cache_single_shard` | `290.08 ns ~ 291.20 ns` | `3.4341 ~ 3.4473 Melem/s` |
| `authorize_flat/authorize_cache_sharded` | `304.73 ns ~ 306.85 ns` | `3.2590 ~ 3.2816 Melem/s` |
| `authorize_flat/scope_cache_sharded` | `292.18 ns ~ 293.92 ns` | `3.4023 ~ 3.4225 Melem/s` |

继承深度：

| 深度 | time 区间 | thrpt 区间 |
|---:|---|---|
| `1` | `969.12 ns ~ 975.67 ns` | `1.0249 ~ 1.0319 Melem/s` |
| `4` | `1.8722 µs ~ 1.9010 µs` | `526.04 ~ 534.13 Kelem/s` |
| `8` | `3.0877 µs ~ 3.1748 µs` | `314.98 ~ 323.86 Kelem/s` |
| `16` | `5.5525 µs ~ 5.7532 µs` | `173.81 ~ 180.10 Kelem/s` |

角色扇出：

| 角色数 | time 区间 | thrpt 区间 |
|---:|---|---|
| `1` | `481.16 ns ~ 482.61 ns` | `2.0721 ~ 2.0783 Melem/s` |
| `8` | `1.8782 µs ~ 1.8912 µs` | `528.75 ~ 532.43 Kelem/s` |
| `32` | `6.7737 µs ~ 6.8142 µs` | `146.75 ~ 147.63 Kelem/s` |
| `128` | `25.740 µs ~ 25.855 µs` | `38.677 ~ 38.850 Kelem/s` |

scope 行为：

| 场景 | time 区间 | thrpt 区间 |
|---|---|---|
| `scope_behavior/scope_allow` | `293.87 ns ~ 295.34 ns` | `3.3859 ~ 3.4028 Melem/s` |
| `scope_behavior/scope_deny` | `269.47 ns ~ 272.46 ns` | `3.6703 ~ 3.7110 Melem/s` |

简要结论：

1. 热缓存路径稳定在 `~300ns` 量级，吞吐约 `3.3 Melem/s`。
2. 继承深度与角色扇出增长时，延迟近似线性上升。
3. `scope_deny` 略快于 `scope_allow`，符合匹配路径短路预期。

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

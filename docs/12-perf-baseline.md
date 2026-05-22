# 12. 性能基线记录

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](11-platform-authorization.md)

本页用于记录发布前的性能基线。它不是硬性 SLA，而是给后续重构和优化提供可比较的参考。

## 固定命令

功能正确性先于性能数据。记录性能前先跑完整矩阵：

```bash
scripts/test-matrix.sh
```

手动性能测试：

```bash
cargo test --offline --features memory-store,memory-cache --test perf -- --ignored --nocapture
```

Criterion 基准：

```bash
cargo bench --features criterion-bench,memory-store,memory-cache
```

## 当前记录项

每次准备发布或做性能相关改动时，建议记录：

- 测试机器：CPU、内存、系统版本。
- Rust 工具链：`rustc --version`。
- 代码版本：commit hash 或 tag。
- `can_access_scope_flat_no_cache`。
- `can_tenant_hot_cache`。
- `can_access_scope_hot_cache`。
- `can_access_scope_hierarchy_depth8_no_cache`。
- `can_access_scope_hot_cache_parallel`。
- Criterion `v03_flat_access`。
- Criterion `v03_hierarchy_depth`。
- Criterion `v03_role_fanout`。

## 记录模板

```text
日期：
commit：
rustc：
机器：

manual perf:
- can_access_scope_flat_no_cache:
- can_tenant_hot_cache:
- can_access_scope_hot_cache:
- can_access_scope_hierarchy_depth8_no_cache:
- can_access_scope_hot_cache_parallel:

criterion:
- v03_flat_access/can_access_scope_no_cache:
- v03_flat_access/can_tenant_cache:
- v03_hierarchy_depth/1:
- v03_hierarchy_depth/4:
- v03_hierarchy_depth/8:
- v03_hierarchy_depth/16:
- v03_role_fanout/1:
- v03_role_fanout/8:
- v03_role_fanout/32:
- v03_role_fanout/128:
```

## 2026-05-22 基线

日期：2026-05-22 13:05:39 CST  
commit：`bad5585` + 当前工作区改动  
rustc：`rustc 1.93.1 (01f6ddf75 2026-02-11)`  
机器：Apple M4 Pro，24 GiB 内存，macOS Darwin 25.4.0 arm64  
备注：Criterion 本次未找到 Gnuplot，使用 plotters backend。

manual perf：

- `can_access_scope_flat_no_cache`：median `774.540 ms`，`3,872.7 ns/op`，`258,218 ops/s`
- `can_tenant_hot_cache`：median `562.585 ms`，`2,812.9 ns/op`，`355,502 ops/s`
- `can_access_scope_hot_cache`：median `575.226 ms`，`2,876.1 ns/op`，`347,689 ops/s`
- `can_access_scope_hierarchy_depth8_no_cache`：median `1,411.592 ms`，`28,231.8 ns/op`，`35,421 ops/s`
- `can_access_scope_hot_cache_parallel`：median `250.129 ms`，`625.3 ns/op`，`1,599,177 ops/s`，`8 threads`

Criterion：

- `v03_flat_access/can_access_scope_no_cache`：`913.33 ns` - `921.67 ns`，估计值 `917.35 ns`
- `v03_flat_access/can_tenant_cache`：`589.29 ns` - `605.29 ns`，估计值 `595.75 ns`
- `v03_hierarchy_depth/1`：`1.5259 us` - `1.5455 us`，估计值 `1.5353 us`
- `v03_hierarchy_depth/4`：`2.5808 us` - `2.6168 us`，估计值 `2.5998 us`
- `v03_hierarchy_depth/8`：`3.8874 us` - `3.9489 us`，估计值 `3.9138 us`
- `v03_hierarchy_depth/16`：`6.7330 us` - `7.2959 us`，估计值 `6.9216 us`
- `v03_role_fanout/1`：`666.45 ns` - `680.08 ns`，估计值 `673.06 ns`
- `v03_role_fanout/8`：`2.4697 us` - `2.4937 us`，估计值 `2.4806 us`
- `v03_role_fanout/32`：`8.6112 us` - `8.7439 us`，估计值 `8.6656 us`
- `v03_role_fanout/128`：`32.258 us` - `32.591 us`，估计值 `32.424 us`

## 判断规则

- 授权结果错误、缓存失效后仍命中过期授权、source error 被吞掉，都是发布阻断问题。
- 性能回退需要结合场景判断；如果热缓存或扁平授权路径明显变慢，应优先检查 clone、锁粒度和 feature 引入的额外分支。
- Criterion 报告出现持续退化时，应在同一机器同一命令下复跑确认，避免把一次性系统抖动当成回归。

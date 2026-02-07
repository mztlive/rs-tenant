# 09. FAQ 与故障排查

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](08-testing-benchmark.md) | [下一章](10-rs-tenant-vs-casbin.md)

## Q1: 为什么明明配了角色还是返回 `Deny`？

优先按顺序检查：

1. `tenant_active(tenant)` 是否为 `true`
2. `principal_active(tenant, principal)` 是否为 `true`
3. 权限字符串是否是 `resource:action`
4. 你请求的 `permission` 是否与角色权限完全一致（或 wildcard 可命中）

## Q2: 我配了 `invoice:*`，为什么仍然拒绝？

通常是 wildcard 未开启。请确认：

```rust
let engine = EngineBuilder::new(store).enable_wildcard(true).build();
```

若未开启，带 `*` 的权限不会匹配。

## Q3: 超级管理员为什么没有放行？

确认三件事：

1. `enable_super_admin(true)` 已开启
2. `is_super_admin(principal)` 返回 `true`
3. 当前 `tenant_active == true`

注意：超级管理员仍受租户启用状态约束。

## Q4: 权限更新后为什么行为没变化？

高概率是缓存未失效。权限关系变更后请调用：

- `invalidate_principal`
- `invalidate_role`
- `invalidate_tenant`

如果你还没有可靠失效机制，建议先暂时关闭缓存。

## Q5: `401`、`403`、`500` 怎么区分？

在 Axum 集成中：

- `401`：缺少认证上下文或 JWT 无效
- `403`：已认证但权限不足
- `500`：授权查询过程异常（例如 Store 读取失败）

## Q6: 如何快速定位线上授权问题？

建议固定输出一条结构化日志（至少包含以下字段）：

- `tenant`
- `principal`
- `permission` 或 `resource`
- `decision`
- `engine_flags`（wildcard/hierarchy/super-admin）

同时记录缓存命中与否，可快速区分“数据问题”还是“配置问题”。

## 排查 Checklist

1. 看配置：feature 与 builder 开关是否符合预期  
2. 看数据：租户/主体是否 active，角色关系是否完整  
3. 看字符串：权限格式、大小写、空格  
4. 看缓存：是否失效，是否命中旧数据  
5. 看异常：Store 错误是否被正确上报

## 继续阅读

- [上一页：08. 测试与性能基准](08-testing-benchmark.md)
- [下一页：10. rs-tenant 与 Casbin 对比](10-rs-tenant-vs-casbin.md)
- [回到文档首页](README.md)
- [返回目录](SUMMARY.md)

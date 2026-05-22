# rs-tenant 开发者使用手册

这份文档面向准备把 `rs-tenant` 接入 Rust SaaS 服务的开发者。阅读顺序按接入路径组织：先跑通一个最小授权，再把它接到数据库、Web 层、平台后台和测试体系里。

`rs-tenant` 只做授权计算：

- 从你的数据源读取租户、成员、角色、权限和授权范围。
- 返回 `AccessDecision`，告诉你是否允许。
- 返回 `AccessScope` 或 `TenantDataAccessScope`，让你把可见范围下推到 SQL、ORM 或搜索条件。

它不提供用户系统、租户管理、角色管理后台、ORM、迁移脚本或审计日志。

## 推荐阅读路径

1. [01. 先理解它解决什么问题](01-overview.md)
2. [02. 把业务概念映射到授权模型](02-domain-model.md)
3. [03. 选择正确的授权 API](03-authorization-flow.md)
4. [04. 5 分钟跑通第一个授权](04-quickstart.md)
5. [05. 接入生产数据源](05-integration-production.md)
6. [06. 接入 Axum 和 JWT](06-axum-integration.md)
7. [07. 常见业务场景](07-examples.md)
8. [08. 测试、缓存和性能验证](08-testing-benchmark.md)
9. [09. FAQ 与故障排查](09-faq-troubleshooting.md)
10. [10. 和 Casbin 怎么取舍](10-rs-tenant-vs-casbin.md)
11. [11. 平台授权：平台员工和跨租户数据](11-platform-authorization.md)
12. [12. 性能基线记录](12-perf-baseline.md)

历史设计记录放在附录：

- [v0.3 重构方案](redesign-v0.3.md)
- [v0.4 平台授权设计方案](redesign-v0.4.md)

## 接入前你需要准备什么

- 一个已经完成认证的主体 id，比如用户 id、员工 id 或账号 id。
- 一个当前租户 id。
- 一套角色分配数据：谁在某个租户下拥有哪个角色。
- 一套角色权限数据：角色拥有哪些 `resource:action` 权限。
- 一套授权范围数据：每次角色分配覆盖全租户，还是覆盖某些层级路径。

如果你还没有真实数据库表，可以先用 `memory-store` feature 跑通示例。

## 最重要的规则

- 默认拒绝：任何缺失、禁用、没有匹配权限、没有显式范围或数据源错误都不会放行。
- 范围绑定在角色分配上，不绑定在用户或 membership 上。
- 列表接口先调用 `accessible_scope`，再把结果转成查询条件。
- 访问单个对象时，先从数据库查出对象真实归属路径，再调用 `can_access_scope`。
- 租户级操作使用 `can_tenant`，路径级授权不会被当成全租户授权。
- 平台员工使用 `platform` feature 下的 `PlatformEngine`，不要塞进租户内 `Engine`。

## 本地预览

```bash
mdbook serve
```

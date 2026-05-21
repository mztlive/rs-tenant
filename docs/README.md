# rs-tenant 文档

`rs-tenant` v0.3.0 是一个租户内 RBAC 授权内核。它不托管业务模型，不提供平台级绕过能力，也不实现通用策略语言；它只根据租户、主体、角色分配、权限和授权范围计算访问结果。

## 快速入口

- [01. 项目总览](01-overview.md)
- [02. 领域模型与权限语义](02-domain-model.md)
- [03. 授权流程详解](03-authorization-flow.md)
- [04. 5 分钟快速接入](04-quickstart.md)
- [05. 生产环境集成指南](05-integration-production.md)
- [06. Axum 与 JWT 集成](06-axum-integration.md)
- [07. 典型案例](07-examples.md)
- [08. 测试与性能基准](08-testing-benchmark.md)
- [09. FAQ 与故障排查](09-faq-troubleshooting.md)
- [10. Casbin 边界](10-rs-tenant-vs-casbin.md)
- [v0.3 重构方案](redesign-v0.3.md)
- [v0.4 平台授权设计方案](redesign-v0.4.md)

## 你将学到什么

- 如何用 `AuthSubject` 表达租户内主体。
- 如何用 `GrantScope` 表达角色分配授予的范围。
- 如何用 `AccessScope` 作为查询前过滤条件。
- 何时调用 `accessible_scope`、`can_access_scope`、`can_tenant`。
- 如何实现 `AuthorizationSource` 接入生产数据库。
- 如何使用 `MemorySource` 和 `MemoryCache` 做本地验证与缓存。
- 为什么 v0.3 删除旧 `authorize/scope/Store/GlobalRole/SuperAdmin/casbin` 兼容层。

## 推荐阅读路径

业务接入优先：

1. [01. 项目总览](01-overview.md)
2. [04. 5 分钟快速接入](04-quickstart.md)
3. [07. 典型案例](07-examples.md)
4. [09. FAQ 与故障排查](09-faq-troubleshooting.md)

架构设计优先：

1. [02. 领域模型与权限语义](02-domain-model.md)
2. [03. 授权流程详解](03-authorization-flow.md)
3. [05. 生产环境集成指南](05-integration-production.md)
4. [08. 测试与性能基准](08-testing-benchmark.md)
5. [10. Casbin 边界](10-rs-tenant-vs-casbin.md)
6. [v0.4 平台授权设计方案](redesign-v0.4.md)

完整目录见 [SUMMARY.md](SUMMARY.md)。

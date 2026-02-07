# rs-tenant 文档

本项目是一个多租户 RBAC 授权库，核心目标是把“授权决策”与“业务存储”解耦。你可以把它当成一个可插拔的权限引擎：业务系统负责实现数据读取，`rs-tenant` 负责计算 `Allow / Deny` 和资源访问范围（`Scope`）。

## 快速入口

- [从项目总览开始](01-overview.md)
- [查看完整目录](SUMMARY.md)
- [5 分钟快速接入](04-quickstart.md)
- [生产环境集成指南](05-integration-production.md)
- [Axum 与 JWT 集成](06-axum-integration.md)
- [FAQ 与故障排查](09-faq-troubleshooting.md)
- [rs-tenant 与 Casbin 对比](10-rs-tenant-vs-casbin.md)

## 你将学到什么

- 设计概念：租户角色、平台角色、超级管理员、权限字符串模型
- 授权流程：`authorize` 与 `scope` 的真实执行顺序
- 集成方法：从内存版快速跑通，到生产自定义 `Store`，再到 Axum/JWT 中间件接入
- 案例实践：常见授权组合与排查思路

## 推荐阅读路径

### 路径 A：业务开发者（优先接入）
1. `01-overview.md`
2. `04-quickstart.md`
3. `06-axum-integration.md`
4. `07-examples.md`
5. `09-faq-troubleshooting.md`

### 路径 B：架构/平台开发者（优先设计）
1. `01-overview.md`
2. `02-domain-model.md`
3. `03-authorization-flow.md`
4. `10-rs-tenant-vs-casbin.md`
5. `05-integration-production.md`
6. `08-testing-benchmark.md`
7. `09-faq-troubleshooting.md`

## 版本与范围说明

- 文档基于当前仓库实现编写（`0.1.x` 语义）
- 默认策略：`Deny by default`
- 本库不负责 ORM、迁移、业务模型托管

详细目录见 `SUMMARY.md`。

## 下一步

- [进入第 01 章：项目总览](01-overview.md)

# 01. 项目总览

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [下一章](02-domain-model.md)

## 项目定位

`rs-tenant` 是一个多租户 RBAC 授权库，聚焦三件事：

1. 在租户上下文内做授权判定：`authorize(tenant, principal, permission)`
2. 在资源查询前给出访问范围：`scope(tenant, principal, resource)`
3. 通过可插拔 `Store` trait 连接你的数据库与缓存

## 适用与不适用场景

适用：
- SaaS 多租户系统
- 需要“租户角色 + 平台全局角色”组合授权
- 希望把权限决策从业务代码里抽离

不适用：
- 以复杂 ABAC 属性策略为主的系统
- 希望库内直接管理 ORM 模型和数据库迁移

## 核心设计原则

### 1) Deny by default
只要任一关键前置条件不满足，就直接 `Deny`，避免“漏配即放行”。

### 2) 存储与引擎解耦
引擎不关心你用 MySQL、PostgreSQL 或其他存储；你只需实现 `Store` 的异步读取能力。

### 3) 可选能力按 feature 开启
常用 feature：
- `memory-store`：内存存储（测试/演示）
- `memory-cache`：内存缓存
- `axum`：Axum 授权中间件
- `axum-jwt`：JWT 解析 + Axum
- `serde`：类型序列化

## 一张图理解执行边界

- 业务系统负责：身份来源、权限数据落库、数据变更触发缓存失效
- `rs-tenant` 负责：读取权限关系、计算结果、返回 `Decision` 或 `Scope`

## 典型接入顺序

1. 用 `memory-store` 跑通最小授权链路
2. 按业务表结构实现 `TenantStore` / `RoleStore` / `GlobalRoleStore`
3. 根据吞吐需求启用 `memory-cache`
4. Web 框架（如 Axum）里统一接入授权中间件

## 继续阅读

- [上一页：文档首页](README.md)
- [下一页：02. 领域模型与权限语义](02-domain-model.md)
- [返回目录](SUMMARY.md)

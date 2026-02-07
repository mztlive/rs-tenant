# 10. rs-tenant 与 Casbin 对比

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](09-faq-troubleshooting.md)

本章用于回答一个常见问题：在多租户业务里，应该选 `rs-tenant` 还是 Casbin。

## 一句话定位

- `rs-tenant`：面向 Rust 多租户 RBAC 的“业务集成型授权引擎”，强调类型安全与可插拔存储。
- Casbin：通用策略引擎，支持多模型（ACL/RBAC/ABAC 等），强调策略表达能力与跨语言生态。

## 核心差异对比

| 维度 | rs-tenant | Casbin |
|---|---|---|
| 模型定位 | 聚焦多租户 RBAC（租户角色 + 平台角色） | 通用授权框架（模型可配置） |
| 规则表达方式 | 代码配置 + Store 数据关系 | `model.conf` + policy 数据 + matcher 表达式 |
| 多租户语义 | 内建 `tenant` 维度与 `scope` 语义 | 可实现多租户，但需要自行建模与约束 |
| 类型安全（Rust） | 强类型 ID（`TenantId`/`PrincipalId` 等） | 通常以字符串策略为主 |
| 结果形态 | `Decision` + `Scope`（可直接用于数据过滤） | 以 `enforce` 判定为主 |
| 存储接入 | 实现 `TenantStore`/`RoleStore`/`GlobalRoleStore` | 通过适配器读写策略存储 |
| 业务集成成本 | 对 Rust 多租户场景较低 | 初期灵活，但策略建模成本更高 |

## 语义层面的关键区别

### 1) 超级管理员与租户启用关系

`rs-tenant` 中，超级管理员是平台级能力，但仍受 `tenant_active` 约束。  
Casbin 是否有同等语义，取决于你如何写模型和策略。

### 2) 租户角色 + 平台角色并集

`rs-tenant` 默认按并集计算这两类权限。  
Casbin 也可以实现，但通常要自行在策略模型中表达并维护一致性。

### 3) 资源范围能力

`rs-tenant` 提供 `scope(...)` 直接返回 `TenantOnly` 或 `None`，适合在查询层做预过滤。  
Casbin 主要返回布尔判定，范围控制一般需要你额外设计。

## 选型建议

优先选择 `rs-tenant`：

- 你的核心场景是 Rust + 多租户 RBAC
- 需要较强的业务语义约束（如租户启用、主体启用、超级管理员开关）
- 想用 trait 对接现有数据库，并由代码保证类型正确

优先选择 Casbin：

- 需要跨语言统一授权框架
- 需要 RBAC 之外的大量动态策略表达（复杂 matcher/ABAC）
- 团队已有成熟 Casbin 规则与运维体系

## 迁移与共存建议

1. 新系统可优先落 `rs-tenant`，减少策略模型设计成本。  
2. 已有 Casbin 系统可先保持策略层不动，把高频多租户链路逐步迁到 `rs-tenant`。  
3. 不建议同一条请求链路里同时做两套独立“最终授权”判定，避免语义漂移。  

## 当前仓库说明

本仓库存在 `casbin` feature 开关，但当前未提供公开 Casbin 适配 API。  
如果需要 Casbin 深度对接，建议在项目层新增适配层，明确“谁是最终决策源”。

## 继续阅读

- [上一页：09. FAQ 与故障排查](09-faq-troubleshooting.md)
- [回到文档首页](README.md)
- [返回目录](SUMMARY.md)

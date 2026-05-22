# 10. Casbin 边界

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](09-faq-troubleshooting.md)

v0.3.0 删除空的 `casbin` feature。v0.4.0 新增 `platform` feature，但它不是 Casbin adapter，也不是通用策略语言。

## 一句话区别

- `rs-tenant`：SaaS RBAC 授权内核，强调强类型、范围计算和业务查询前过滤；v0.4.0 增加 sibling 平台授权。
- Casbin：通用策略引擎，强调模型配置、matcher 表达能力和跨语言生态。

## 核心差异

| 维度 | rs-tenant v0.4 | Casbin |
|---|---|---|
| 定位 | 租户内 RBAC core + 可选平台授权 | 通用策略框架 |
| 主体模型 | `AuthSubject { tenant, principal }` / `PlatformSubject { principal }` | 由模型和策略自行定义 |
| 权限模型 | `Permission { resource, action }` | 字符串策略和 matcher |
| 范围模型 | `GrantScope` -> `AccessScope`，`PlatformGrantScope` -> `TenantDataAccessScope` | 需要自行建模 |
| 查询前过滤 | `accessible_scope(ScopeQuery)` / `accessible_tenants(TenantDataScopeQuery)` | 通常需要额外设计 |
| 目标点判定 | `can_access_scope(...)` / `can_access_tenant_scope(...)` | `enforce(...)` 或自定义 |
| 租户级判定 | `can_tenant(...)` / `can_access_tenant(...)` | 由 matcher 表达 |
| 数据源 | `AuthorizationSource` / `PlatformAuthorizationSource` 只读授权数据 | adapter 读写 policy |
| 平台能力 | 独立 `platform` feature，无全局 bypass | 可自行建模 |

## 为什么不保留空 feature？

空 feature 会让调用方误以为：

- 已经存在 Casbin adapter。
- Casbin policy 可以直接映射为 role assignment scope。
- 两套最终判定可以安全叠加。

这些假设都不成立。v0.3 选择删除空开关，避免语义漂移。

v0.4 的 `platform` feature 也不改变这一点。它提供的是 `PlatformEngine`、`PlatformAuthorizationSource`、`PlatformGrantScope` 和 `TenantDataAccessScope`，不是 Casbin adapter。

## 未来如果要适配 Casbin

必须先明确：

1. Casbin 是最终决策源，还是只是授权数据源。
2. tenant status 和 membership status 由谁维护。
3. `RoleAssignment.scope` 如何从 policy 映射。
4. `GrantScope::Tenant` 与 `GrantScope::Paths` 如何表达。
5. 缓存失效由谁触发。
6. explain 结果如何对齐。

不建议在同一请求链路里同时做：

```rust
engine.can_tenant(request).await? == AccessDecision::Allow
    && casbin.enforce(args)?
```

两个最终判定源会制造语义漂移。若需要共存，应明确一个系统是最终授权源，另一个只作为数据来源或迁移过渡工具。

## 选型建议

优先选择 `rs-tenant`：

- 核心场景是 Rust + SaaS 租户内 RBAC。
- 需要平台账号访问平台自身资源或跨租户数据范围，但不希望引入全局绕过。
- 需要把授权范围转成数据库查询条件。
- 希望用 Rust 类型约束权限、主体和范围。
- 希望 core 只读授权数据，不托管 policy 管理后台。

优先选择 Casbin：

- 需要跨语言统一策略体系。
- 需要复杂 ABAC、动态 matcher 或多授权模型。
- 团队已有成熟 Casbin policy、adapter 和运维体系。

## 继续阅读

- [上一章：09. FAQ 与故障排查](09-faq-troubleshooting.md)
- [下一章：11. 平台授权](11-platform-authorization.md)
- [回到文档首页](README.md)
- [返回目录](SUMMARY.md)

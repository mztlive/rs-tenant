# 10. 和 Casbin 怎么取舍

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](09-faq-troubleshooting.md) | [下一章](11-platform-authorization.md)

`rs-tenant` 和 Casbin 都可以参与授权，但定位不同。选择前先看你的核心问题是什么。

## 一句话

- `rs-tenant`：Rust SaaS RBAC 授权内核，重点是强类型、租户内范围计算和查询前过滤。
- Casbin：通用策略引擎，重点是模型配置、matcher 表达能力和跨语言生态。

## 对比

| 维度 | rs-tenant | Casbin |
|---|---|---|
| 主要场景 | Rust SaaS 租户内 RBAC | 通用访问控制 |
| 主体模型 | 固定为 `AuthSubject`，平台另有 `PlatformSubject` | 由模型定义 |
| 权限模型 | `resource:action` | policy 字符串和 matcher |
| 数据范围 | 内置 `GrantScope` / `AccessScope` | 需要自行建模 |
| 查询前过滤 | 一等 API：`accessible_scope` | 通常需要额外设计 |
| 数据源 | 只读 `AuthorizationSource` | adapter 读写 policy |
| 平台员工 | 可选 `platform` feature | 由模型自行表达 |
| super admin | 不提供全局绕过 | 可自行建模 |

## 什么时候选 rs-tenant

优先选 `rs-tenant`：

- 服务主要用 Rust 写。
- 核心是 SaaS 租户内 RBAC。
- 权限需要和门店、区域、组织树等层级范围绑定。
- 列表、搜索、导出必须先计算可见范围。
- 团队希望授权规则在 Rust 类型和 API 里固定下来。
- 你已经有用户、租户、角色管理系统，只缺授权内核。

## 什么时候选 Casbin

优先选 Casbin：

- 多语言服务需要共享一套策略体系。
- 需要复杂 ABAC 或动态 matcher。
- 授权模型经常变化，不能固定在 Rust 类型里。
- 团队已有成熟 Casbin policy、adapter、管理后台和运维经验。

## 能不能一起用

可以，但要明确谁是最终决策源。

不建议同一请求里同时做两个最终判定：

```rust
engine.can_tenant(request).await? == AccessDecision::Allow
    && casbin.enforce(args)?
```

这样会出现两套模型同时解释权限，后续很难排查。

更清晰的方式是：

- Casbin 作为最终决策源，`rs-tenant` 不参与这条链路。
- 或 `rs-tenant` 作为最终决策源，Casbin 只作为迁移期数据来源。

## 为什么没有 `casbin` feature

旧版本里空的 `casbin` feature 容易让调用方误解为：

- 已经存在 Casbin adapter。
- Casbin policy 可以自动映射为 `GrantScope`。
- 两套最终判定可以安全叠加。

这些假设都不成立，所以当前版本不保留这个空 feature。

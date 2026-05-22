# 03. 选择正确的授权 API

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](02-domain-model.md) | [下一章](04-quickstart.md)

`rs-tenant` 刻意不提供一个语义模糊的 `can(...)`。你需要按业务场景选择 API。

## 列表、搜索、导出：`accessible_scope`

当接口要返回一批数据时，先计算可访问范围，再把范围下推到查询。

```rust
use rs_tenant::{AccessScope, Permission, ScopeQuery};

let scope = engine
    .accessible_scope(ScopeQuery {
        subject,
        permission: Permission::parse("order:read")?,
    })
    .await?;

match scope {
    AccessScope::None => {
        // 返回空列表，或按你的产品策略返回 403
    }
    AccessScope::Tenant { tenant } => {
        // WHERE tenant_id = ?
    }
    AccessScope::Paths { tenant, roots } => {
        // WHERE tenant_id = ? AND scope_path is under any root
    }
}
```

适合：

- 订单列表。
- 客户搜索。
- 导出报表。
- 看板聚合查询。

## 单个对象：`can_access_scope`

当接口访问一个确定对象时，先加载对象并得到它的真实 `ScopePath`，再做点判定。

```rust
use rs_tenant::{AccessDecision, Permission, ScopedAccessRequest};

let order = repo.load_order(order_id).await?;
let decision = engine
    .can_access_scope(ScopedAccessRequest {
        subject,
        permission: Permission::parse("order:update")?,
        target: order.scope_path(),
    })
    .await?;

if decision == AccessDecision::Deny {
    // 返回 403
}
```

适合：

- 读取订单详情。
- 修改某个客户。
- 删除某个门店下的配置。
- 下载某个具体文件。

重点：目标路径来自业务数据，不来自客户端自报。

## 租户级操作：`can_tenant`

当操作没有更细的业务对象路径，且必须要求全租户范围时，使用 `can_tenant`。

```rust
use rs_tenant::{AccessDecision, Permission, TenantAccessRequest};

let decision = engine
    .can_tenant(TenantAccessRequest {
        subject,
        permission: Permission::parse("tenant/settings:update")?,
    })
    .await?;
```

适合：

- 租户设置。
- 租户级账单配置。
- 全租户报表开关。

如果主体只有某些路径的授权，`can_tenant` 会拒绝。它不会把路径级授权升级为全租户授权。

## 排查问题：`explain_*`

线上主链路通常只需要 decision 或 scope。测试、日志和排障可以使用解释 API：

```rust
let explanation = engine
    .explain_tenant(TenantAccessRequest {
        subject,
        permission: Permission::parse("tenant/settings:update")?,
    })
    .await?;
```

解释结果包含：

- `decision`
- `reason`
- `scope`

数据源错误仍然通过 `Err` 返回，不会被伪装成 deny reason。

## 引擎内部按什么顺序判断

租户内主流程是：

1. 读取租户状态。
2. 读取成员状态。
3. 读取角色分配。
4. 如果启用角色继承，展开父角色。
5. 读取角色权限。
6. 匹配请求权限。
7. 合并命中的授权范围。
8. 按调用的 API 返回 scope 或 decision。

任意状态无效、权限不匹配或范围不覆盖，最终都会拒绝。

## 角色继承和 wildcard

默认建议先关闭，业务确实需要时再开启：

```rust
let engine = EngineBuilder::new(source)
    .enable_role_hierarchy(true)
    .enable_wildcard(true)
    .max_role_depth(16)
    .build();
```

角色继承只继承权限，不改变当前角色分配的范围。也就是说，如果 `store_reader` 继承了 `order_reader` 的权限，最终范围仍然是 `store_reader` 这次 assignment 上的 `GrantScope`。

## 平台授权 API 对照

启用 `platform` feature 后，对应 API 是：

| 场景 | API |
|---|---|
| 平台自身资源，如平台角色管理 | `PlatformEngine::can_platform` |
| 跨租户列表、搜索、导出 | `PlatformEngine::accessible_tenants` |
| 指定租户级平台操作 | `PlatformEngine::can_access_tenant` |
| 指定租户下的路径对象 | `PlatformEngine::can_access_tenant_scope` |

平台 API 详见 [11. 平台授权：平台员工和跨租户数据](11-platform-authorization.md)。

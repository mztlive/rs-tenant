# 11. 平台授权：平台员工和跨租户数据

> 导航：[首页](README.md) | [目录](SUMMARY.md) | [上一章](10-rs-tenant-vs-casbin.md)

启用 `platform` feature 后，可以给平台员工建模：他们可能访问平台后台资源，也可能在受控范围内查看或管理租户数据。

平台授权不是 super admin。它不会跳过租户内 `Engine`，也不会把平台员工伪装成租户成员。

## Step 1: 启用 feature

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["platform"] }
```

示例和测试通常再加 `memory-store`：

```toml
[dependencies]
rs-tenant = { version = "0.4.0", features = ["memory-store", "platform"] }
```

## Step 2: 选择平台范围

平台角色分配使用 `PlatformGrantScope`：

| 范围 | 用途 |
|---|---|
| `Platform` | 访问平台自身资源，如平台角色管理 |
| `AllTenants` | 对某个 permission 拥有所有租户数据范围 |
| `Tenants` | 对某个 permission 拥有明确租户集合 |
| `TenantPaths` | 对某个 permission 拥有指定租户下的指定路径 |

这四种范围不能混用语义：

- `can_platform` 只接受 `Platform`。
- `accessible_tenants` 只返回租户数据范围。
- `TenantPaths` 不能被 `can_access_tenant` 当成租户级权限。

## Step 3: 跑通内存示例

```rust
use rs_tenant::{
    AccessDecision, Permission, TenantId,
    platform::{
        MemoryPlatformSource, PlatformEngineBuilder, PlatformGrantScope, PlatformPrincipalId,
        PlatformPrincipalStatus, PlatformRoleId, PlatformSubject, TenantDataAccessRequest,
    },
};

async fn support_can_access_one_tenant() -> rs_tenant::Result<()> {
    let principal = PlatformPrincipalId::parse("support_1")?;
    let role = PlatformRoleId::parse("tenant_support")?;
    let tenant = TenantId::parse("tenant_a")?;
    let permission = Permission::parse("tenant/order:read")?;

    let source = MemoryPlatformSource::new();
    source.set_principal_status(principal.clone(), PlatformPrincipalStatus::Active);
    source.add_role_assignment(
        principal.clone(),
        role.clone(),
        PlatformGrantScope::tenants(vec![tenant.clone()])?,
    );
    source.add_role_permission(role, permission.clone());

    let engine = PlatformEngineBuilder::new(source).build();
    let decision = engine
        .can_access_tenant(TenantDataAccessRequest {
            subject: PlatformSubject::new(principal),
            permission,
            tenant,
        })
        .await?;

    assert_eq!(decision, AccessDecision::Allow);
    Ok(())
}
```

也可以直接运行仓库示例：

```bash
cargo run --example platform --features memory-store,platform
```

## Step 4: 实现平台数据源

生产环境实现 `PlatformAuthorizationSource`：

```rust
use async_trait::async_trait;
use rs_tenant::{
    Permission, SourceError,
    platform::{
        PlatformAuthorizationSource, PlatformPrincipalStatus, PlatformRoleAssignment,
        PlatformRoleId, PlatformSubject,
    },
};

#[derive(Clone)]
pub struct DbPlatformAuthorizationSource;

#[async_trait]
impl PlatformAuthorizationSource for DbPlatformAuthorizationSource {
    async fn platform_principal_status(
        &self,
        subject: &PlatformSubject,
    ) -> Result<PlatformPrincipalStatus, SourceError> {
        let _ = subject;
        Ok(PlatformPrincipalStatus::Active)
    }

    async fn platform_role_assignments(
        &self,
        subject: &PlatformSubject,
    ) -> Result<Vec<PlatformRoleAssignment>, SourceError> {
        let _ = subject;
        Ok(Vec::new())
    }

    async fn platform_role_permissions(
        &self,
        role: &PlatformRoleId,
    ) -> Result<Vec<Permission>, SourceError> {
        let _ = role;
        Ok(Vec::new())
    }

    async fn platform_parent_roles(
        &self,
        role: &PlatformRoleId,
    ) -> Result<Vec<PlatformRoleId>, SourceError> {
        let _ = role;
        Ok(Vec::new())
    }
}
```

平台 Source 只读取授权数据，不创建租户、不写审计、不更新角色。

## Step 5: 平台自身资源

平台角色管理、平台权限配置、租户创建入口等属于平台自身资源，用 `can_platform`。

```rust
use rs_tenant::{
    Permission,
    platform::{PlatformAccessRequest, PlatformSubject},
};

let decision = platform_engine
    .can_platform(PlatformAccessRequest {
        subject: PlatformSubject::new(platform_principal),
        permission: Permission::parse("platform/role:update")?,
    })
    .await?;
```

这个调用只接受 `PlatformGrantScope::platform()` 授权。

## Step 6: 跨租户列表和导出

跨租户查询先拿 `TenantDataAccessScope`：

```rust
use rs_tenant::{
    Permission,
    platform::{TenantDataAccessScope, TenantDataScopeQuery},
};

let scope = platform_engine
    .accessible_tenants(TenantDataScopeQuery {
        subject,
        permission: Permission::parse("tenant/order:read")?,
    })
    .await?;

match scope {
    TenantDataAccessScope::None => {}
    TenantDataAccessScope::AllTenants => {
        // query all tenants, still scoped by this permission
    }
    TenantDataAccessScope::Tenants { tenants } => {
        // WHERE tenant_id IN (...)
    }
    TenantDataAccessScope::TenantPaths { entries } => {
        // WHERE tenant_id/path matches any entry
    }
}
```

`AllTenants` 只对当前 permission 生效，不是全系统绕过。

## Step 7: 指定租户或指定路径

指定租户级操作：

```rust
use rs_tenant::platform::TenantDataAccessRequest;

let decision = platform_engine
    .can_access_tenant(TenantDataAccessRequest {
        subject,
        permission,
        tenant,
    })
    .await?;
```

指定租户下的路径对象：

```rust
use rs_tenant::platform::TenantScopedDataAccessRequest;

let decision = platform_engine
    .can_access_tenant_scope(TenantScopedDataAccessRequest {
        subject,
        permission,
        tenant,
        target,
    })
    .await?;
```

如果平台员工只有 `TenantPaths` 范围，`can_access_tenant` 会拒绝，必须使用带 `target` 的路径判定。

## 建模建议

- 平台角色表和租户角色表分开。
- 平台权限命名可以用 `platform/...` 表示平台自身资源，用 `tenant/...` 表示租户数据管理权限。
- 平台数据范围仍然绑定在角色分配上。
- 审批、原因、过期时间和审计日志由应用层维护。
- 不要在租户内 `Engine` 外层加“平台用户直接 allow”的分支。

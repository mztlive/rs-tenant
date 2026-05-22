# rs-tenant v0.6 权限目录与角色 API 模型设计方案

> 状态：0.6.0 设计草案
> 目标版本：`0.6.0`
> 范围：权限目录、权限分组、租户与平台角色 API DTO、前端导出模型、profile 权限读取
> 兼容策略：依赖 v0.5 IAM Service，不改变 core 和 platform 判定语义

## 1. 背景

v0.5 解决“角色、绑定、权限管理代码散落”的问题，但还有一个高频重复点没有收敛：权限目录。

很多业务系统需要同时维护：

- 后端 permission 常量。
- 角色编辑页权限树。
- profile 返回的权限列表。
- 菜单和按钮权限控制。
- 前端类型定义。
- 文档里的权限说明。

如果权限目录仍然由应用项目自己通过宏、`build.rs`、前端生成文件拼起来，接入成本还是很高，而且容易出现前后端权限不一致。平台后台同样有这个问题：平台角色、平台自身资源权限、跨租户数据权限也需要目录、展示分组、前端常量和 profile 返回。

v0.6 的目标是让 `Permission` 不只是一个可解析字符串，而是有官方目录模型、分组模型、导出模型和 role API DTO。

## 2. 目标

v0.6 需要提供：

- `PermissionDefinition`：权限元数据。
- `PermissionCatalog`：权限目录和校验入口。
- `PermissionGroup`：角色编辑页可直接使用的分组结构。
- `RoleView`、`RoleDetail`、`RolePermissionView` 等通用 API 模型。
- profile 权限读取模型。
- JSON/TypeScript 导出能力。
- v0.5 `TenantIamService` 对 catalog 的可选约束。
- v0.5 `PlatformIamService` 对 catalog 的可选约束。
- 平台角色 API DTO 和平台 profile 权限读取模型。

最终接入方应该只需要维护一份权限定义：

```rust
let catalog = PermissionCatalog::new([
    PermissionDefinition::new("order:read", "订单查看").group("订单"),
    PermissionDefinition::new("order:update", "订单编辑").group("订单"),
    PermissionDefinition::new("platform/role:update", "平台角色编辑").group("平台管理"),
    PermissionDefinition::new("tenant/order:read", "平台查看租户订单").group("平台租户数据"),
])?;
```

然后后端校验、租户角色编辑页、平台角色编辑页、profile、前端权限常量都从它派生。

## 3. 非目标

v0.6 不做以下事情：

- 不决定菜单结构。
- 不决定页面路由。
- 不绑定 React、Vue、Flutter 或具体前端框架。
- 不提供后台 UI 组件。
- 不把权限目录变成动态策略语言。
- 不让 catalog 参与最终授权判定；最终判定仍由 `Engine` 使用角色分配和权限数据完成。
- 不把平台权限和租户权限强行拆成两个字符串类型；二者仍共享 `Permission`，通过命名约定和 service 入口区分语义。

## 4. 模块边界

建议继续放在 `iam` feature 下，或新增细分 feature：

```toml
[features]
iam = []
catalog = ["iam"]
platform-iam = ["platform", "iam"]
platform-catalog = ["platform-iam", "catalog"]
```

模块结构：

```text
src/
  iam/
    catalog.rs
    api.rs
    export.rs
```

`catalog` 是元数据层，`service` 是管理层，`engine` 是判定层。

```text
PermissionCatalog
        |
        +-- TenantIamService 写入校验
        +-- PlatformIamService 写入校验
        +-- API DTO 展示
        +-- 前端导出
        +-- 文档生成

Engine 只读取 Permission，不依赖展示元数据
PlatformEngine 也只读取 Permission，不依赖展示元数据
```

## 5. PermissionDefinition

建议类型：

```rust
pub struct PermissionDefinition {
    pub permission: Permission,
    pub name: String,
    pub description: Option<String>,
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub deprecated: bool,
}
```

构造 API：

```rust
impl PermissionDefinition {
    pub fn new(permission: impl AsRef<str>, name: impl Into<String>) -> Result<Self>;
    pub fn description(self, value: impl Into<String>) -> Self;
    pub fn group(self, value: impl Into<String>) -> Self;
    pub fn tag(self, value: impl Into<String>) -> Self;
    pub fn deprecated(self, value: bool) -> Self;
}
```

规则：

- `permission` 使用现有 `Permission::parse`。
- `name` trim 后不能为空。
- `description` trim 后为空则归一为 `None`。
- `group` trim 后为空则归一为 `None`。
- `tags` 去重并保持稳定排序。
- `deprecated = true` 的权限仍可存在于角色中，但新写入时可配置为拒绝。

## 6. PermissionCatalog

```rust
pub struct PermissionCatalog {
    definitions: Vec<PermissionDefinition>,
}
```

核心 API：

```rust
impl PermissionCatalog {
    pub fn new(definitions: impl IntoIterator<Item = PermissionDefinition>) -> Result<Self>;

    pub fn contains(&self, permission: &Permission) -> bool;
    pub fn get(&self, permission: &Permission) -> Option<&PermissionDefinition>;
    pub fn definitions(&self) -> &[PermissionDefinition];
    pub fn groups(&self) -> Vec<PermissionGroup>;

    pub fn validate_all<'a>(
        &self,
        permissions: impl IntoIterator<Item = &'a Permission>,
    ) -> Result<()>;
}
```

构造规则：

- 权限不能重复。
- 同一个权限不能有冲突的展示名。
- 输出顺序稳定。
- 默认按定义顺序输出，分组内也保持定义顺序。

## 7. PermissionGroup

```rust
pub struct PermissionGroup {
    pub key: String,
    pub name: String,
    pub permissions: Vec<PermissionDefinition>,
}
```

规则：

- 没有 group 的权限归入默认分组。
- 默认分组 key 可配置，默认值为 `default`。
- `key` 由 group name 归一化得到，避免前端重复处理。

这个结构可以直接用于角色编辑页：

```json
[
  {
    "key": "order",
    "name": "订单",
    "permissions": [
      { "permission": "order:read", "name": "订单查看" },
      { "permission": "order:update", "name": "订单编辑" }
    ]
  }
]
```

## 8. TenantIamService 集成

v0.6 让 v0.5 的 service 支持 catalog：

```rust
let service = TenantIamService::builder(store)
    .permission_catalog(catalog)
    .reject_unknown_permissions(true)
    .reject_deprecated_permissions(false)
    .build();
```

影响：

- `set_role_permissions` 可拒绝未登记权限。
- `permissions(subject)` 可以返回带展示信息的 view。
- `role_detail` 可以返回角色权限和 catalog 对照结果。

新增方法：

```rust
pub async fn role_detail(
    &self,
    tenant: &TenantId,
    role: &RoleId,
) -> Result<RoleDetail>;

pub async fn permission_catalog(&self) -> PermissionCatalogView;

pub async fn subject_permissions(
    &self,
    subject: &AuthSubject,
) -> Result<SubjectPermissions>;
```

## 9. PlatformIamService 集成

平台 service 使用同一个 `PermissionCatalog`，但写入和读取入口必须是平台类型：

```rust
let platform_service = PlatformIamService::builder(platform_store)
    .permission_catalog(catalog)
    .reject_unknown_permissions(true)
    .reject_deprecated_permissions(false)
    .build();
```

影响：

- `set_platform_role_permissions` 可拒绝未登记权限。
- `platform_permissions(subject)` 可以返回带展示信息的 view。
- `platform_role_detail` 可以返回平台角色权限和 catalog 对照结果。
- `accessible_tenants`、`can_access_tenant_scope` 的最终判定仍只依赖 `PlatformEngine`，catalog 不参与 allow/deny。

新增方法：

```rust
pub async fn platform_role_detail(
    &self,
    role: &PlatformRoleId,
) -> Result<PlatformRoleDetail>;

pub async fn permission_catalog(&self) -> PermissionCatalogView;

pub async fn platform_subject_permissions(
    &self,
    subject: &PlatformSubject,
) -> Result<PlatformSubjectPermissions>;
```

平台权限命名建议：

| 类型 | 示例 | 说明 |
|---|---|---|
| 平台自身资源 | `platform/role:update` | 只应由 `can_platform` 判定 |
| 平台管理租户数据 | `tenant/order:read` | 由 `accessible_tenants` 或 `can_access_tenant_scope` 判定 |
| 租户内业务权限 | `order:read` | 由租户内 `Engine` 判定 |

Catalog 可以包含三类权限，但 service 要在写入时按入口约束用途。例如 `TenantIamService` 默认不应把 `platform/role:update` 写入租户角色，`PlatformIamService` 默认不应把普通 `order:read` 当成平台租户数据权限，除非应用显式放宽。

## 10. API DTO

### 10.1 RoleView

```rust
pub struct RoleView {
    pub id: RoleId,
    pub name: String,
    pub description: Option<String>,
    pub system: bool,
    pub disabled: bool,
    pub permission_count: usize,
    pub assignment_count: Option<usize>,
}
```

`assignment_count` 可选，因为某些 Store 不一定高效支持统计。

### 10.2 RoleDetail

```rust
pub struct RoleDetail {
    pub role: RoleView,
    pub permissions: Vec<RolePermissionView>,
    pub parent_roles: Vec<RoleView>,
}
```

### 10.3 RolePermissionView

```rust
pub struct RolePermissionView {
    pub permission: Permission,
    pub name: Option<String>,
    pub description: Option<String>,
    pub group: Option<String>,
    pub assigned: bool,
    pub deprecated: bool,
    pub unknown: bool,
}
```

规则：

- `unknown = true` 表示角色中已有该权限，但 catalog 没有登记。
- `deprecated = true` 表示 catalog 标记废弃。
- 角色编辑页可以用这个结构显示存量异常。

### 10.4 SubjectPermissions

```rust
pub struct SubjectPermissions {
    pub subject: AuthSubject,
    pub permissions: Vec<Permission>,
    pub definitions: Vec<PermissionDefinition>,
}
```

用途：

- profile 接口返回当前用户权限。
- 前端菜单和按钮基于权限判断。
- 调试当前主体有效权限。

### 10.5 PlatformRoleView

```rust
pub struct PlatformRoleView {
    pub id: PlatformRoleId,
    pub name: String,
    pub description: Option<String>,
    pub system: bool,
    pub disabled: bool,
    pub permission_count: usize,
    pub assignment_count: Option<usize>,
}
```

### 10.6 PlatformRoleDetail

```rust
pub struct PlatformRoleDetail {
    pub role: PlatformRoleView,
    pub permissions: Vec<RolePermissionView>,
    pub parent_roles: Vec<PlatformRoleView>,
}
```

`RolePermissionView` 可以被租户角色和平台角色复用，因为它描述的是权限元数据对照结果，不携带 role id。

### 10.7 PlatformSubjectPermissions

```rust
pub struct PlatformSubjectPermissions {
    pub subject: PlatformSubject,
    pub permissions: Vec<Permission>,
    pub definitions: Vec<PermissionDefinition>,
}
```

用途：

- 平台 profile 接口返回当前平台主体权限。
- 平台菜单和按钮基于权限判断。
- 调试当前平台主体有效权限。
- 区分平台自身权限和平台管理租户数据权限。

## 11. 导出能力

v0.6 应提供框架无关导出：

```rust
catalog.to_json()?;
catalog.to_typescript("AppPermission")?;
catalog.to_markdown()?;
```

### JSON

用于前端运行时读取：

```json
{
  "permissions": [
    {
      "permission": "order:read",
      "name": "订单查看",
      "group": "订单"
    }
  ],
  "groups": [
    {
      "key": "order",
      "name": "订单",
      "permissions": ["order:read"]
    }
  ]
}
```

### TypeScript

用于前端类型：

```ts
export type AppPermission =
  | "order:read"
  | "order:update";

export const APP_PERMISSIONS = [
  "order:read",
  "order:update",
] as const;
```

### Markdown

用于权限文档：

```md
## 订单

| 权限 | 名称 | 描述 |
|---|---|---|
| order:read | 订单查看 | - |
```

## 12. 权限和菜单的关系

v0.6 不直接建模菜单，但可以提供一个轻量 helper：

```rust
pub trait PermissionRequirement {
    fn is_satisfied_by(&self, permissions: &[Permission]) -> bool;
}
```

内置实现：

```rust
pub enum PermissionExpr {
    One(Permission),
    All(Vec<Permission>),
    Any(Vec<Permission>),
}
```

这个表达式可用于菜单、按钮、接口文档，但不要替代 `Engine` 的最终授权判定。

## 13. 迁移策略

从应用自维护权限常量迁移：

1. 保留原权限字符串不变。
2. 用 `PermissionDefinition` 包装已有权限。
3. 先启用 `reject_unknown_permissions(false)`，观察存量 unknown。
4. 修正角色权限数据。
5. 按租户权限、平台自身权限、平台租户数据权限补充 group/tag。
6. 再启用 `reject_unknown_permissions(true)`。
7. 前端改用 JSON 或 TypeScript 导出。

## 14. 测试清单

必须覆盖：

- catalog 拒绝重复权限。
- catalog 拒绝空展示名。
- group 输出顺序稳定。
- unknown role permission 能在 `RolePermissionView` 中显示。
- deprecated permission 能继续展示。
- `reject_unknown_permissions = true` 时写入被拒绝。
- `reject_unknown_permissions = false` 时存量权限可读取。
- JSON 导出稳定。
- TypeScript 导出稳定。
- Markdown 导出稳定。
- `SubjectPermissions` 能返回有效权限及对应 definition。
- `PlatformRoleDetail` 能显示平台角色的 unknown/deprecated 权限。
- `PlatformSubjectPermissions` 能返回平台主体有效权限及对应 definition。
- `TenantIamService` 默认拒绝平台自身权限写入租户角色。
- `PlatformIamService` 默认拒绝普通租户业务权限写入平台角色。

## 15. 实施阶段

### 阶段一：Catalog 类型

- 新增 `PermissionDefinition`。
- 新增 `PermissionCatalog`。
- 新增 `PermissionGroup`。
- 补构造校验和单元测试。

### 阶段二：Service 集成

- `TenantIamServiceBuilder` 支持 catalog。
- `set_role_permissions` 支持 unknown/deprecated 策略。
- 新增 `role_detail` 和 `subject_permissions`。
- `PlatformIamServiceBuilder` 支持 catalog。
- `set_platform_role_permissions` 支持 unknown/deprecated 策略。
- 新增 `platform_role_detail` 和 `platform_subject_permissions`。
- 增加权限用途约束，避免平台自身权限写入租户角色。

### 阶段三：导出

- JSON 导出。
- TypeScript 导出。
- Markdown 导出。
- 用快照测试保证稳定输出。

### 阶段四：文档与示例

- 增加角色编辑页数据流示例。
- 增加 profile 权限读取示例。
- 增加平台角色编辑页数据流示例。
- 增加平台 profile 权限读取示例。
- 增加前端常量导出示例。

## 16. 结论

v0.6 的价值是把“权限字符串”提升为“权限目录”。

完成后，业务项目应只维护一份权限定义：

```text
PermissionCatalog
        |
        +-- 后端角色权限校验
        +-- 角色编辑页权限树
        +-- 平台角色编辑页权限树
        +-- profile 权限返回
        +-- 平台 profile 权限返回
        +-- 前端类型和常量导出
        +-- Markdown 权限文档
```

这能显著减少接入项目在权限目录、角色编辑、前端权限常量上的重复代码。

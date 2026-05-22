# rs-tenant v0.7 Axum 产品化接入设计方案

> 状态：0.7.0 设计草案
> 目标版本：`0.7.0`
> 范围：Axum 高层 builder、认证上下文装配、路由授权 guard、手动判定 helper、标准错误响应
> 兼容策略：基于 v0.5 Store/Service 和 v0.6 Catalog，不移除现有低层 Axum API

## 1. 背景

当前 `axum` feature 已经提供基础能力：

- `AuthContext`
- `TenantAuthorizeLayer`
- `PlatformAuthorizeLayer`
- `axum-jwt` 默认 JWT layer
- `can_access_scope` helper

但接入体验仍然偏底层。应用项目通常还要自己写：

- JWT claims 到 `AuthSubject` 的映射。
- tenant/principal 从 header、path、session 中提取的逻辑。
- Router 上如何统一挂认证和授权 layer。
- handler 里如何拿当前 subject。
- 403/401/500 的统一响应。
- profile 权限接口。
- 手动调用 `accessible_scope` 时的样板代码。

v0.7 的目标是让 Axum 接入变成一个产品化 builder：应用只把 Store、JWT 映射和少量业务身份映射接进去，其余都由库装配。

## 2. 目标

v0.7 需要让接入方的 Web 层默认写法接近：

```rust
let auth = TenantAuthLayer::builder(iam_service)
    .jwt(jwt_config, claims_mapper)
    .build();

let app = Router::new()
    .route("/orders", get(list_orders).layer(auth.require("order:read")))
    .route("/profile", get(auth_profile))
    .layer(auth.context());
```

目标能力：

- 统一认证上下文装配。
- 支持 JWT、自定义 extractor、header/session 三类常见来源。
- 提供 `RequirePermissionLayer`。
- 提供 handler extractor：`CurrentSubject`、`CurrentPermissions`、`CurrentAccessScope`。
- 提供手动判定 helper，减少 handler 样板。
- 提供标准错误类型，允许应用自定义响应转换。
- 集成 v0.6 catalog，提供 profile 权限和权限目录接口。

## 3. 非目标

v0.7 不做以下事情：

- 不签发 JWT。
- 不处理登录、密码、短信验证码、OAuth 回调。
- 不决定应用的账号体系。
- 不定义用户、员工、代理商、门店等业务实体。
- 不直接生成路由页面。
- 不替代业务对象级授权中“先加载对象真实归属路径”的步骤。
- 不把所有业务接口都强行 layer 化；对象级授权仍推荐 handler 内手动判定。

## 4. 模块边界

建议新增或扩展 feature：

```toml
[features]
axum = ["dep:axum", "dep:http", "dep:tower"]
axum-jwt = ["axum", "dep:jsonwebtoken", "serde"]
axum-iam = ["axum", "iam", "catalog"]
```

模块结构：

```text
src/
  axum/
    context.rs
    error.rs
    layer.rs
    extractor.rs
    guard.rs
    profile.rs
    jwt.rs
```

保留现有低层 API：

- `AuthContext`
- `TenantAuthorizeLayer`
- `PlatformAuthorizeLayer`
- `jwt::JwtAuthLayer`

新增高层 API 不应破坏这些类型。

## 5. TenantAxumAuth

建议核心类型：

```rust
pub struct TenantAxumAuth<S, C> {
    service: Arc<TenantIamService<S, C>>,
    config: TenantAxumConfig,
}
```

Builder：

```rust
impl<S, C> TenantAxumAuth<S, C>
where
    S: TenantAuthStore,
    C: Cache,
{
    pub fn builder(service: TenantIamService<S, C>) -> TenantAxumAuthBuilder<S, C>;

    pub fn context_layer(&self) -> TenantAuthContextLayer<S, C>;

    pub fn require(
        &self,
        permission: impl AsRef<str>,
    ) -> Result<RequireTenantPermissionLayer<S, C>>;

    pub fn profile_routes(&self) -> Router;
}
```

配置：

```rust
pub struct TenantAxumConfig {
    pub expose_error_message: bool,
    pub fail_closed_on_source_error: bool,
}
```

默认值：

- `expose_error_message = false`
- `fail_closed_on_source_error = true`

## 6. 认证上下文装配

### 6.1 AuthContextResolver

提供可插拔 resolver：

```rust
#[async_trait]
pub trait AuthContextResolver: Send + Sync {
    async fn resolve<B>(&self, req: &Request<B>) -> Result<AuthContext, AuthExtractError>;
}
```

内置实现：

- `JwtContextResolver<C>`：从 JWT claims 映射。
- `HeaderContextResolver`：从 header 读取 tenant/principal。
- `ExtensionContextResolver`：复用已有扩展。

### 6.2 ClaimsMapper

JWT 场景下提供更灵活映射：

```rust
pub trait ClaimsMapper<C>: Send + Sync {
    fn map_claims(&self, claims: &C) -> Result<AuthContext, AuthExtractError>;
}
```

默认 mapper 仍支持：

```json
{
  "tenant_id": "tenant_a",
  "principal_id": "user_1"
}
```

业务项目如果有 `account_kind`、`employee_id`、`store_id`，可以自己实现 mapper。

## 7. Context Layer

`context_layer` 只负责认证上下文注入，不做权限判定：

```rust
Router::new()
    .route("/orders", get(list_orders))
    .layer(auth.context_layer());
```

注入内容：

- `AuthContext`
- `AuthSubject`
- `TenantIamService<S, C>` 的 `Arc`
- 可选 `SubjectPermissions` 缓存结果

规则：

- 缺少认证信息返回 401。
- token 非法返回 401。
- 上下文解析成功但权限不足由后续 guard 返回 403。
- 数据源错误返回 500，默认不暴露内部错误文本。

## 8. Require Permission Layer

租户级操作可以直接使用 route layer：

```rust
Router::new().route(
    "/tenant/settings",
    post(update_settings).layer(auth.require("tenant/settings:update")?),
)
```

内部调用：

```rust
service.can_tenant(TenantAccessRequest { subject, permission }).await
```

适用场景：

- 租户设置。
- 租户级报表。
- 创建某类资源。
- 不需要先加载业务对象 scope 的接口。

不适用：

- 更新订单。
- 删除设备。
- 查看某条销售单。

这些接口需要先加载对象真实归属路径，再调用 `can_access_scope`。

## 9. Extractor

### 9.1 CurrentSubject

```rust
pub struct CurrentSubject(pub AuthSubject);
```

Handler 使用：

```rust
async fn list_orders(
    CurrentSubject(subject): CurrentSubject,
    State(service): State<AppIamService>,
) -> Result<Json<Vec<Order>>, AuthHttpError> {
    let scope = service
        .accessible_scope(ScopeQuery {
            subject,
            permission: Permission::parse("order:read")?,
        })
        .await?;

    Ok(Json(repo.list_by_scope(scope).await?))
}
```

### 9.2 CurrentPermissions

```rust
pub struct CurrentPermissions(pub SubjectPermissions);
```

用于 profile、菜单、按钮控制。

### 9.3 RequiredScope

提供 helper 而不是复杂 extractor：

```rust
let scope = authz.accessible_scope("order:read").await?;
```

其中 `authz` 从 extension 中读取当前 subject 和 service。

## 10. Handler Helper

建议提供：

```rust
pub struct Authz<S, C> {
    subject: AuthSubject,
    service: Arc<TenantIamService<S, C>>,
}
```

方法：

```rust
impl<S, C> Authz<S, C> {
    pub fn subject(&self) -> &AuthSubject;

    pub async fn require_tenant(
        &self,
        permission: impl AsRef<str>,
    ) -> Result<(), AuthHttpError>;

    pub async fn accessible_scope(
        &self,
        permission: impl AsRef<str>,
    ) -> Result<AccessScope, AuthHttpError>;

    pub async fn require_scope(
        &self,
        permission: impl AsRef<str>,
        target: ScopePath,
    ) -> Result<(), AuthHttpError>;
}
```

对象级接口使用：

```rust
async fn update_order(
    Authz(authz): Authz<AppStore, AppCache>,
    Path(order_id): Path<OrderId>,
    Json(input): Json<UpdateOrderInput>,
) -> Result<Json<Order>, AuthHttpError> {
    let order = repo.load(order_id).await?;
    authz
        .require_scope("order:update", order.scope_path())
        .await?;

    let updated = repo.update(order.id, input).await?;
    Ok(Json(updated))
}
```

这样保留“先查真实对象归属”的安全边界，同时减少权限判定样板。

## 11. Profile 和 Catalog 路由

v0.7 可以提供可选 route：

```rust
let app = Router::new().nest("/auth", auth.profile_routes());
```

默认接口：

| 路由 | 方法 | 说明 |
|---|---|---|
| `/auth/profile` | `GET` | 当前 subject、权限列表、权限元数据 |
| `/auth/permissions` | `GET` | 权限目录 |

返回模型来自 v0.6：

- `SubjectPermissions`
- `PermissionCatalogView`

这些 route 不包含用户昵称、头像、手机号等业务 profile 字段。业务系统可以组合自己的用户 profile。

## 12. 错误模型

```rust
pub enum AuthHttpError {
    Unauthorized,
    Forbidden,
    BadRequest(String),
    Source,
    Internal,
}
```

默认映射：

| 错误 | HTTP |
|---|---|
| `Unauthorized` | 401 |
| `Forbidden` | 403 |
| `BadRequest` | 400 |
| `Source` | 500 |
| `Internal` | 500 |

允许应用自定义：

```rust
pub trait AuthErrorResponseMapper {
    fn map_error(&self, err: AuthHttpError) -> Response;
}
```

默认响应不泄漏数据库错误、token 细节或内部栈信息。

## 13. Platform Axum

v0.7 的重点先是租户内授权。

平台授权可以提供并行 builder：

```rust
pub struct PlatformAxumAuth<S> {
    engine: Arc<PlatformEngine<S>>,
}
```

但不应阻塞租户内高层接入落地。平台 builder 可作为 v0.7 的后半部分或 v0.8 候选。

## 14. 接入最终形态

理想接入代码：

```rust
let store = AppTenantAuthStore::new(pool);

let catalog = PermissionCatalog::new(app_permissions())?;

let iam = TenantIamService::builder(store)
    .permission_catalog(catalog)
    .engine_config(EngineConfig {
        enable_role_hierarchy: true,
        enable_wildcard: true,
        max_role_depth: 16,
    })
    .cache(MemoryCache::new(100_000))
    .build();

let auth = TenantAxumAuth::builder(iam)
    .jwt(jwt_state, AppClaimsMapper)
    .build();

let app = Router::new()
    .route("/orders", get(list_orders))
    .route(
        "/tenant/settings",
        post(update_settings).layer(auth.require("tenant/settings:update")?),
    )
    .nest("/auth", auth.profile_routes())
    .layer(auth.context_layer());
```

业务 handler 中：

```rust
async fn list_orders(Authz(authz): Authz<AppStore, AppCache>) -> Result<Json<Vec<Order>>, Error> {
    let scope = authz.accessible_scope("order:read").await?;
    Ok(Json(repo.list_by_scope(scope).await?))
}
```

接入项目保留的工作：

- 实现 `TenantAuthStore`。
- 实现 JWT claims 到 `AuthContext` 的映射。
- 对业务对象级接口加载真实归属路径。
- 把 `AccessScope` 下推到业务查询。

## 15. 测试清单

必须覆盖：

- context layer 缺少 token 返回 401。
- context layer token 非法返回 401。
- context layer 成功注入 `AuthContext` 和 `AuthSubject`。
- require layer 授权成功时进入 handler。
- require layer 权限不足返回 403。
- source/store 错误返回 500。
- `Authz::accessible_scope` 返回正确 scope。
- `Authz::require_scope` 拒绝越权路径。
- profile route 返回当前 subject 权限。
- permission catalog route 返回稳定结构。
- 自定义 claims mapper 能工作。
- 默认错误响应不泄漏内部错误。

## 16. 实施阶段

### 阶段一：高层类型和错误

- 定义 `TenantAxumAuth`。
- 定义 `AuthHttpError`。
- 定义 `AuthContextResolver`。
- 保留现有低层 API。

### 阶段二：Context Layer

- 实现 extension 注入。
- 实现 JWT resolver。
- 实现 header resolver。
- 补 401/500 测试。

### 阶段三：Require Layer 和 Authz

- 实现 `auth.require(permission)`。
- 实现 `Authz` extractor。
- 补租户级和对象级授权示例。

### 阶段四：Profile 和 Catalog Routes

- 集成 v0.6 `SubjectPermissions`。
- 提供 `/auth/profile` 和 `/auth/permissions`。
- 文档说明业务 profile 如何组合。

### 阶段五：生产示例

- 增加完整 Axum 示例。
- 示例中只留一个假的 `TenantAuthStore` 实现。
- README 更新为推荐三步接入。

## 17. 结论

v0.7 的目标是把 Web 接入体验收口：

```text
TenantAuthStore
        |
        +-- TenantIamService
        |
        +-- TenantAxumAuth
              |
              +-- context layer
              +-- require layer
              +-- Authz extractor
              +-- profile/catalog routes
```

完成后，`rs-tenant` 的推荐使用方式就能贴近最初目标：

1. 实现数据存储 trait。
2. 在 Axum 中使用中间件。
3. 在对象级接口里手动调用方法验证权限。

其余角色、权限、绑定、profile、权限目录和缓存失效的通用部分都由库承担。

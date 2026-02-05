# rs-tenant（多租户 RBAC 权限管理库，Rust）

状态：v0.1 设计稿
适用范围：通用多租户权限管理库，可被任意业务系统集成
核心定位：轻量、强类型、协议无关、存储外置、RBAC 为主、可扩展

## 项目简介

本项目提供一套面向多租户场景的 RBAC 权限管理设计。默认安全隔离、强类型 ID、权限解析与匹配可复用，存储与协议实现交由外部系统完成。

当前仓库以设计文档为主，实现尚在建设中。

## 设计目标

- 多租户隔离默认安全（Deny by default）
- 外部系统可自定义存储实现（DB/缓存/服务）
- RBAC 作为主模型，支持角色继承与通配权限
- 支持全局角色（跨租户的系统级角色）
- 提供内存实现，便于测试与演示
- 可选集成 Casbin 作为替代引擎

## 非目标

- 默认不引入复杂 ABAC
- 不强绑任何协议或 Web 框架
- 不强制使用 Casbin
- 不提供业务层数据模型与迁移工具

## 核心概念

- Tenant：租户
- Principal：主体（用户/服务账号/机器人）
- Role：角色
- Permission：权限（resource:action）
- GlobalRole：全局角色（跨租户复用）
- Decision：授权结果（Allow/Deny）

## 多租户数据隔离策略

行业默认方案：共享数据库 + tenant_id 列

- 每张业务表必须包含 tenant_id
- 所有读写必须带 tenant_id
- 联表时必须以 tenant_id 进行约束
- 唯一约束与外键必须包含 tenant_id

库侧保证

- 所有 Store trait 必须显式传入 TenantId
- authorize() 与 scope() 均必须传 TenantId
- scope() 必须至少包含 tenant_id = ? 过滤条件

## Crate 与模块规划

建议拆成多个 crate，便于复用与扩展：

- rbac-core：类型与 trait 定义，权限解析与匹配逻辑
- rbac-engine：RBAC 引擎实现，角色继承、通配权限处理，授权与 scope 计算
- rbac-store-memory：内存实现（测试/演示）
- rbac-cache：缓存实现与失效接口
- rbac-axum（后续扩展）：与具体框架集成
- rbac-casbin（可选）：Casbin 适配器

## 类型系统设计（强类型 + serde）

所有 ID 使用 String，通过 newtype 保证强类型。

```rust
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct TenantId(String);

pub struct PrincipalId(String);
pub struct RoleId(String);
pub struct GlobalRoleId(String);

pub struct ResourceName(String);
pub struct Permission(String);
```

建议实现的 trait

- Clone, Eq, Hash, Debug, Display
- TryFrom<&str> 与 From<String>
- AsRef<str>, Borrow<str>

ID 与资源命名约定

- 不能为空，去除首尾空白
- 建议长度 1..=128，超过可拒绝
- 建议仅允许 [a-zA-Z0-9:_-]
- 主体 ID 建议全局唯一，便于全局角色绑定

构造与校验约定

- new/try_from 负责校验并返回 Result
- from_string 允许直接包裹，用于已可信输入
- v0.1 默认启用校验，不做隐式容错

## 权限模型

权限字符串规范

- 格式：resource:action
- 支持通配：resource:*、*:*
- 建议小写规范化
- 不允许空段（例如 :read 或 invoice:）
- 默认仅允许 [a-z0-9:_-]（非法字符直接拒绝）
- 如需兼容旧系统，可通过自定义校验器放宽

规范化建议

- trim 空白并转小写
- 未通过校验时返回错误，不进入授权流程

权限字符白名单扩展策略

- v0.1 允许注入自定义校验器（例如放宽到 [a-zA-Z0-9:._-]）
- 默认实现仍保持严格模式，避免权限串不可控

说明：权限字符串不包含 tenant 信息

- 租户隔离由运行时上下文与存储层保证
- authorize() / scope() 必须传入 TenantId
- Store 查询与数据表层面强制 tenant_id 过滤
- scope() 至少返回 tenant_id 约束，避免跨租户访问

匹配规则

- 精确匹配优先
- 通配匹配次之
- 只要任一权限匹配则 Allow
- 默认 Deny
- v0.1 仅支持 Allow 集合，不提供显式 Deny

通配开关

- enable_wildcard = false 时，通配权限不参与匹配
- enable_wildcard = false 时不报错但会忽略通配权限

## 角色继承

- 支持角色继承 DAG
- 引擎默认做环检测
- 可配置最大深度防止异常图
- 检测到环时返回错误（避免隐性放权）

角色继承语义

- role_inherits(role) 返回该角色直接继承的“父角色”集合
- 角色权限向下继承，子角色包含父角色权限
- 继承展开以 role 为起点，沿父角色方向遍历

## 全局角色

- global_roles(principal) 返回全局角色集合
- 授权时自动合并全局角色权限
- 全局角色与租户角色共享同一 Permission 规则
- 全局角色与租户角色权限做去重合并

## Store Traits（外部实现）

按职责分层，便于你在 DB 层映射：

```rust
pub trait TenantStore {
    async fn tenant_active(&self, tenant: TenantId) -> Result<bool, StoreError>;
    async fn principal_active(&self, tenant: TenantId, principal: PrincipalId) -> Result<bool, StoreError>;
}

pub trait RoleStore {
    async fn principal_roles(&self, tenant: TenantId, principal: PrincipalId) -> Result<Vec<RoleId>, StoreError>;
    async fn role_permissions(&self, tenant: TenantId, role: RoleId) -> Result<Vec<Permission>, StoreError>;
    async fn role_inherits(&self, tenant: TenantId, role: RoleId) -> Result<Vec<RoleId>, StoreError>;
}

pub trait GlobalRoleStore {
    async fn global_roles(&self, principal: PrincipalId) -> Result<Vec<GlobalRoleId>, StoreError>;
    async fn global_role_permissions(&self, role: GlobalRoleId) -> Result<Vec<Permission>, StoreError>;
}

pub trait Store: TenantStore + RoleStore + GlobalRoleStore + Send + Sync {}
```

异步与错误处理约定

- v0.1 默认 async store 接口，适配 DB/缓存/服务调用
- 同步存储可用包装器（在 async 中调用阻塞 I/O 需外部自行处理）
- StoreError 由存储层定义，Engine 统一包装

建议错误类型

```rust
pub type StoreError = Box<dyn std::error::Error + Send + Sync>;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Store(StoreError),
    InvalidId(String),
    InvalidPermission(String),
    RoleCycleDetected { tenant: TenantId, role: RoleId },
    RoleDepthExceeded { tenant: TenantId, role: RoleId, max_depth: usize },
}
```

## 引擎 API（建议）

```rust
pub enum Decision {
    Allow,
    Deny,
}

pub struct Engine<S: Store> { /* ... */ }

impl<S: Store> Engine<S> {
    pub async fn authorize(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
        permission: Permission,
    ) -> Result<Decision> { /* ... */ }

    pub async fn scope(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
        resource: ResourceName,
    ) -> Result<Scope> { /* ... */ }
}

pub struct EngineBuilder<S: Store> { /* ... */ }

impl<S: Store> EngineBuilder<S> {
    pub fn new(store: S) -> Self;
    pub fn enable_role_hierarchy(self, on: bool) -> Self;
    pub fn enable_wildcard(self, on: bool) -> Self;
    pub fn max_inherit_depth(self, depth: usize) -> Self;
    pub fn cache<C: Cache + 'static>(self, cache: C) -> Self;
    pub fn build(self) -> Engine<S>;
}
```

配置与默认值

- enable_role_hierarchy = false
- enable_wildcard = false
- max_inherit_depth = 16
- cache = None
- permission_normalize = true

## 授权流程

1. 校验 tenant_active
2. 校验 principal_active
3. 查询租户角色
4. 展开角色继承
5. 合并全局角色
6. 匹配权限（含通配）
7. 返回 Allow / Deny

授权语义

- tenant 或 principal 未激活时直接 Deny
- 存储或计算错误返回 Err
- 权限集合为空时返回 Deny

权限计算细节

- 角色继承开启时，DFS/BFS 展开并做环检测
- 角色继承关闭时，仅使用直接角色
- 多处来源的权限合并后去重
- 匹配采用 HashSet 优化

## Scope API（资源过滤）

RBAC 的 scope() 输出必须包含 tenant_id 条件。

建议返回 AST，外部可转 SQL/ORM：

```rust
pub enum Scope {
    None,
    TenantOnly { tenant: TenantId },
}
```

scope() 语义

- scope(resource) 判断是否具备该资源的任意权限
- action 维度被忽略，只要 resource 匹配即允许
- 若无权限，返回 Scope::None
- 若有权限，返回 Scope::TenantOnly
- tenant 或 principal 未激活时返回 Scope::None

未来需要资源级授权时，可扩展：

- Scope::IdList(Vec<ResourceId>)
- Scope::Predicate(String) 或 DSL AST

## 缓存与失效

建议引擎内置缓存接口：

```rust
pub trait Cache {
    async fn get_permissions(&self, tenant: TenantId, principal: PrincipalId) -> Option<Vec<Permission>>;
    async fn set_permissions(&self, tenant: TenantId, principal: PrincipalId, perms: Vec<Permission>);
    async fn invalidate_principal(&self, tenant: TenantId, principal: PrincipalId);
    async fn invalidate_role(&self, tenant: TenantId, role: RoleId);
    async fn invalidate_tenant(&self, tenant: TenantId);
}
```

缓存一致性建议

- 角色、权限、继承、全局角色更新时触发失效
- 失效粒度优先使用 principal 粒度，必要时向上扩散
- tenant_active 与 principal_active 建议每次实时检查

## 数据模型建议（关系型数据库）

- tenants(id, active, created_at)
- principals(tenant_id, id, active)
- roles(tenant_id, id)
- principal_roles(tenant_id, principal_id, role_id)
- role_permissions(tenant_id, role_id, permission)
- role_inherits(tenant_id, role_id, inherited_role_id)
- global_roles(id)
- principal_global_roles(principal_id, global_role_id)
- global_role_permissions(global_role_id, permission)

索引建议

- tenants(id) 唯一索引
- principals(tenant_id, id) 唯一索引
- roles(tenant_id, id) 唯一索引
- principal_roles(tenant_id, principal_id) 组合索引
- role_permissions(tenant_id, role_id) 组合索引
- role_inherits(tenant_id, role_id) 组合索引
- principal_global_roles(principal_id) 组合索引

## 可选 Casbin 适配

策略

- 默认不依赖 Casbin
- feature = "casbin" 时提供 CasbinAuthorizer
- tenant_id 映射到 Casbin Domain
- 不强制 Casbin policy 格式

## 线程安全与性能

- Engine、Store、Cache 要求 Send + Sync
- 角色展开与权限合并应做去重，避免重复匹配
- 建议缓存 (tenant, principal) 的有效权限集合
- 角色继承与通配开启会增加授权开销

## 版本与兼容

- 遵循语义化版本
- v0.x 期间允许不兼容变更
- v1.0 后保证公开 API 稳定

## 使用示例（伪代码）

```rust
use rbac_engine::{Engine, EngineBuilder, Permission};
use rbac_core::{TenantId, PrincipalId, ResourceName};

struct PgStore { /* ... */ }
impl Store for PgStore { /* ... */ }

let engine = EngineBuilder::new(PgStore::new())
    .enable_role_hierarchy(true)
    .enable_wildcard(true)
    .cache(LruCache::new(10_000))
    .build();

let tenant = TenantId::try_from("t_123")?;
let user = PrincipalId::try_from("u_42")?;

engine.authorize(tenant, user, Permission::try_from("invoice:read")?)?;
let scope = engine.scope(tenant, user, ResourceName::try_from("invoice")?)?;
```

## Feature Flags

- serde：为 ID 与 Permission 提供序列化支持
- casbin：启用 Casbin 适配引擎
- memory-store：提供内存实现
- memory-cache：提供内存缓存实现
- axum：提供 Axum 集成（中间件）
- axum-jwt：提供 JWT 解析与 AuthContext extractor（依赖 axum + jsonwebtoken + serde）

## 设计确认点

最终设计的关键确认点：

1. 默认 RBAC + 角色继承 + 通配权限
2. 默认支持全局角色
3. String ID + serde
4. scope() 至少强制 tenant_id
5. 外部存储完全由 trait 实现（默认 async）

补充确认：

- 全局角色仅参与授权判断
- 租户停用时，全局角色失效
- ID new() 非空校验，提供 TryFrom<&str>
- v0.1 scope 仅支持 None / TenantOnly

## 实现路线（建议）

1. rbac-core：ID 与 Permission 类型、校验、匹配函数、错误类型
2. rbac-engine：授权流程、角色继承展开、scope 计算
3. rbac-store-memory：内存 Store 与基础示例
4. 单元测试与集成测试

## 测试计划（建议）

- Permission 解析与规范化
- 通配匹配与精确匹配
- 角色继承环检测与深度限制
- 全局角色合并与去重
- tenant/principal 停用的 Deny 行为
- scope() 输出符合 tenant 限制
- 缓存失效逻辑正确性

## 开发与测试

- 运行测试：`cargo test`

## 文档

- DDD + Axum 接入指南：`docs/axum_ddd_integration.md`

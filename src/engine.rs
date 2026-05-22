use crate::cache::{Cache, EffectiveGrant, NoCache};
use crate::decision::{AccessDecision, AccessExplanation, DenyReason};
use crate::error::{Error, Result};
use crate::ids::{PrincipalId, RoleId, TenantId};
use crate::request::{AuthSubject, ScopeQuery, ScopedAccessRequest, TenantAccessRequest};
use crate::role_hierarchy::{RoleHierarchy, expand_roles};
use crate::scope::AccessScope;
use crate::source::{AuthorizationSource, MembershipStatus, TenantStatus};
use async_trait::async_trait;

/// 引擎行为配置。
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EngineConfig {
    /// 是否通过 [`AuthorizationSource::parent_roles`] 启用角色继承遍历。
    pub enable_role_hierarchy: bool,
    /// 是否启用完整资源/动作通配符匹配。
    pub enable_wildcard: bool,
    /// 最大角色继承深度。
    pub max_role_depth: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            enable_role_hierarchy: false,
            enable_wildcard: false,
            max_role_depth: 16,
        }
    }
}

impl EngineConfig {
    /// 生成用于区分缓存条目的配置签名。
    fn signature(&self) -> String {
        format!(
            "rh:{};wc:{};depth:{}",
            u8::from(self.enable_role_hierarchy),
            u8::from(self.enable_wildcard),
            self.max_role_depth
        )
    }
}

/// 租户 RBAC 授权引擎。
#[derive(Debug)]
pub struct Engine<S, C = NoCache> {
    source: S,
    cache: C,
    config: EngineConfig,
    config_signature: String,
}

/// [`Engine`] 构造器。
pub struct EngineBuilder<S, C = NoCache> {
    source: S,
    cache: C,
    config: EngineConfig,
}

impl<S> EngineBuilder<S, NoCache> {
    /// 使用默认配置和空缓存创建构造器。
    pub fn new(source: S) -> Self {
        Self {
            source,
            cache: NoCache,
            config: EngineConfig::default(),
        }
    }
}

impl<S, C> EngineBuilder<S, C> {
    /// 替换完整引擎配置。
    pub fn config(mut self, config: EngineConfig) -> Self {
        self.config = config;
        self
    }

    /// 启用或禁用角色继承。
    pub fn enable_role_hierarchy(mut self, on: bool) -> Self {
        self.config.enable_role_hierarchy = on;
        self
    }

    /// 启用或禁用通配符匹配。
    pub fn enable_wildcard(mut self, on: bool) -> Self {
        self.config.enable_wildcard = on;
        self
    }

    /// 设置最大角色继承深度。
    pub fn max_role_depth(mut self, depth: usize) -> Self {
        self.config.max_role_depth = depth;
        self
    }

    /// 设置缓存实现。
    pub fn cache<C2: Cache>(self, cache: C2) -> EngineBuilder<S, C2> {
        EngineBuilder {
            source: self.source,
            cache,
            config: self.config,
        }
    }

    /// 构建引擎。
    pub fn build(self) -> Engine<S, C> {
        let config_signature = self.config.signature();
        Engine {
            source: self.source,
            cache: self.cache,
            config: self.config,
            config_signature,
        }
    }
}

impl<S, C> Engine<S, C>
where
    S: AuthorizationSource,
    C: Cache,
{
    /// 返回当前引擎配置。
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// 计算某个权限可访问的数据范围。
    pub async fn accessible_scope(&self, query: ScopeQuery) -> Result<AccessScope> {
        let (scope, _) = self.resolve_scope(query).await?;
        Ok(scope)
    }

    /// 检查主体是否可以访问目标范围路径。
    pub async fn can_access_scope(&self, request: ScopedAccessRequest) -> Result<AccessDecision> {
        Ok(self.explain_access_scope(request).await?.decision)
    }

    /// 检查主体是否拥有租户级访问权。
    pub async fn can_tenant(&self, request: TenantAccessRequest) -> Result<AccessDecision> {
        Ok(self.explain_tenant(request).await?.decision)
    }

    /// 解释目标路径访问检查结果。
    pub async fn explain_access_scope(
        &self,
        request: ScopedAccessRequest,
    ) -> Result<AccessExplanation> {
        let query = ScopeQuery {
            subject: request.subject,
            permission: request.permission,
        };
        let (scope, reason) = self.resolve_scope(query).await?;
        let (decision, reason) = match &scope {
            AccessScope::None => (
                AccessDecision::Deny,
                reason.or(Some(DenyReason::PermissionMissing)),
            ),
            AccessScope::Tenant { .. } => (AccessDecision::Allow, None),
            AccessScope::Paths { .. } if scope.allows_path(&request.target) => {
                (AccessDecision::Allow, None)
            }
            AccessScope::Paths { .. } => (AccessDecision::Deny, Some(DenyReason::ScopeDenied)),
        };
        Ok(AccessExplanation {
            decision,
            reason,
            scope,
        })
    }

    /// 解释租户级访问检查结果。
    pub async fn explain_tenant(&self, request: TenantAccessRequest) -> Result<AccessExplanation> {
        let query = ScopeQuery {
            subject: request.subject,
            permission: request.permission,
        };
        let (scope, reason) = self.resolve_scope(query).await?;
        let (decision, reason) = match &scope {
            AccessScope::Tenant { .. } => (AccessDecision::Allow, None),
            AccessScope::Paths { .. } => {
                (AccessDecision::Deny, Some(DenyReason::TargetScopeRequired))
            }
            AccessScope::None => (
                AccessDecision::Deny,
                reason.or(Some(DenyReason::PermissionMissing)),
            ),
        };
        Ok(AccessExplanation {
            decision,
            reason,
            scope,
        })
    }

    /// 失效某个主体的缓存授权。
    pub async fn invalidate_principal(&self, tenant: &TenantId, principal: &PrincipalId) {
        self.cache.invalidate_principal(tenant, principal).await;
    }

    /// 失效某个角色的缓存授权。
    pub async fn invalidate_role(&self, tenant: &TenantId, role: &RoleId) {
        self.cache.invalidate_role(tenant, role).await;
    }

    /// 失效某个租户的缓存授权。
    pub async fn invalidate_tenant(&self, tenant: &TenantId) {
        self.cache.invalidate_tenant(tenant).await;
    }

    /// 失效所有缓存授权。
    pub async fn invalidate_all(&self) {
        self.cache.invalidate_all().await;
    }

    /// 解析权限查询对应的最终访问范围和拒绝原因。
    async fn resolve_scope(&self, query: ScopeQuery) -> Result<(AccessScope, Option<DenyReason>)> {
        let tenant = query.subject.tenant.clone();
        if self.source.tenant_status(&tenant).await? != TenantStatus::Active {
            return Ok((AccessScope::None, Some(DenyReason::TenantInactive)));
        }
        if self.source.membership_status(&query.subject).await? != MembershipStatus::Active {
            return Ok((AccessScope::None, Some(DenyReason::PrincipalInactive)));
        }

        let grants = self.effective_grants(&query.subject).await?;
        let matched_scopes = grants
            .into_iter()
            .filter(|grant| {
                grant.matches_permission(&query.permission, self.config.enable_wildcard)
            })
            .map(|grant| grant.scope);
        let scope = AccessScope::merge(tenant, matched_scopes);
        let reason = match scope {
            AccessScope::None => Some(DenyReason::PermissionMissing),
            _ => None,
        };
        Ok((scope, reason))
    }

    /// 读取或计算主体在当前引擎配置下的有效授权。
    async fn effective_grants(&self, subject: &AuthSubject) -> Result<Vec<EffectiveGrant>> {
        if let Some(grants) = self
            .cache
            .get_effective_grants(&subject.tenant, &subject.principal, &self.config_signature)
            .await
        {
            return Ok(grants);
        }

        let assignments = self.source.role_assignments(subject).await?;
        let mut grants = Vec::new();
        for assignment in assignments {
            let roles = if self.config.enable_role_hierarchy {
                let hierarchy = TenantRoleHierarchy {
                    engine: self,
                    tenant: &subject.tenant,
                };
                expand_roles(&hierarchy, assignment.role.clone()).await?
            } else {
                vec![assignment.role]
            };

            for role in roles {
                let permissions = self.source.role_permissions(&subject.tenant, &role).await?;
                grants.extend(permissions.into_iter().map(|permission| {
                    EffectiveGrant::new(role.clone(), permission, assignment.scope.clone())
                }));
            }
        }

        self.cache
            .set_effective_grants(
                &subject.tenant,
                &subject.principal,
                &self.config_signature,
                grants.clone(),
            )
            .await;
        Ok(grants)
    }
}

struct TenantRoleHierarchy<'a, S, C> {
    engine: &'a Engine<S, C>,
    tenant: &'a TenantId,
}

#[async_trait]
impl<S, C> RoleHierarchy for TenantRoleHierarchy<'_, S, C>
where
    S: AuthorizationSource,
    C: Cache,
{
    type Role = RoleId;

    async fn parent_roles(&self, role: &Self::Role) -> Result<Vec<Self::Role>> {
        self.engine
            .source
            .parent_roles(self.tenant, role)
            .await
            .map_err(Error::from)
    }

    fn max_depth(&self) -> usize {
        self.engine.config.max_role_depth
    }

    fn cycle_error(&self, role: Self::Role) -> Error {
        Error::RoleCycleDetected {
            tenant: self.tenant.clone(),
            role,
        }
    }

    fn depth_error(&self, role: Self::Role) -> Error {
        Error::RoleDepthExceeded {
            tenant: self.tenant.clone(),
            role,
            max_depth: self.engine.config.max_role_depth,
        }
    }
}

#[cfg(all(test, feature = "memory-store"))]
mod tests {
    use super::*;
    use crate::memory_source::MemorySource;
    use crate::{GrantScope, MembershipStatus, Permission, ScopePath, TenantStatus};
    use futures::executor::block_on;

    /// 构造一组通用测试标识符。
    fn ids() -> (TenantId, PrincipalId, RoleId) {
        (
            TenantId::parse("tenant_1").expect("tenant"),
            PrincipalId::parse("user_1").expect("principal"),
            RoleId::parse("reader").expect("role"),
        )
    }

    /// 构造已激活租户、成员关系和角色授权的测试数据源。
    fn active_source(scope: GrantScope, permission: &str) -> (MemorySource, AuthSubject) {
        let (tenant, principal, role) = ids();
        let source = MemorySource::new();
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
        source.add_role_assignment(tenant.clone(), principal.clone(), role.clone(), scope);
        source.add_role_permission(
            tenant.clone(),
            role,
            Permission::parse(permission).expect("permission"),
        );
        (source, AuthSubject::new(tenant, principal))
    }

    #[test]
    fn accessible_scope_should_return_tenant_for_tenant_grant() {
        let (source, subject) = active_source(GrantScope::tenant(), "invoice:read");
        let engine = EngineBuilder::new(source).build();
        let scope = block_on(engine.accessible_scope(ScopeQuery {
            subject,
            permission: Permission::parse("invoice:read").expect("permission"),
        }))
        .expect("scope");

        assert!(matches!(scope, AccessScope::Tenant { .. }));
    }

    #[test]
    fn can_tenant_should_deny_path_only_grant() {
        let root = ScopePath::parse("agent/1").expect("scope path");
        let (source, subject) = active_source(
            GrantScope::paths(vec![root]).expect("grant scope"),
            "invoice:read",
        );
        let engine = EngineBuilder::new(source).build();
        let explanation = block_on(engine.explain_tenant(TenantAccessRequest {
            subject,
            permission: Permission::parse("invoice:read").expect("permission"),
        }))
        .expect("explanation");

        assert_eq!(explanation.decision, AccessDecision::Deny);
        assert_eq!(explanation.reason, Some(DenyReason::TargetScopeRequired));
    }

    #[test]
    fn can_access_scope_should_allow_descendant_path() {
        let root = ScopePath::parse("agent/1").expect("scope path");
        let target = ScopePath::parse("agent/1/store/2").expect("scope path");
        let (source, subject) = active_source(
            GrantScope::paths(vec![root]).expect("grant scope"),
            "invoice:read",
        );
        let engine = EngineBuilder::new(source).build();
        let decision = block_on(engine.can_access_scope(ScopedAccessRequest {
            subject,
            permission: Permission::parse("invoice:read").expect("permission"),
            target,
        }))
        .expect("decision");

        assert_eq!(decision, AccessDecision::Allow);
    }

    #[test]
    fn wildcard_should_require_config_flag() {
        let (source, subject) = active_source(GrantScope::tenant(), "invoice:*");
        let strict_engine = EngineBuilder::new(source.clone()).build();
        let wildcard_engine = EngineBuilder::new(source).enable_wildcard(true).build();
        let request = TenantAccessRequest {
            subject,
            permission: Permission::parse("invoice:read").expect("permission"),
        };

        let strict = block_on(strict_engine.can_tenant(request.clone())).expect("decision");
        let wildcard = block_on(wildcard_engine.can_tenant(request)).expect("decision");

        assert_eq!(strict, AccessDecision::Deny);
        assert_eq!(wildcard, AccessDecision::Allow);
    }

    #[test]
    fn role_hierarchy_should_use_assignment_scope() {
        let (tenant, principal, child) = ids();
        let parent = RoleId::parse("parent").expect("role");
        let root = ScopePath::parse("agent/1").expect("scope path");
        let source = MemorySource::new();
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            child.clone(),
            GrantScope::paths(vec![root.clone()]).expect("grant scope"),
        );
        source.add_parent_role(tenant.clone(), child, parent.clone());
        source.add_role_permission(
            tenant.clone(),
            parent,
            Permission::parse("invoice:read").expect("permission"),
        );

        let engine = EngineBuilder::new(source)
            .enable_role_hierarchy(true)
            .build();
        let scope = block_on(engine.accessible_scope(ScopeQuery {
            subject: AuthSubject::new(tenant, principal),
            permission: Permission::parse("invoice:read").expect("permission"),
        }))
        .expect("scope");

        assert_eq!(
            scope,
            AccessScope::Paths {
                tenant: TenantId::parse("tenant_1").expect("tenant"),
                roots: vec![root],
            }
        );
    }

    #[test]
    fn inactive_tenant_should_return_none_with_reason() {
        let (tenant, principal, _) = ids();
        let source = MemorySource::new();
        source.set_tenant_status(tenant.clone(), TenantStatus::Inactive);
        let engine = EngineBuilder::new(source).build();
        let explanation = block_on(engine.explain_tenant(TenantAccessRequest {
            subject: AuthSubject::new(tenant, principal),
            permission: Permission::parse("invoice:read").expect("permission"),
        }))
        .expect("explanation");

        assert_eq!(explanation.scope, AccessScope::None);
        assert_eq!(explanation.reason, Some(DenyReason::TenantInactive));
    }

    #[test]
    fn inactive_membership_should_return_none_with_reason() {
        let (tenant, principal, role) = ids();
        let source = MemorySource::new();
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        source.set_membership_status(
            tenant.clone(),
            principal.clone(),
            MembershipStatus::Inactive,
        );
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            role.clone(),
            GrantScope::tenant(),
        );
        source.add_role_permission(
            tenant.clone(),
            role,
            Permission::parse("invoice:read").expect("permission"),
        );
        let engine = EngineBuilder::new(source).build();

        let explanation = block_on(engine.explain_tenant(TenantAccessRequest {
            subject: AuthSubject::new(tenant, principal),
            permission: Permission::parse("invoice:read").expect("permission"),
        }))
        .expect("explanation");

        assert_eq!(explanation.scope, AccessScope::None);
        assert_eq!(explanation.reason, Some(DenyReason::PrincipalInactive));
    }

    #[test]
    fn no_role_assignment_should_return_none() {
        let (tenant, principal, _) = ids();
        let source = MemorySource::new();
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
        let engine = EngineBuilder::new(source).build();

        let explanation = block_on(engine.explain_tenant(TenantAccessRequest {
            subject: AuthSubject::new(tenant, principal),
            permission: Permission::parse("invoice:read").expect("permission"),
        }))
        .expect("explanation");

        assert_eq!(explanation.scope, AccessScope::None);
        assert_eq!(explanation.reason, Some(DenyReason::PermissionMissing));
    }

    #[test]
    fn permission_missing_should_deny() {
        let (source, subject) = active_source(GrantScope::tenant(), "invoice:read");
        let engine = EngineBuilder::new(source).build();

        let explanation = block_on(engine.explain_tenant(TenantAccessRequest {
            subject,
            permission: Permission::parse("invoice:update").expect("permission"),
        }))
        .expect("explanation");

        assert_eq!(explanation.decision, AccessDecision::Deny);
        assert_eq!(explanation.reason, Some(DenyReason::PermissionMissing));
    }

    #[test]
    fn can_access_scope_should_deny_outside_roots() {
        let root = ScopePath::parse("agent/1").expect("scope path");
        let target = ScopePath::parse("agent/2/store/9").expect("scope path");
        let (source, subject) = active_source(
            GrantScope::paths(vec![root]).expect("grant scope"),
            "invoice:read",
        );
        let engine = EngineBuilder::new(source).build();

        let explanation = block_on(engine.explain_access_scope(ScopedAccessRequest {
            subject,
            permission: Permission::parse("invoice:read").expect("permission"),
            target,
        }))
        .expect("explanation");

        assert_eq!(explanation.decision, AccessDecision::Deny);
        assert_eq!(explanation.reason, Some(DenyReason::ScopeDenied));
    }

    #[test]
    fn can_tenant_should_allow_tenant_grant() {
        let (source, subject) = active_source(GrantScope::tenant(), "invoice:read");
        let engine = EngineBuilder::new(source).build();

        let decision = block_on(engine.can_tenant(TenantAccessRequest {
            subject,
            permission: Permission::parse("invoice:read").expect("permission"),
        }))
        .expect("decision");

        assert_eq!(decision, AccessDecision::Allow);
    }

    #[test]
    fn multiple_path_assignments_should_merge_roots() {
        let (tenant, principal, first_role) = ids();
        let second_role = RoleId::parse("reader_2").expect("role");
        let source = MemorySource::new();
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            first_role.clone(),
            GrantScope::paths(vec![
                ScopePath::parse("agent/1/store/1").expect("scope path"),
                ScopePath::parse("agent/2").expect("scope path"),
            ])
            .expect("grant scope"),
        );
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            second_role.clone(),
            GrantScope::paths(vec![ScopePath::parse("agent/1").expect("scope path")])
                .expect("grant scope"),
        );
        let permission = Permission::parse("invoice:read").expect("permission");
        source.add_role_permission(tenant.clone(), first_role, permission.clone());
        source.add_role_permission(tenant.clone(), second_role, permission.clone());
        let engine = EngineBuilder::new(source).build();

        let scope = block_on(engine.accessible_scope(ScopeQuery {
            subject: AuthSubject::new(tenant.clone(), principal),
            permission,
        }))
        .expect("scope");

        assert_eq!(
            scope,
            AccessScope::Paths {
                tenant,
                roots: vec![
                    ScopePath::parse("agent/1").expect("scope path"),
                    ScopePath::parse("agent/2").expect("scope path"),
                ],
            }
        );
    }

    #[test]
    fn role_cycle_should_return_error() {
        let (tenant, principal, child) = ids();
        let parent = RoleId::parse("parent").expect("role");
        let source = MemorySource::new();
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            child.clone(),
            GrantScope::tenant(),
        );
        source.add_parent_role(tenant.clone(), child.clone(), parent.clone());
        source.add_parent_role(tenant.clone(), parent, child.clone());
        let engine = EngineBuilder::new(source)
            .enable_role_hierarchy(true)
            .build();

        let err = block_on(engine.accessible_scope(ScopeQuery {
            subject: AuthSubject::new(tenant.clone(), principal),
            permission: Permission::parse("invoice:read").expect("permission"),
        }))
        .expect_err("must detect cycle");

        assert!(matches!(
            err,
            Error::RoleCycleDetected { tenant: ref err_tenant, role }
                if err_tenant == &tenant && role == child
        ));
    }

    #[test]
    fn role_depth_exceeded_should_return_error() {
        let (tenant, principal, child) = ids();
        let parent = RoleId::parse("parent").expect("role");
        let grandparent = RoleId::parse("grandparent").expect("role");
        let source = MemorySource::new();
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            child,
            GrantScope::tenant(),
        );
        source.add_parent_role(
            tenant.clone(),
            RoleId::parse("reader").expect("role"),
            parent.clone(),
        );
        source.add_parent_role(tenant.clone(), parent, grandparent.clone());
        let engine = EngineBuilder::new(source)
            .enable_role_hierarchy(true)
            .max_role_depth(1)
            .build();

        let err = block_on(engine.accessible_scope(ScopeQuery {
            subject: AuthSubject::new(tenant.clone(), principal),
            permission: Permission::parse("invoice:read").expect("permission"),
        }))
        .expect_err("must enforce max depth");

        assert!(matches!(
            err,
            Error::RoleDepthExceeded { tenant: ref err_tenant, role, max_depth: 1 }
                if err_tenant == &tenant && role == grandparent
        ));
    }
}

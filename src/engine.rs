use crate::cache::{Cache, EffectiveGrant, NoCache};
use crate::decision::{AccessDecision, AccessExplanation, DenyReason};
use crate::error::{Error, Result};
use crate::ids::{PrincipalId, RoleId, TenantId};
use crate::request::{AuthSubject, ScopeQuery, ScopedAccessRequest, TenantAccessRequest};
use crate::scope::AccessScope;
use crate::source::{AuthorizationSource, MembershipStatus, TenantStatus};
use std::collections::HashSet;

/// Engine behavior configuration.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EngineConfig {
    /// Enables role inheritance traversal through [`AuthorizationSource::parent_roles`].
    pub enable_role_hierarchy: bool,
    /// Enables complete resource/action wildcard matching.
    pub enable_wildcard: bool,
    /// Maximum role inheritance depth.
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
    fn signature(&self) -> String {
        format!(
            "rh:{};wc:{};depth:{}",
            u8::from(self.enable_role_hierarchy),
            u8::from(self.enable_wildcard),
            self.max_role_depth
        )
    }
}

/// Tenant RBAC authorization engine.
#[derive(Debug)]
pub struct Engine<S, C = NoCache> {
    source: S,
    cache: C,
    config: EngineConfig,
    config_signature: String,
}

/// Builder for [`Engine`].
pub struct EngineBuilder<S, C = NoCache> {
    source: S,
    cache: C,
    config: EngineConfig,
}

impl<S> EngineBuilder<S, NoCache> {
    /// Creates a builder using default configuration and no cache.
    pub fn new(source: S) -> Self {
        Self {
            source,
            cache: NoCache,
            config: EngineConfig::default(),
        }
    }
}

impl<S, C> EngineBuilder<S, C> {
    /// Replaces the full engine configuration.
    pub fn config(mut self, config: EngineConfig) -> Self {
        self.config = config;
        self
    }

    /// Enables or disables role inheritance.
    pub fn enable_role_hierarchy(mut self, on: bool) -> Self {
        self.config.enable_role_hierarchy = on;
        self
    }

    /// Enables or disables wildcard matching.
    pub fn enable_wildcard(mut self, on: bool) -> Self {
        self.config.enable_wildcard = on;
        self
    }

    /// Sets maximum role inheritance depth.
    pub fn max_role_depth(mut self, depth: usize) -> Self {
        self.config.max_role_depth = depth;
        self
    }

    /// Sets the cache implementation.
    pub fn cache<C2: Cache>(self, cache: C2) -> EngineBuilder<S, C2> {
        EngineBuilder {
            source: self.source,
            cache,
            config: self.config,
        }
    }

    /// Builds the engine.
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
    /// Returns the current engine configuration.
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Computes the accessible data scope for a permission.
    pub async fn accessible_scope(&self, query: ScopeQuery) -> Result<AccessScope> {
        let (scope, _) = self.scope_with_reason(query).await?;
        Ok(scope)
    }

    /// Checks whether a subject can access a target scope path.
    pub async fn can_access_scope(&self, request: ScopedAccessRequest) -> Result<AccessDecision> {
        Ok(self.explain_access_scope(request).await?.decision)
    }

    /// Checks whether a subject has tenant-wide access.
    pub async fn can_tenant(&self, request: TenantAccessRequest) -> Result<AccessDecision> {
        Ok(self.explain_tenant(request).await?.decision)
    }

    /// Explains a target path access check.
    pub async fn explain_access_scope(
        &self,
        request: ScopedAccessRequest,
    ) -> Result<AccessExplanation> {
        let query = ScopeQuery {
            subject: request.subject,
            permission: request.permission,
        };
        let (scope, reason) = self.scope_with_reason(query).await?;
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

    /// Explains a tenant-level access check.
    pub async fn explain_tenant(&self, request: TenantAccessRequest) -> Result<AccessExplanation> {
        let query = ScopeQuery {
            subject: request.subject,
            permission: request.permission,
        };
        let (scope, reason) = self.scope_with_reason(query).await?;
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

    /// Invalidates grants cached for a principal.
    pub async fn invalidate_principal(&self, tenant: &TenantId, principal: &PrincipalId) {
        self.cache.invalidate_principal(tenant, principal).await;
    }

    /// Invalidates grants cached for a role.
    pub async fn invalidate_role(&self, tenant: &TenantId, role: &RoleId) {
        self.cache.invalidate_role(tenant, role).await;
    }

    /// Invalidates grants cached for a tenant.
    pub async fn invalidate_tenant(&self, tenant: &TenantId) {
        self.cache.invalidate_tenant(tenant).await;
    }

    /// Invalidates all cached grants.
    pub async fn invalidate_all(&self) {
        self.cache.invalidate_all().await;
    }

    async fn scope_with_reason(
        &self,
        query: ScopeQuery,
    ) -> Result<(AccessScope, Option<DenyReason>)> {
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
                grant
                    .permission
                    .matches(&query.permission, self.config.enable_wildcard)
            })
            .map(|grant| grant.scope);
        let scope = AccessScope::merge(tenant, matched_scopes);
        let reason = match scope {
            AccessScope::None => Some(DenyReason::PermissionMissing),
            _ => None,
        };
        Ok((scope, reason))
    }

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
                self.expand_roles(&subject.tenant, assignment.role.clone())
                    .await?
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

    async fn expand_roles(&self, tenant: &TenantId, root: RoleId) -> Result<Vec<RoleId>> {
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();
        let mut output = Vec::new();
        self.expand_from_role(tenant, root, &mut visited, &mut visiting, &mut output)
            .await?;
        Ok(output)
    }

    async fn expand_from_role(
        &self,
        tenant: &TenantId,
        root: RoleId,
        visited: &mut HashSet<RoleId>,
        visiting: &mut HashSet<RoleId>,
        output: &mut Vec<RoleId>,
    ) -> Result<()> {
        visiting.insert(root.clone());
        output.push(root.clone());
        let parents = self.source.parent_roles(tenant, &root).await?;
        let mut stack: Vec<(RoleId, usize, std::vec::IntoIter<RoleId>)> =
            vec![(root, 0, parents.into_iter())];

        while let Some((current, depth, mut iter)) = stack.pop() {
            if let Some(parent) = iter.next() {
                stack.push((current.clone(), depth, iter));
                let next_depth = depth + 1;
                if next_depth > self.config.max_role_depth {
                    return Err(Error::RoleDepthExceeded {
                        tenant: tenant.clone(),
                        role: parent,
                        max_depth: self.config.max_role_depth,
                    });
                }
                if visiting.contains(&parent) {
                    return Err(Error::RoleCycleDetected {
                        tenant: tenant.clone(),
                        role: parent,
                    });
                }
                if visited.contains(&parent) {
                    continue;
                }

                let parents = self.source.parent_roles(tenant, &parent).await?;
                visiting.insert(parent.clone());
                output.push(parent.clone());
                stack.push((parent, next_depth, parents.into_iter()));
                continue;
            }

            visiting.remove(&current);
            visited.insert(current);
        }

        Ok(())
    }
}

#[cfg(all(test, feature = "memory-store"))]
mod tests {
    use super::*;
    use crate::memory_source::MemorySource;
    use crate::{GrantScope, MembershipStatus, Permission, ScopePath, TenantStatus};
    use futures::executor::block_on;

    fn ids() -> (TenantId, PrincipalId, RoleId) {
        (
            TenantId::parse("tenant_1").expect("tenant"),
            PrincipalId::parse("user_1").expect("principal"),
            RoleId::parse("reader").expect("role"),
        )
    }

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
}

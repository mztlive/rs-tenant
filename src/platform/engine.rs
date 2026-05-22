use super::{
    PlatformAccessRequest, PlatformAuthorizationSource, PlatformGrantScope,
    PlatformPrincipalStatus, PlatformRoleId, PlatformSubject, TenantDataAccessRequest,
    TenantDataAccessScope, TenantDataScopeQuery, TenantScopedDataAccessRequest,
};
use crate::{AccessDecision, Error, Permission, Result};
use std::collections::HashSet;

/// Platform engine behavior configuration.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PlatformEngineConfig {
    /// Enables platform role inheritance traversal.
    pub enable_role_hierarchy: bool,
    /// Enables complete resource/action wildcard matching.
    pub enable_wildcard: bool,
    /// Maximum platform role inheritance depth.
    pub max_role_depth: usize,
}

impl Default for PlatformEngineConfig {
    fn default() -> Self {
        Self {
            enable_role_hierarchy: false,
            enable_wildcard: false,
            max_role_depth: 16,
        }
    }
}

/// Platform authorization engine.
#[derive(Debug)]
pub struct PlatformEngine<S> {
    source: S,
    config: PlatformEngineConfig,
}

/// Builder for [`PlatformEngine`].
pub struct PlatformEngineBuilder<S> {
    source: S,
    config: PlatformEngineConfig,
}

impl<S> PlatformEngineBuilder<S> {
    /// Creates a builder using default configuration.
    pub fn new(source: S) -> Self {
        Self {
            source,
            config: PlatformEngineConfig::default(),
        }
    }

    /// Replaces the full platform engine configuration.
    pub fn config(mut self, config: PlatformEngineConfig) -> Self {
        self.config = config;
        self
    }

    /// Enables or disables platform role inheritance.
    pub fn enable_role_hierarchy(mut self, on: bool) -> Self {
        self.config.enable_role_hierarchy = on;
        self
    }

    /// Enables or disables wildcard matching.
    pub fn enable_wildcard(mut self, on: bool) -> Self {
        self.config.enable_wildcard = on;
        self
    }

    /// Sets maximum platform role inheritance depth.
    pub fn max_role_depth(mut self, depth: usize) -> Self {
        self.config.max_role_depth = depth;
        self
    }

    /// Builds the platform engine.
    pub fn build(self) -> PlatformEngine<S> {
        PlatformEngine {
            source: self.source,
            config: self.config,
        }
    }
}

impl<S> PlatformEngine<S>
where
    S: PlatformAuthorizationSource,
{
    /// Returns the current platform engine configuration.
    pub fn config(&self) -> &PlatformEngineConfig {
        &self.config
    }

    /// Checks whether a platform principal can access a platform-owned resource.
    pub async fn can_platform(&self, request: PlatformAccessRequest) -> Result<AccessDecision> {
        let grants = self
            .matching_grants(&request.subject, &request.permission)
            .await?;
        let allowed = grants
            .into_iter()
            .any(|grant| matches!(grant.scope, PlatformGrantScope::Platform));
        Ok(decision(allowed))
    }

    /// Computes the accessible tenant data scope for a platform permission.
    pub async fn accessible_tenants(
        &self,
        query: TenantDataScopeQuery,
    ) -> Result<TenantDataAccessScope> {
        let grants = self
            .matching_grants(&query.subject, &query.permission)
            .await?;
        TenantDataAccessScope::merge(grants.into_iter().map(|grant| grant.scope))
    }

    /// Checks tenant-level data access for a platform principal.
    pub async fn can_access_tenant(
        &self,
        request: TenantDataAccessRequest,
    ) -> Result<AccessDecision> {
        let scope = self
            .accessible_tenants(TenantDataScopeQuery {
                subject: request.subject,
                permission: request.permission,
            })
            .await?;
        Ok(decision(scope.allows_tenant(&request.tenant)))
    }

    /// Checks tenant path data access for a platform principal.
    pub async fn can_access_tenant_scope(
        &self,
        request: TenantScopedDataAccessRequest,
    ) -> Result<AccessDecision> {
        let scope = self
            .accessible_tenants(TenantDataScopeQuery {
                subject: request.subject,
                permission: request.permission,
            })
            .await?;
        Ok(decision(
            scope.allows_path(&request.tenant, &request.target),
        ))
    }

    async fn matching_grants(
        &self,
        subject: &PlatformSubject,
        required: &Permission,
    ) -> Result<Vec<PlatformEffectiveGrant>> {
        if self.source.platform_principal_status(subject).await? != PlatformPrincipalStatus::Active
        {
            return Ok(Vec::new());
        }

        let grants = self.effective_grants(subject).await?;
        Ok(grants
            .into_iter()
            .filter(|grant| {
                grant
                    .permission
                    .matches(required, self.config.enable_wildcard)
            })
            .collect())
    }

    async fn effective_grants(
        &self,
        subject: &PlatformSubject,
    ) -> Result<Vec<PlatformEffectiveGrant>> {
        let assignments = self.source.platform_role_assignments(subject).await?;
        let mut grants = Vec::new();
        for assignment in assignments {
            let roles = if self.config.enable_role_hierarchy {
                self.expand_roles(assignment.role.clone()).await?
            } else {
                vec![assignment.role]
            };

            for role in roles {
                let permissions = self.source.platform_role_permissions(&role).await?;
                grants.extend(
                    permissions
                        .into_iter()
                        .map(|permission| PlatformEffectiveGrant {
                            permission,
                            scope: assignment.scope.clone(),
                        }),
                );
            }
        }
        Ok(grants)
    }

    async fn expand_roles(&self, root: PlatformRoleId) -> Result<Vec<PlatformRoleId>> {
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();
        let mut output = Vec::new();
        self.expand_from_role(root, &mut visited, &mut visiting, &mut output)
            .await?;
        Ok(output)
    }

    async fn expand_from_role(
        &self,
        root: PlatformRoleId,
        visited: &mut HashSet<PlatformRoleId>,
        visiting: &mut HashSet<PlatformRoleId>,
        output: &mut Vec<PlatformRoleId>,
    ) -> Result<()> {
        visiting.insert(root.clone());
        output.push(root.clone());
        let parents = self.source.platform_parent_roles(&root).await?;
        let mut stack: Vec<(PlatformRoleId, usize, std::vec::IntoIter<PlatformRoleId>)> =
            vec![(root, 0, parents.into_iter())];

        while let Some((current, depth, mut iter)) = stack.pop() {
            if let Some(parent) = iter.next() {
                stack.push((current.clone(), depth, iter));
                let next_depth = depth + 1;
                if next_depth > self.config.max_role_depth {
                    return Err(Error::PlatformRoleDepthExceeded {
                        role: parent,
                        max_depth: self.config.max_role_depth,
                    });
                }
                if visiting.contains(&parent) {
                    return Err(Error::PlatformRoleCycleDetected { role: parent });
                }
                if visited.contains(&parent) {
                    continue;
                }

                let parents = self.source.platform_parent_roles(&parent).await?;
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

#[derive(Clone, Debug)]
struct PlatformEffectiveGrant {
    permission: Permission,
    scope: PlatformGrantScope,
}

fn decision(allowed: bool) -> AccessDecision {
    if allowed {
        AccessDecision::Allow
    } else {
        AccessDecision::Deny
    }
}

#[cfg(all(test, feature = "memory-store"))]
mod tests {
    use super::*;
    use crate::platform::{MemoryPlatformSource, TenantScopedRoots};
    use crate::{Permission, ScopePath, ScopeRoots, TenantId};
    use futures::executor::block_on;

    fn principal() -> super::PlatformSubject {
        super::PlatformSubject::new(
            crate::platform::PlatformPrincipalId::parse("platform_admin").expect("principal"),
        )
    }

    fn role(value: &str) -> PlatformRoleId {
        PlatformRoleId::parse(value).expect("role")
    }

    fn tenant(value: &str) -> TenantId {
        TenantId::parse(value).expect("tenant")
    }

    fn path(value: &str) -> ScopePath {
        ScopePath::parse(value).expect("path")
    }

    fn active_source(
        scope: PlatformGrantScope,
        permission: &str,
    ) -> (MemoryPlatformSource, PlatformSubject) {
        let source = MemoryPlatformSource::new();
        let subject = principal();
        let role = role("platform_reader");
        source.set_platform_principal_status(
            subject.principal.clone(),
            PlatformPrincipalStatus::Active,
        );
        source.add_platform_role_assignment(subject.principal.clone(), role.clone(), scope);
        source
            .add_platform_role_permission(role, Permission::parse(permission).expect("permission"));
        (source, subject)
    }

    #[test]
    fn can_platform_should_deny_inactive_principal() {
        let source = MemoryPlatformSource::new();
        let engine = PlatformEngineBuilder::new(source).build();
        let decision = block_on(engine.can_platform(PlatformAccessRequest {
            subject: principal(),
            permission: Permission::parse("platform/role:update").expect("permission"),
        }))
        .expect("decision");

        assert_eq!(decision, AccessDecision::Deny);
    }

    #[test]
    fn can_platform_should_deny_without_role_assignments() {
        let source = MemoryPlatformSource::new();
        let subject = principal();
        source.set_platform_principal_status(
            subject.principal.clone(),
            PlatformPrincipalStatus::Active,
        );
        let engine = PlatformEngineBuilder::new(source).build();
        let decision = block_on(engine.can_platform(PlatformAccessRequest {
            subject,
            permission: Permission::parse("platform/role:update").expect("permission"),
        }))
        .expect("decision");

        assert_eq!(decision, AccessDecision::Deny);
    }

    #[test]
    fn can_platform_should_allow_platform_scope() {
        let (source, subject) =
            active_source(PlatformGrantScope::platform(), "platform/role:update");
        let engine = PlatformEngineBuilder::new(source).build();
        let decision = block_on(engine.can_platform(PlatformAccessRequest {
            subject,
            permission: Permission::parse("platform/role:update").expect("permission"),
        }))
        .expect("decision");

        assert_eq!(decision, AccessDecision::Allow);
    }

    #[test]
    fn platform_scope_should_not_access_tenant_data() {
        let (source, subject) = active_source(PlatformGrantScope::platform(), "tenant:read");
        let engine = PlatformEngineBuilder::new(source).build();
        let scope = block_on(engine.accessible_tenants(TenantDataScopeQuery {
            subject,
            permission: Permission::parse("tenant:read").expect("permission"),
        }))
        .expect("scope");

        assert_eq!(scope, TenantDataAccessScope::None);
    }

    #[test]
    fn all_tenants_should_access_any_tenant_data() {
        let (source, subject) = active_source(PlatformGrantScope::all_tenants(), "tenant:read");
        let engine = PlatformEngineBuilder::new(source).build();
        let decision = block_on(engine.can_access_tenant(TenantDataAccessRequest {
            subject,
            permission: Permission::parse("tenant:read").expect("permission"),
            tenant: tenant("tenant_b"),
        }))
        .expect("decision");

        assert_eq!(decision, AccessDecision::Allow);
    }

    #[test]
    fn tenant_set_should_allow_only_listed_tenant() {
        let (source, subject) = active_source(
            PlatformGrantScope::tenants(vec![tenant("tenant_a")]).expect("scope"),
            "tenant:read",
        );
        let engine = PlatformEngineBuilder::new(source).build();
        let decision = block_on(engine.can_access_tenant(TenantDataAccessRequest {
            subject,
            permission: Permission::parse("tenant:read").expect("permission"),
            tenant: tenant("tenant_b"),
        }))
        .expect("decision");

        assert_eq!(decision, AccessDecision::Deny);
    }

    #[test]
    fn tenant_paths_should_allow_descendant_path() {
        let (source, subject) = active_source(
            PlatformGrantScope::tenant_paths(vec![TenantScopedRoots::new(
                tenant("tenant_a"),
                ScopeRoots::new(vec![path("agent/1")]).expect("roots"),
            )])
            .expect("scope"),
            "tenant/order:read",
        );
        let engine = PlatformEngineBuilder::new(source).build();
        let decision = block_on(
            engine.can_access_tenant_scope(TenantScopedDataAccessRequest {
                subject,
                permission: Permission::parse("tenant/order:read").expect("permission"),
                tenant: tenant("tenant_a"),
                target: path("agent/1/store/2"),
            }),
        )
        .expect("decision");

        assert_eq!(decision, AccessDecision::Allow);
    }

    #[test]
    fn tenant_paths_should_deny_sibling_path() {
        let (source, subject) = active_source(
            PlatformGrantScope::tenant_paths(vec![TenantScopedRoots::new(
                tenant("tenant_a"),
                ScopeRoots::new(vec![path("agent/1")]).expect("roots"),
            )])
            .expect("scope"),
            "tenant/order:read",
        );
        let engine = PlatformEngineBuilder::new(source).build();
        let decision = block_on(
            engine.can_access_tenant_scope(TenantScopedDataAccessRequest {
                subject,
                permission: Permission::parse("tenant/order:read").expect("permission"),
                tenant: tenant("tenant_a"),
                target: path("agent/2/store/1"),
            }),
        )
        .expect("decision");

        assert_eq!(decision, AccessDecision::Deny);
    }

    #[test]
    fn tenant_paths_should_not_allow_tenant_level_access() {
        let (source, subject) = active_source(
            PlatformGrantScope::tenant_paths(vec![TenantScopedRoots::new(
                tenant("tenant_a"),
                ScopeRoots::new(vec![path("agent/1")]).expect("roots"),
            )])
            .expect("scope"),
            "tenant:read",
        );
        let engine = PlatformEngineBuilder::new(source).build();
        let decision = block_on(engine.can_access_tenant(TenantDataAccessRequest {
            subject,
            permission: Permission::parse("tenant:read").expect("permission"),
            tenant: tenant("tenant_a"),
        }))
        .expect("decision");

        assert_eq!(decision, AccessDecision::Deny);
    }

    #[test]
    fn multiple_role_assignments_should_merge_tenant_scope() {
        let source = MemoryPlatformSource::new();
        let subject = principal();
        let role_a = role("tenant_a_reader");
        let role_b = role("tenant_b_reader");
        let permission = Permission::parse("tenant:read").expect("permission");
        source.set_platform_principal_status(
            subject.principal.clone(),
            PlatformPrincipalStatus::Active,
        );
        source.add_platform_role_assignment(
            subject.principal.clone(),
            role_a.clone(),
            PlatformGrantScope::tenants(vec![tenant("tenant_a")]).expect("scope"),
        );
        source.add_platform_role_assignment(
            subject.principal.clone(),
            role_b.clone(),
            PlatformGrantScope::tenants(vec![tenant("tenant_b")]).expect("scope"),
        );
        source.add_platform_role_permission(role_a, permission.clone());
        source.add_platform_role_permission(role_b, permission.clone());
        let engine = PlatformEngineBuilder::new(source).build();
        let scope = block_on(engine.accessible_tenants(TenantDataScopeQuery {
            subject,
            permission,
        }))
        .expect("scope");

        assert_eq!(
            scope,
            TenantDataAccessScope::Tenants {
                tenants: vec![tenant("tenant_a"), tenant("tenant_b")]
            }
        );
    }

    #[test]
    fn mixed_tenant_and_path_assignments_should_return_error() {
        let source = MemoryPlatformSource::new();
        let subject = principal();
        let tenant_role = role("tenant_reader");
        let path_role = role("path_reader");
        let permission = Permission::parse("tenant/order:read").expect("permission");
        source.set_platform_principal_status(
            subject.principal.clone(),
            PlatformPrincipalStatus::Active,
        );
        source.add_platform_role_assignment(
            subject.principal.clone(),
            tenant_role.clone(),
            PlatformGrantScope::tenants(vec![tenant("tenant_a")]).expect("scope"),
        );
        source.add_platform_role_assignment(
            subject.principal.clone(),
            path_role.clone(),
            PlatformGrantScope::tenant_paths(vec![TenantScopedRoots::new(
                tenant("tenant_b"),
                ScopeRoots::new(vec![path("agent/1")]).expect("roots"),
            )])
            .expect("scope"),
        );
        source.add_platform_role_permission(tenant_role, permission.clone());
        source.add_platform_role_permission(path_role, permission.clone());
        let engine = PlatformEngineBuilder::new(source).build();

        let err = block_on(engine.accessible_tenants(TenantDataScopeQuery {
            subject: subject.clone(),
            permission: permission.clone(),
        }))
        .expect_err("must reject mixed grants");
        assert!(err.to_string().contains("must not mix"));

        let err = block_on(
            engine.can_access_tenant_scope(TenantScopedDataAccessRequest {
                subject,
                permission,
                tenant: tenant("tenant_b"),
                target: path("agent/1/order/1"),
            }),
        )
        .expect_err("must reject mixed grants consistently");
        assert!(err.to_string().contains("must not mix"));
    }

    #[test]
    fn role_hierarchy_should_use_parent_permissions() {
        let source = MemoryPlatformSource::new();
        let subject = principal();
        let child = role("child");
        let parent = role("parent");
        source.set_platform_principal_status(
            subject.principal.clone(),
            PlatformPrincipalStatus::Active,
        );
        source.add_platform_role_assignment(
            subject.principal.clone(),
            child.clone(),
            PlatformGrantScope::platform(),
        );
        source.add_platform_parent_role(child, parent.clone());
        source.add_platform_role_permission(
            parent,
            Permission::parse("platform/role:update").expect("permission"),
        );
        let engine = PlatformEngineBuilder::new(source)
            .enable_role_hierarchy(true)
            .build();
        let decision = block_on(engine.can_platform(PlatformAccessRequest {
            subject,
            permission: Permission::parse("platform/role:update").expect("permission"),
        }))
        .expect("decision");

        assert_eq!(decision, AccessDecision::Allow);
    }

    #[test]
    fn role_hierarchy_should_detect_cycle() {
        let source = MemoryPlatformSource::new();
        let subject = principal();
        let child = role("child");
        let parent = role("parent");
        source.set_platform_principal_status(
            subject.principal.clone(),
            PlatformPrincipalStatus::Active,
        );
        source.add_platform_role_assignment(
            subject.principal.clone(),
            child.clone(),
            PlatformGrantScope::platform(),
        );
        source.add_platform_parent_role(child.clone(), parent.clone());
        source.add_platform_parent_role(parent, child.clone());
        let engine = PlatformEngineBuilder::new(source)
            .enable_role_hierarchy(true)
            .build();
        let err = block_on(engine.can_platform(PlatformAccessRequest {
            subject,
            permission: Permission::parse("platform/role:update").expect("permission"),
        }))
        .expect_err("must detect cycle");

        assert!(matches!(
            err,
            Error::PlatformRoleCycleDetected { role } if role == child
        ));
    }

    #[test]
    fn role_hierarchy_should_limit_max_depth() {
        let source = MemoryPlatformSource::new();
        let subject = principal();
        let child = role("child");
        let parent = role("parent");
        let grandparent = role("grandparent");
        source.set_platform_principal_status(
            subject.principal.clone(),
            PlatformPrincipalStatus::Active,
        );
        source.add_platform_role_assignment(
            subject.principal.clone(),
            child.clone(),
            PlatformGrantScope::platform(),
        );
        source.add_platform_parent_role(child, parent);
        source.add_platform_parent_role(role("parent"), grandparent.clone());
        let engine = PlatformEngineBuilder::new(source)
            .enable_role_hierarchy(true)
            .max_role_depth(1)
            .build();
        let err = block_on(engine.can_platform(PlatformAccessRequest {
            subject,
            permission: Permission::parse("platform/role:update").expect("permission"),
        }))
        .expect_err("must enforce depth");

        assert!(matches!(
            err,
            Error::PlatformRoleDepthExceeded { role, max_depth: 1 } if role == grandparent
        ));
    }

    #[test]
    fn wildcard_should_require_config_flag() {
        let (source, subject) = active_source(PlatformGrantScope::platform(), "platform/role:*");
        let strict_engine = PlatformEngineBuilder::new(source.clone()).build();
        let wildcard_engine = PlatformEngineBuilder::new(source)
            .enable_wildcard(true)
            .build();
        let request = PlatformAccessRequest {
            subject,
            permission: Permission::parse("platform/role:update").expect("permission"),
        };

        let strict = block_on(strict_engine.can_platform(request.clone())).expect("decision");
        let wildcard = block_on(wildcard_engine.can_platform(request)).expect("decision");

        assert_eq!(strict, AccessDecision::Deny);
        assert_eq!(wildcard, AccessDecision::Allow);
    }
}

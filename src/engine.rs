use crate::cache::{Cache, NoCache};
use crate::error::{Error, Result};
use crate::permission::{Permission, permission_matches, resource_matches};
use crate::store::Store;
use crate::types::{PrincipalId, ResourceName, RoleId, TenantId};
use std::collections::HashSet;

const CACHE_SIGNATURE_PREFIX: &str = "__rs_tenant_cache_sig__=";

/// Authorization decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Permission is granted.
    Allow,
    /// Permission is denied.
    Deny,
}

/// Scope result for resource filtering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// No access to the resource.
    None,
    /// Access limited to a tenant.
    TenantOnly { tenant: TenantId },
}

/// RBAC engine with pluggable store and optional cache.
#[derive(Debug)]
pub struct Engine<S, C = NoCache> {
    store: S,
    cache: C,
    cache_signature_marker: Permission,
    enable_role_hierarchy: bool,
    enable_wildcard: bool,
    max_inherit_depth: usize,
    permission_normalize: bool,
}

/// Builder for [`Engine`].
pub struct EngineBuilder<S, C = NoCache> {
    store: S,
    cache: C,
    enable_role_hierarchy: bool,
    enable_wildcard: bool,
    max_inherit_depth: usize,
    permission_normalize: bool,
}

impl<S> EngineBuilder<S, NoCache> {
    /// Creates a new builder with default configuration.
    pub fn new(store: S) -> Self {
        Self {
            store,
            cache: NoCache,
            enable_role_hierarchy: false,
            enable_wildcard: false,
            max_inherit_depth: 16,
            permission_normalize: true,
        }
    }
}

impl<S, C> EngineBuilder<S, C> {
    /// Enables or disables role inheritance.
    pub fn enable_role_hierarchy(mut self, on: bool) -> Self {
        self.enable_role_hierarchy = on;
        self
    }

    /// Enables or disables wildcard permission matching.
    pub fn enable_wildcard(mut self, on: bool) -> Self {
        self.enable_wildcard = on;
        self
    }

    /// Sets maximum inheritance depth.
    pub fn max_inherit_depth(mut self, depth: usize) -> Self {
        self.max_inherit_depth = depth;
        self
    }

    /// Enables or disables permission normalization for matching.
    pub fn permission_normalize(mut self, on: bool) -> Self {
        self.permission_normalize = on;
        self
    }

    /// Sets the cache implementation.
    pub fn cache<C2: Cache>(self, cache: C2) -> EngineBuilder<S, C2> {
        EngineBuilder {
            store: self.store,
            cache,
            enable_role_hierarchy: self.enable_role_hierarchy,
            enable_wildcard: self.enable_wildcard,
            max_inherit_depth: self.max_inherit_depth,
            permission_normalize: self.permission_normalize,
        }
    }

    /// Builds the engine.
    pub fn build(self) -> Engine<S, C> {
        let cache_signature_marker = Permission::from_string(format!(
            "{CACHE_SIGNATURE_PREFIX}rh:{};wc:{};depth:{};norm:{}",
            u8::from(self.enable_role_hierarchy),
            u8::from(self.enable_wildcard),
            self.max_inherit_depth,
            u8::from(self.permission_normalize),
        ));

        Engine {
            store: self.store,
            cache: self.cache,
            cache_signature_marker,
            enable_role_hierarchy: self.enable_role_hierarchy,
            enable_wildcard: self.enable_wildcard,
            max_inherit_depth: self.max_inherit_depth,
            permission_normalize: self.permission_normalize,
        }
    }
}

impl<S, C> Engine<S, C>
where
    S: Store,
    C: Cache,
{
    /// Authorizes a principal for a permission within a tenant.
    pub async fn authorize(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
        permission: Permission,
    ) -> Result<Decision> {
        if !self
            .store
            .tenant_active(tenant.clone())
            .await
            .map_err(Error::from)?
        {
            return Ok(Decision::Deny);
        }
        if !self
            .store
            .principal_active(tenant.clone(), principal.clone())
            .await
            .map_err(Error::from)?
        {
            return Ok(Decision::Deny);
        }

        let permissions = self.effective_permissions(&tenant, &principal).await?;
        let allowed = permissions.iter().any(|granted| {
            permission_matches(
                granted,
                &permission,
                self.enable_wildcard,
                self.permission_normalize,
            )
        });

        Ok(if allowed {
            Decision::Allow
        } else {
            Decision::Deny
        })
    }

    /// Computes scope for a resource within a tenant.
    pub async fn scope(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
        resource: ResourceName,
    ) -> Result<Scope> {
        if !self
            .store
            .tenant_active(tenant.clone())
            .await
            .map_err(Error::from)?
        {
            return Ok(Scope::None);
        }
        if !self
            .store
            .principal_active(tenant.clone(), principal.clone())
            .await
            .map_err(Error::from)?
        {
            return Ok(Scope::None);
        }

        let permissions = self.effective_permissions(&tenant, &principal).await?;
        let allowed = permissions.iter().any(|granted| {
            resource_matches(
                granted,
                &resource,
                self.enable_wildcard,
                self.permission_normalize,
            )
        });

        Ok(if allowed {
            Scope::TenantOnly { tenant }
        } else {
            Scope::None
        })
    }

    async fn effective_permissions(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> Result<Vec<Permission>> {
        if let Some(cached) = self.cache.get_permissions(tenant, principal).await
            && let Some(perms) = self.decode_cached_permissions(cached)
        {
            return Ok(perms);
        }

        let direct_roles = self
            .store
            .principal_roles(tenant.clone(), principal.clone())
            .await
            .map_err(Error::from)?;
        let roles = if self.enable_role_hierarchy {
            self.expand_roles(tenant, direct_roles).await?
        } else {
            direct_roles
        };

        let mut permissions = HashSet::new();
        for role in roles {
            let role_permissions = self
                .store
                .role_permissions(tenant.clone(), role)
                .await
                .map_err(Error::from)?;
            permissions.extend(role_permissions);
        }

        let global_roles = self
            .store
            .global_roles(principal.clone())
            .await
            .map_err(Error::from)?;
        for role in global_roles {
            let global_permissions = self
                .store
                .global_role_permissions(role)
                .await
                .map_err(Error::from)?;
            permissions.extend(global_permissions);
        }

        let perms: Vec<Permission> = permissions.into_iter().collect();
        let cached = self.encode_cached_permissions(perms.clone());
        self.cache.set_permissions(tenant, principal, cached).await;
        Ok(perms)
    }

    fn encode_cached_permissions(&self, perms: Vec<Permission>) -> Vec<Permission> {
        let mut cached = Vec::with_capacity(perms.len() + 1);
        cached.push(self.cache_signature_marker.clone());
        cached.extend(perms);
        cached
    }

    fn decode_cached_permissions(&self, cached: Vec<Permission>) -> Option<Vec<Permission>> {
        let mut iter = cached.into_iter();
        let marker = iter.next()?;
        if marker != self.cache_signature_marker {
            return None;
        }
        Some(iter.collect())
    }

    async fn expand_roles(&self, tenant: &TenantId, roles: Vec<RoleId>) -> Result<Vec<RoleId>> {
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();
        let mut output = Vec::new();

        for role in roles {
            if visited.contains(&role) {
                continue;
            }
            self.expand_from_role(tenant, role, &mut visited, &mut visiting, &mut output)
                .await?;
        }

        Ok(output)
    }

    async fn expand_from_role(
        &self,
        tenant: &TenantId,
        role: RoleId,
        visited: &mut HashSet<RoleId>,
        visiting: &mut HashSet<RoleId>,
        output: &mut Vec<RoleId>,
    ) -> Result<()> {
        let parents = self
            .store
            .role_inherits(tenant.clone(), role.clone())
            .await
            .map_err(Error::from)?;
        visiting.insert(role.clone());
        output.push(role.clone());

        let mut stack: Vec<(RoleId, usize, std::vec::IntoIter<RoleId>)> =
            vec![(role, 0, parents.into_iter())];

        while let Some((current, depth, mut iter)) = stack.pop() {
            if let Some(parent) = iter.next() {
                stack.push((current.clone(), depth, iter));

                let next_depth = depth + 1;
                if next_depth > self.max_inherit_depth {
                    return Err(Error::RoleDepthExceeded {
                        tenant: tenant.clone(),
                        role: parent,
                        max_depth: self.max_inherit_depth,
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

                let parents = self
                    .store
                    .role_inherits(tenant.clone(), parent.clone())
                    .await
                    .map_err(Error::from)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::Permission;
    use crate::store::{GlobalRoleStore, RoleStore, TenantStore};
    use crate::types::{GlobalRoleId, PrincipalId, ResourceName, RoleId, TenantId};
    use async_trait::async_trait;
    use futures::executor::block_on;
    use std::collections::HashMap;

    #[derive(Default, Clone)]
    struct TestStore {
        tenant_active: bool,
        principal_active: bool,
        roles: Vec<RoleId>,
        role_permissions: HashMap<RoleId, Vec<Permission>>,
        role_inherits: HashMap<RoleId, Vec<RoleId>>,
        global_roles: HashMap<PrincipalId, Vec<GlobalRoleId>>,
        global_role_permissions: HashMap<GlobalRoleId, Vec<Permission>>,
    }

    fn active_store() -> TestStore {
        TestStore {
            tenant_active: true,
            principal_active: true,
            ..TestStore::default()
        }
    }

    #[async_trait]
    impl TenantStore for TestStore {
        async fn tenant_active(
            &self,
            _tenant: TenantId,
        ) -> std::result::Result<bool, crate::StoreError> {
            Ok(self.tenant_active)
        }

        async fn principal_active(
            &self,
            _tenant: TenantId,
            _principal: PrincipalId,
        ) -> std::result::Result<bool, crate::StoreError> {
            Ok(self.principal_active)
        }
    }

    #[async_trait]
    impl RoleStore for TestStore {
        async fn principal_roles(
            &self,
            _tenant: TenantId,
            _principal: PrincipalId,
        ) -> std::result::Result<Vec<RoleId>, crate::StoreError> {
            Ok(self.roles.clone())
        }

        async fn role_permissions(
            &self,
            _tenant: TenantId,
            role: RoleId,
        ) -> std::result::Result<Vec<Permission>, crate::StoreError> {
            Ok(self
                .role_permissions
                .get(&role)
                .cloned()
                .unwrap_or_default())
        }

        async fn role_inherits(
            &self,
            _tenant: TenantId,
            role: RoleId,
        ) -> std::result::Result<Vec<RoleId>, crate::StoreError> {
            Ok(self.role_inherits.get(&role).cloned().unwrap_or_default())
        }
    }

    #[async_trait]
    impl GlobalRoleStore for TestStore {
        async fn global_roles(
            &self,
            principal: PrincipalId,
        ) -> std::result::Result<Vec<GlobalRoleId>, crate::StoreError> {
            Ok(self
                .global_roles
                .get(&principal)
                .cloned()
                .unwrap_or_default())
        }

        async fn global_role_permissions(
            &self,
            role: GlobalRoleId,
        ) -> std::result::Result<Vec<Permission>, crate::StoreError> {
            Ok(self
                .global_role_permissions
                .get(&role)
                .cloned()
                .unwrap_or_default())
        }
    }

    #[test]
    fn authorize_should_allow_exact_permission() {
        let mut store = active_store();

        let role = RoleId::try_from("role_a").unwrap();
        store.roles = vec![role.clone()];
        store
            .role_permissions
            .insert(role, vec![Permission::try_from("invoice:read").unwrap()]);

        let engine = EngineBuilder::new(store).build();
        let decision = block_on(engine.authorize(
            TenantId::try_from("tenant_1").unwrap(),
            PrincipalId::try_from("user_1").unwrap(),
            Permission::try_from("invoice:read").unwrap(),
        ))
        .unwrap();

        assert_eq!(decision, Decision::Allow);
    }

    #[test]
    fn authorize_should_deny_when_tenant_inactive() {
        let store = TestStore {
            tenant_active: false,
            principal_active: true,
            ..TestStore::default()
        };

        let engine = EngineBuilder::new(store).build();
        let decision = block_on(engine.authorize(
            TenantId::try_from("tenant_1").unwrap(),
            PrincipalId::try_from("user_1").unwrap(),
            Permission::try_from("invoice:read").unwrap(),
        ))
        .unwrap();

        assert_eq!(decision, Decision::Deny);
    }

    #[test]
    fn authorize_should_allow_with_wildcard_when_enabled() {
        let mut store = active_store();

        let role = RoleId::try_from("role_a").unwrap();
        store.roles = vec![role.clone()];
        store
            .role_permissions
            .insert(role, vec![Permission::try_from("invoice:*").unwrap()]);

        let engine = EngineBuilder::new(store).enable_wildcard(true).build();
        let decision = block_on(engine.authorize(
            TenantId::try_from("tenant_1").unwrap(),
            PrincipalId::try_from("user_1").unwrap(),
            Permission::try_from("invoice:read").unwrap(),
        ))
        .unwrap();

        assert_eq!(decision, Decision::Allow);
    }

    #[test]
    fn authorize_should_deny_wildcard_when_disabled() {
        let mut store = active_store();

        let role = RoleId::try_from("role_a").unwrap();
        store.roles = vec![role.clone()];
        store
            .role_permissions
            .insert(role, vec![Permission::try_from("invoice:*").unwrap()]);

        let engine = EngineBuilder::new(store).build();
        let decision = block_on(engine.authorize(
            TenantId::try_from("tenant_1").unwrap(),
            PrincipalId::try_from("user_1").unwrap(),
            Permission::try_from("invoice:read").unwrap(),
        ))
        .unwrap();

        assert_eq!(decision, Decision::Deny);
    }

    #[test]
    fn authorize_should_allow_via_global_role() {
        let mut store = active_store();

        let principal = PrincipalId::try_from("user_1").unwrap();
        let global_role = GlobalRoleId::try_from("global_admin").unwrap();
        store
            .global_roles
            .insert(principal.clone(), vec![global_role.clone()]);
        store.global_role_permissions.insert(
            global_role,
            vec![Permission::try_from("invoice:read").unwrap()],
        );

        let engine = EngineBuilder::new(store).build();
        let decision = block_on(engine.authorize(
            TenantId::try_from("tenant_1").unwrap(),
            principal,
            Permission::try_from("invoice:read").unwrap(),
        ))
        .unwrap();

        assert_eq!(decision, Decision::Allow);
    }

    #[test]
    fn scope_should_return_tenant_only_when_resource_matches() {
        let mut store = active_store();

        let role = RoleId::try_from("role_a").unwrap();
        store.roles = vec![role.clone()];
        store
            .role_permissions
            .insert(role, vec![Permission::try_from("invoice:read").unwrap()]);

        let engine = EngineBuilder::new(store).build();
        let scope = block_on(engine.scope(
            TenantId::try_from("tenant_1").unwrap(),
            PrincipalId::try_from("user_1").unwrap(),
            ResourceName::try_from("invoice").unwrap(),
        ))
        .unwrap();

        assert!(matches!(scope, Scope::TenantOnly { .. }));
    }

    #[test]
    fn scope_should_return_none_when_resource_not_allowed() {
        let mut store = active_store();

        let role = RoleId::try_from("role_a").unwrap();
        store.roles = vec![role.clone()];
        store
            .role_permissions
            .insert(role, vec![Permission::try_from("invoice:read").unwrap()]);

        let engine = EngineBuilder::new(store).build();
        let scope = block_on(engine.scope(
            TenantId::try_from("tenant_1").unwrap(),
            PrincipalId::try_from("user_1").unwrap(),
            ResourceName::try_from("customer").unwrap(),
        ))
        .unwrap();

        assert_eq!(scope, Scope::None);
    }

    #[test]
    fn scope_should_ignore_wildcard_permission_when_disabled() {
        let mut store = active_store();

        let role = RoleId::try_from("role_a").unwrap();
        store.roles = vec![role.clone()];
        store
            .role_permissions
            .insert(role, vec![Permission::try_from("invoice:*").unwrap()]);

        let engine = EngineBuilder::new(store).build();
        let scope = block_on(engine.scope(
            TenantId::try_from("tenant_1").unwrap(),
            PrincipalId::try_from("user_1").unwrap(),
            ResourceName::try_from("invoice").unwrap(),
        ))
        .unwrap();

        assert_eq!(scope, Scope::None);
    }

    #[cfg(feature = "memory-cache")]
    #[test]
    fn shared_cache_should_isolate_different_engine_configs() {
        let mut store = active_store();

        let role = RoleId::try_from("role_a").unwrap();
        store.roles = vec![role.clone()];
        store
            .role_permissions
            .insert(role, vec![Permission::try_from("invoice:*").unwrap()]);

        let cache = crate::MemoryCache::new(8);
        let wildcard_engine = EngineBuilder::new(store.clone())
            .enable_wildcard(true)
            .cache(cache.clone())
            .build();
        let strict_engine = EngineBuilder::new(store).cache(cache).build();

        let tenant = TenantId::try_from("tenant_1").unwrap();
        let principal = PrincipalId::try_from("user_1").unwrap();
        let required = Permission::try_from("invoice:read").unwrap();

        let wildcard_decision = block_on(wildcard_engine.authorize(
            tenant.clone(),
            principal.clone(),
            required.clone(),
        ))
        .unwrap();
        assert_eq!(wildcard_decision, Decision::Allow);

        let strict_decision =
            block_on(strict_engine.authorize(tenant, principal, required)).unwrap();
        assert_eq!(strict_decision, Decision::Deny);
    }

    #[test]
    fn role_cycle_should_return_error() {
        let mut store = active_store();

        let role_a = RoleId::try_from("role_a").unwrap();
        let role_b = RoleId::try_from("role_b").unwrap();
        store.roles = vec![role_a.clone()];
        store.role_permissions.insert(
            role_a.clone(),
            vec![Permission::try_from("invoice:read").unwrap()],
        );
        store
            .role_inherits
            .insert(role_a.clone(), vec![role_b.clone()]);
        store.role_inherits.insert(role_b, vec![role_a.clone()]);

        let engine = EngineBuilder::new(store)
            .enable_role_hierarchy(true)
            .build();
        let result = block_on(engine.authorize(
            TenantId::try_from("tenant_1").unwrap(),
            PrincipalId::try_from("user_1").unwrap(),
            Permission::try_from("invoice:read").unwrap(),
        ));

        assert!(matches!(result, Err(Error::RoleCycleDetected { .. })));
    }

    #[test]
    fn role_depth_should_return_error_when_exceeded() {
        let mut store = active_store();

        let role_a = RoleId::try_from("role_a").unwrap();
        let role_b = RoleId::try_from("role_b").unwrap();
        let role_c = RoleId::try_from("role_c").unwrap();
        store.roles = vec![role_a.clone()];
        store
            .role_inherits
            .insert(role_a.clone(), vec![role_b.clone()]);
        store.role_inherits.insert(role_b, vec![role_c]);

        let engine = EngineBuilder::new(store)
            .enable_role_hierarchy(true)
            .max_inherit_depth(1)
            .build();
        let result = block_on(engine.authorize(
            TenantId::try_from("tenant_1").unwrap(),
            PrincipalId::try_from("user_1").unwrap(),
            Permission::try_from("invoice:read").unwrap(),
        ));

        assert!(matches!(result, Err(Error::RoleDepthExceeded { .. })));
    }
}

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use async_trait::async_trait;
use crate::permission::Permission;
use crate::store::{GlobalRoleStore, RoleStore, TenantStore};
use crate::types::{GlobalRoleId, PrincipalId, RoleId, TenantId};

/// In-memory store implementation for tests and demos.
#[derive(Debug, Default, Clone)]
pub struct MemoryStore {
    inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    tenants: RwLock<HashMap<TenantId, bool>>,
    principals: RwLock<HashMap<(TenantId, PrincipalId), bool>>,
    principal_roles: RwLock<HashMap<(TenantId, PrincipalId), HashSet<RoleId>>>,
    role_permissions: RwLock<HashMap<(TenantId, RoleId), HashSet<Permission>>>,
    role_inherits: RwLock<HashMap<(TenantId, RoleId), HashSet<RoleId>>>,
    global_roles: RwLock<HashMap<PrincipalId, HashSet<GlobalRoleId>>>,
    global_role_permissions: RwLock<HashMap<GlobalRoleId, HashSet<Permission>>>,
}

impl MemoryStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets tenant active status.
    pub fn set_tenant_active(&self, tenant: TenantId, active: bool) {
        let mut guard = self.inner.tenants.write().expect("poisoned lock");
        guard.insert(tenant, active);
    }

    /// Sets principal active status within a tenant.
    pub fn set_principal_active(&self, tenant: TenantId, principal: PrincipalId, active: bool) {
        let mut guard = self.inner.principals.write().expect("poisoned lock");
        guard.insert((tenant, principal), active);
    }

    /// Adds a role to a principal.
    pub fn add_principal_role(&self, tenant: TenantId, principal: PrincipalId, role: RoleId) {
        let mut guard = self.inner.principal_roles.write().expect("poisoned lock");
        guard
            .entry((tenant, principal))
            .or_default()
            .insert(role);
    }

    /// Adds a permission to a role.
    pub fn add_role_permission(&self, tenant: TenantId, role: RoleId, permission: Permission) {
        let mut guard = self.inner.role_permissions.write().expect("poisoned lock");
        guard.entry((tenant, role)).or_default().insert(permission);
    }

    /// Adds an inheritance edge for a role.
    pub fn add_role_inherit(&self, tenant: TenantId, role: RoleId, parent: RoleId) {
        let mut guard = self.inner.role_inherits.write().expect("poisoned lock");
        guard.entry((tenant, role)).or_default().insert(parent);
    }

    /// Adds a global role to a principal.
    pub fn add_global_role(&self, principal: PrincipalId, role: GlobalRoleId) {
        let mut guard = self.inner.global_roles.write().expect("poisoned lock");
        guard.entry(principal).or_default().insert(role);
    }

    /// Adds a permission to a global role.
    pub fn add_global_role_permission(&self, role: GlobalRoleId, permission: Permission) {
        let mut guard = self
            .inner
            .global_role_permissions
            .write()
            .expect("poisoned lock");
        guard.entry(role).or_default().insert(permission);
    }
}

#[async_trait]
impl TenantStore for MemoryStore {
    async fn tenant_active(
        &self,
        tenant: TenantId,
    ) -> std::result::Result<bool, crate::StoreError> {
        let guard = self.inner.tenants.read().expect("poisoned lock");
        Ok(guard.get(&tenant).copied().unwrap_or(false))
    }

    async fn principal_active(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
    ) -> std::result::Result<bool, crate::StoreError> {
        let guard = self.inner.principals.read().expect("poisoned lock");
        Ok(guard
            .get(&(tenant, principal))
            .copied()
            .unwrap_or(false))
    }
}

#[async_trait]
impl RoleStore for MemoryStore {
    async fn principal_roles(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
    ) -> std::result::Result<Vec<RoleId>, crate::StoreError> {
        let guard = self.inner.principal_roles.read().expect("poisoned lock");
        Ok(guard
            .get(&(tenant, principal))
            .map(|roles| roles.iter().cloned().collect())
            .unwrap_or_default())
    }

    async fn role_permissions(
        &self,
        tenant: TenantId,
        role: RoleId,
    ) -> std::result::Result<Vec<Permission>, crate::StoreError> {
        let guard = self.inner.role_permissions.read().expect("poisoned lock");
        Ok(guard
            .get(&(tenant, role))
            .map(|perms| perms.iter().cloned().collect())
            .unwrap_or_default())
    }

    async fn role_inherits(
        &self,
        tenant: TenantId,
        role: RoleId,
    ) -> std::result::Result<Vec<RoleId>, crate::StoreError> {
        let guard = self.inner.role_inherits.read().expect("poisoned lock");
        Ok(guard
            .get(&(tenant, role))
            .map(|roles| roles.iter().cloned().collect())
            .unwrap_or_default())
    }
}

#[async_trait]
impl GlobalRoleStore for MemoryStore {
    async fn global_roles(
        &self,
        principal: PrincipalId,
    ) -> std::result::Result<Vec<GlobalRoleId>, crate::StoreError> {
        let guard = self.inner.global_roles.read().expect("poisoned lock");
        Ok(guard
            .get(&principal)
            .map(|roles| roles.iter().cloned().collect())
            .unwrap_or_default())
    }

    async fn global_role_permissions(
        &self,
        role: GlobalRoleId,
    ) -> std::result::Result<Vec<Permission>, crate::StoreError> {
        let guard = self
            .inner
            .global_role_permissions
            .read()
            .expect("poisoned lock");
        Ok(guard
            .get(&role)
            .map(|perms| perms.iter().cloned().collect())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use super::*;

    #[test]
    fn memory_store_should_support_basic_flow() {
        let store = MemoryStore::new();
        let tenant = TenantId::try_from("tenant_1").unwrap();
        let principal = PrincipalId::try_from("user_1").unwrap();
        let role = RoleId::try_from("role_a").unwrap();
        let perm = Permission::try_from("invoice:read").unwrap();

        store.set_tenant_active(tenant.clone(), true);
        store.set_principal_active(tenant.clone(), principal.clone(), true);
        store.add_principal_role(tenant.clone(), principal.clone(), role.clone());
        store.add_role_permission(tenant.clone(), role, perm.clone());

        let engine = crate::EngineBuilder::new(store).build();
        let decision = block_on(engine.authorize(tenant, principal, perm)).unwrap();

        assert_eq!(decision, crate::Decision::Allow);
    }
}

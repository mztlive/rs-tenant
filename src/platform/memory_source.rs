use super::{
    PlatformAuthorizationSource, PlatformGrantScope, PlatformPrincipalId, PlatformPrincipalStatus,
    PlatformRoleAssignment, PlatformRoleId, PlatformSubject,
};
use crate::{Permission, SourceError};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// In-memory platform authorization source for tests and demos.
#[derive(Debug, Default, Clone)]
pub struct MemoryPlatformSource {
    inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    principals: RwLock<HashMap<PlatformPrincipalId, PlatformPrincipalStatus>>,
    assignments: RwLock<HashMap<PlatformPrincipalId, Vec<PlatformRoleAssignment>>>,
    role_permissions: RwLock<HashMap<PlatformRoleId, HashSet<Permission>>>,
    parent_roles: RwLock<HashMap<PlatformRoleId, HashSet<PlatformRoleId>>>,
}

fn read_guard<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn write_guard<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl MemoryPlatformSource {
    /// Creates an empty platform source.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets principal status.
    pub fn set_principal_status(
        &self,
        principal: PlatformPrincipalId,
        status: PlatformPrincipalStatus,
    ) {
        write_guard(&self.inner.principals).insert(principal, status);
    }

    /// Adds a role assignment.
    pub fn add_role_assignment(
        &self,
        principal: PlatformPrincipalId,
        role: PlatformRoleId,
        scope: PlatformGrantScope,
    ) {
        write_guard(&self.inner.assignments)
            .entry(principal)
            .or_default()
            .push(PlatformRoleAssignment::new(role, scope));
    }

    /// Adds a permission to a role.
    pub fn add_role_permission(&self, role: PlatformRoleId, permission: Permission) {
        write_guard(&self.inner.role_permissions)
            .entry(role)
            .or_default()
            .insert(permission);
    }

    /// Adds a direct parent role.
    pub fn add_parent_role(&self, role: PlatformRoleId, parent: PlatformRoleId) {
        write_guard(&self.inner.parent_roles)
            .entry(role)
            .or_default()
            .insert(parent);
    }
}

#[async_trait]
impl PlatformAuthorizationSource for MemoryPlatformSource {
    async fn platform_principal_status(
        &self,
        subject: &PlatformSubject,
    ) -> std::result::Result<PlatformPrincipalStatus, SourceError> {
        Ok(read_guard(&self.inner.principals)
            .get(&subject.principal)
            .copied()
            .unwrap_or(PlatformPrincipalStatus::Inactive))
    }

    async fn platform_role_assignments(
        &self,
        subject: &PlatformSubject,
    ) -> std::result::Result<Vec<PlatformRoleAssignment>, SourceError> {
        Ok(read_guard(&self.inner.assignments)
            .get(&subject.principal)
            .cloned()
            .unwrap_or_default())
    }

    async fn platform_role_permissions(
        &self,
        role: &PlatformRoleId,
    ) -> std::result::Result<Vec<Permission>, SourceError> {
        Ok(read_guard(&self.inner.role_permissions)
            .get(role)
            .map(|permissions| permissions.iter().cloned().collect())
            .unwrap_or_default())
    }

    async fn platform_parent_roles(
        &self,
        role: &PlatformRoleId,
    ) -> std::result::Result<Vec<PlatformRoleId>, SourceError> {
        Ok(read_guard(&self.inner.parent_roles)
            .get(role)
            .map(|parents| parents.iter().cloned().collect())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn memory_platform_source_should_default_to_inactive() {
        let source = MemoryPlatformSource::new();
        let subject =
            PlatformSubject::new(PlatformPrincipalId::parse("platform_admin").expect("principal"));
        let status = block_on(source.platform_principal_status(&subject)).expect("status");

        assert_eq!(status, PlatformPrincipalStatus::Inactive);
    }

    #[test]
    fn memory_platform_source_should_recover_from_poisoned_lock() {
        let source = MemoryPlatformSource::new();
        let inner = source.inner.clone();
        let _ = std::thread::spawn(move || {
            let _guard = inner.principals.write().unwrap();
            panic!("poison principals lock");
        })
        .join();

        let principal = PlatformPrincipalId::parse("platform_admin").expect("principal");
        source.set_principal_status(principal.clone(), PlatformPrincipalStatus::Active);
        let subject = PlatformSubject::new(principal);
        let status = block_on(source.platform_principal_status(&subject)).expect("status");

        assert_eq!(status, PlatformPrincipalStatus::Active);
    }
}

use crate::permission::Permission;
use crate::types::{PrincipalId, RoleId, TenantId};
use async_trait::async_trait;

/// Cache interface for effective permissions.
#[async_trait]
pub trait Cache: Send + Sync {
    /// Gets cached permissions for a (tenant, principal) pair.
    async fn get_permissions(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> Option<Vec<Permission>>;

    /// Sets cached permissions for a (tenant, principal) pair.
    async fn set_permissions(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
        perms: Vec<Permission>,
    );

    /// Invalidates cache for a principal.
    async fn invalidate_principal(&self, tenant: &TenantId, principal: &PrincipalId);

    /// Invalidates cache for a role.
    async fn invalidate_role(&self, tenant: &TenantId, role: &RoleId);

    /// Invalidates cache for a tenant.
    async fn invalidate_tenant(&self, tenant: &TenantId);
}

/// No-op cache implementation.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoCache;

#[async_trait]
impl Cache for NoCache {
    async fn get_permissions(
        &self,
        _tenant: &TenantId,
        _principal: &PrincipalId,
    ) -> Option<Vec<Permission>> {
        None
    }

    async fn set_permissions(
        &self,
        _tenant: &TenantId,
        _principal: &PrincipalId,
        _perms: Vec<Permission>,
    ) {
    }

    async fn invalidate_principal(&self, _tenant: &TenantId, _principal: &PrincipalId) {}

    async fn invalidate_role(&self, _tenant: &TenantId, _role: &RoleId) {}

    async fn invalidate_tenant(&self, _tenant: &TenantId) {}
}

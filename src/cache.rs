use crate::ids::{PrincipalId, RoleId, TenantId};
use crate::permission::Permission;
use crate::scope::GrantScope;
use async_trait::async_trait;

/// Internal effective grant cached per tenant principal and engine config.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveGrant {
    /// Role from which the permission was read.
    pub role: RoleId,
    /// Permission granted by the role.
    pub permission: Permission,
    /// Scope attached to the original role assignment.
    pub scope: GrantScope,
}

impl EffectiveGrant {
    /// Creates an effective grant.
    pub fn new(role: RoleId, permission: Permission, scope: GrantScope) -> Self {
        Self {
            role,
            permission,
            scope,
        }
    }
}

/// Cache interface for effective grants.
#[async_trait]
pub trait Cache: Send + Sync {
    /// Gets cached grants for a tenant principal under a config signature.
    async fn get_effective_grants(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
        config_signature: &str,
    ) -> Option<Vec<EffectiveGrant>>;

    /// Sets cached grants for a tenant principal under a config signature.
    async fn set_effective_grants(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
        config_signature: &str,
        grants: Vec<EffectiveGrant>,
    );

    /// Invalidates cache for a principal.
    async fn invalidate_principal(&self, tenant: &TenantId, principal: &PrincipalId);

    /// Invalidates cache for a role.
    async fn invalidate_role(&self, tenant: &TenantId, role: &RoleId);

    /// Invalidates cache for a tenant.
    async fn invalidate_tenant(&self, tenant: &TenantId);

    /// Invalidates all cached grants.
    async fn invalidate_all(&self);
}

/// No-op cache implementation.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoCache;

#[async_trait]
impl Cache for NoCache {
    async fn get_effective_grants(
        &self,
        _tenant: &TenantId,
        _principal: &PrincipalId,
        _config_signature: &str,
    ) -> Option<Vec<EffectiveGrant>> {
        None
    }

    async fn set_effective_grants(
        &self,
        _tenant: &TenantId,
        _principal: &PrincipalId,
        _config_signature: &str,
        _grants: Vec<EffectiveGrant>,
    ) {
    }

    async fn invalidate_principal(&self, _tenant: &TenantId, _principal: &PrincipalId) {}

    async fn invalidate_role(&self, _tenant: &TenantId, _role: &RoleId) {}

    async fn invalidate_tenant(&self, _tenant: &TenantId) {}

    async fn invalidate_all(&self) {}
}

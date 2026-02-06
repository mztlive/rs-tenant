use crate::error::StoreError;
use crate::permission::Permission;
use crate::types::{GlobalRoleId, PrincipalId, RoleId, TenantId};
use async_trait::async_trait;

/// Store interface for tenant and principal activation.
#[async_trait]
pub trait TenantStore {
    /// Returns whether a tenant is active.
    async fn tenant_active(&self, tenant: TenantId) -> std::result::Result<bool, StoreError>;

    /// Returns whether a tenant is active using borrowed identifiers.
    ///
    /// Default implementation clones and delegates to [`tenant_active`].
    async fn tenant_active_ref(&self, tenant: &TenantId) -> std::result::Result<bool, StoreError> {
        self.tenant_active(tenant.clone()).await
    }

    /// Returns whether a principal is active within a tenant.
    async fn principal_active(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
    ) -> std::result::Result<bool, StoreError>;

    /// Returns whether a principal is active using borrowed identifiers.
    ///
    /// Default implementation clones and delegates to [`principal_active`].
    async fn principal_active_ref(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> std::result::Result<bool, StoreError> {
        self.principal_active(tenant.clone(), principal.clone())
            .await
    }
}

/// Store interface for tenant-scoped roles.
#[async_trait]
pub trait RoleStore {
    /// Returns roles assigned to a principal within a tenant.
    async fn principal_roles(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
    ) -> std::result::Result<Vec<RoleId>, StoreError>;

    /// Returns roles assigned to a principal using borrowed identifiers.
    ///
    /// Default implementation clones and delegates to [`principal_roles`].
    async fn principal_roles_ref(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> std::result::Result<Vec<RoleId>, StoreError> {
        self.principal_roles(tenant.clone(), principal.clone())
            .await
    }

    /// Returns permissions bound to a role within a tenant.
    async fn role_permissions(
        &self,
        tenant: TenantId,
        role: RoleId,
    ) -> std::result::Result<Vec<Permission>, StoreError>;

    /// Returns permissions bound to a role using borrowed identifiers.
    ///
    /// Default implementation clones and delegates to [`role_permissions`].
    async fn role_permissions_ref(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> std::result::Result<Vec<Permission>, StoreError> {
        self.role_permissions(tenant.clone(), role.clone()).await
    }

    /// Returns direct parent roles for inheritance traversal.
    async fn role_inherits(
        &self,
        tenant: TenantId,
        role: RoleId,
    ) -> std::result::Result<Vec<RoleId>, StoreError>;

    /// Returns parent roles using borrowed identifiers.
    ///
    /// Default implementation clones and delegates to [`role_inherits`].
    async fn role_inherits_ref(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> std::result::Result<Vec<RoleId>, StoreError> {
        self.role_inherits(tenant.clone(), role.clone()).await
    }
}

/// Store interface for global roles.
#[async_trait]
pub trait GlobalRoleStore {
    /// Returns global roles assigned to a principal.
    async fn global_roles(
        &self,
        principal: PrincipalId,
    ) -> std::result::Result<Vec<GlobalRoleId>, StoreError>;

    /// Returns global roles using borrowed identifiers.
    ///
    /// Default implementation clones and delegates to [`global_roles`].
    async fn global_roles_ref(
        &self,
        principal: &PrincipalId,
    ) -> std::result::Result<Vec<GlobalRoleId>, StoreError> {
        self.global_roles(principal.clone()).await
    }

    /// Returns permissions bound to a global role.
    async fn global_role_permissions(
        &self,
        role: GlobalRoleId,
    ) -> std::result::Result<Vec<Permission>, StoreError>;

    /// Returns global role permissions using borrowed identifiers.
    ///
    /// Default implementation clones and delegates to [`global_role_permissions`].
    async fn global_role_permissions_ref(
        &self,
        role: &GlobalRoleId,
    ) -> std::result::Result<Vec<Permission>, StoreError> {
        self.global_role_permissions(role.clone()).await
    }

    /// Returns whether a principal is a platform-level super administrator.
    ///
    /// Default implementation returns `false`. Override this for systems that
    /// need a direct super-admin capability without role expansion.
    async fn is_super_admin(
        &self,
        _principal: PrincipalId,
    ) -> std::result::Result<bool, StoreError> {
        Ok(false)
    }

    /// Returns whether a principal is a platform-level super administrator.
    ///
    /// Default implementation clones and delegates to [`is_super_admin`].
    async fn is_super_admin_ref(
        &self,
        principal: &PrincipalId,
    ) -> std::result::Result<bool, StoreError> {
        self.is_super_admin(principal.clone()).await
    }
}

/// Composite store trait.
pub trait Store: TenantStore + RoleStore + GlobalRoleStore + Send + Sync {}

impl<T> Store for T where T: TenantStore + RoleStore + GlobalRoleStore + Send + Sync {}

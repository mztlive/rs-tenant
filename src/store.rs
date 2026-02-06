use crate::error::StoreError;
use crate::permission::Permission;
use crate::types::{GlobalRoleId, PrincipalId, RoleId, TenantId};
use async_trait::async_trait;

/// Store interface for tenant and principal activation.
#[async_trait]
pub trait TenantStore {
    /// Returns whether a tenant is active.
    async fn tenant_active(&self, tenant: TenantId) -> std::result::Result<bool, StoreError>;

    /// Returns whether a principal is active within a tenant.
    async fn principal_active(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
    ) -> std::result::Result<bool, StoreError>;
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

    /// Returns permissions bound to a role within a tenant.
    async fn role_permissions(
        &self,
        tenant: TenantId,
        role: RoleId,
    ) -> std::result::Result<Vec<Permission>, StoreError>;

    /// Returns direct parent roles for inheritance traversal.
    async fn role_inherits(
        &self,
        tenant: TenantId,
        role: RoleId,
    ) -> std::result::Result<Vec<RoleId>, StoreError>;
}

/// Store interface for global roles.
#[async_trait]
pub trait GlobalRoleStore {
    /// Returns global roles assigned to a principal.
    async fn global_roles(
        &self,
        principal: PrincipalId,
    ) -> std::result::Result<Vec<GlobalRoleId>, StoreError>;

    /// Returns permissions bound to a global role.
    async fn global_role_permissions(
        &self,
        role: GlobalRoleId,
    ) -> std::result::Result<Vec<Permission>, StoreError>;
}

/// Composite store trait.
pub trait Store: TenantStore + RoleStore + GlobalRoleStore + Send + Sync {}

impl<T> Store for T where T: TenantStore + RoleStore + GlobalRoleStore + Send + Sync {}

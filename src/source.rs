use crate::error::SourceError;
use crate::ids::{RoleId, TenantId};
use crate::permission::Permission;
use crate::request::AuthSubject;
use crate::role::RoleAssignment;
use async_trait::async_trait;

/// Tenant status used by authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TenantStatus {
    /// Tenant can participate in authorization.
    Active,
    /// Tenant is missing, disabled, or otherwise not usable.
    Inactive,
}

/// Principal membership status within a tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MembershipStatus {
    /// Principal can participate in tenant authorization.
    Active,
    /// Principal is missing, disabled, or otherwise not usable.
    Inactive,
}

/// Read-only authorization data source.
#[async_trait]
pub trait AuthorizationSource: Send + Sync {
    /// Returns tenant status.
    async fn tenant_status(
        &self,
        tenant: &TenantId,
    ) -> std::result::Result<TenantStatus, SourceError>;

    /// Returns principal membership status.
    async fn membership_status(
        &self,
        subject: &AuthSubject,
    ) -> std::result::Result<MembershipStatus, SourceError>;

    /// Returns scoped role assignments for the subject.
    async fn role_assignments(
        &self,
        subject: &AuthSubject,
    ) -> std::result::Result<Vec<RoleAssignment>, SourceError>;

    /// Returns permissions bound to a tenant role.
    async fn role_permissions(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> std::result::Result<Vec<Permission>, SourceError>;

    /// Returns direct parent roles for role inheritance.
    async fn parent_roles(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> std::result::Result<Vec<RoleId>, SourceError>;
}

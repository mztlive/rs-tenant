use super::{PlatformPrincipalStatus, PlatformRoleAssignment, PlatformRoleId, PlatformSubject};
use crate::{Permission, SourceError};
use async_trait::async_trait;

/// Read-only source for platform authorization data.
#[async_trait]
pub trait PlatformAuthorizationSource: Send + Sync {
    /// Returns the status of a platform principal.
    async fn platform_principal_status(
        &self,
        subject: &PlatformSubject,
    ) -> std::result::Result<PlatformPrincipalStatus, SourceError>;

    /// Returns direct platform role assignments for a platform principal.
    async fn platform_role_assignments(
        &self,
        subject: &PlatformSubject,
    ) -> std::result::Result<Vec<PlatformRoleAssignment>, SourceError>;

    /// Returns permissions granted by a platform role.
    async fn platform_role_permissions(
        &self,
        role: &PlatformRoleId,
    ) -> std::result::Result<Vec<Permission>, SourceError>;

    /// Returns direct parent roles for a platform role.
    async fn platform_parent_roles(
        &self,
        role: &PlatformRoleId,
    ) -> std::result::Result<Vec<PlatformRoleId>, SourceError>;
}

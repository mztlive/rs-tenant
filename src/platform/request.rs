use super::PlatformSubject;
use crate::{Permission, ScopePath, TenantId};

/// Platform-owned resource access request.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlatformAccessRequest {
    /// Platform subject.
    pub subject: PlatformSubject,
    /// Full permission, including action.
    pub permission: Permission,
}

/// Query for a platform principal's accessible tenant data scope.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TenantDataScopeQuery {
    /// Platform subject.
    pub subject: PlatformSubject,
    /// Full permission, including action.
    pub permission: Permission,
}

/// Tenant-level data access request for a platform principal.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TenantDataAccessRequest {
    /// Platform subject.
    pub subject: PlatformSubject,
    /// Full permission, including action.
    pub permission: Permission,
    /// Target tenant.
    pub tenant: TenantId,
}

/// Tenant path data access request for a platform principal.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TenantScopedDataAccessRequest {
    /// Platform subject.
    pub subject: PlatformSubject,
    /// Full permission, including action.
    pub permission: Permission,
    /// Target tenant.
    pub tenant: TenantId,
    /// Target path within the tenant.
    pub target: ScopePath,
}

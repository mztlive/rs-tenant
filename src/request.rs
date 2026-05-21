use crate::ids::{PrincipalId, TenantId};
use crate::permission::Permission;
use crate::scope::ScopePath;

/// Tenant-scoped subject for authorization.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AuthSubject {
    /// Tenant identifier.
    pub tenant: TenantId,
    /// Principal identifier.
    pub principal: PrincipalId,
}

impl AuthSubject {
    /// Creates an authorization subject.
    pub fn new(tenant: TenantId, principal: PrincipalId) -> Self {
        Self { tenant, principal }
    }
}

/// Query for a permission's accessible data scope.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScopeQuery {
    /// Tenant-scoped subject.
    pub subject: AuthSubject,
    /// Full permission, including action.
    pub permission: Permission,
}

/// Tenant-level access request.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TenantAccessRequest {
    /// Tenant-scoped subject.
    pub subject: AuthSubject,
    /// Full permission, including action.
    pub permission: Permission,
}

/// Path-scoped access request.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScopedAccessRequest {
    /// Tenant-scoped subject.
    pub subject: AuthSubject,
    /// Full permission, including action.
    pub permission: Permission,
    /// Target path being accessed.
    pub target: ScopePath,
}

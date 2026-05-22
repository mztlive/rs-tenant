use super::{PlatformGrantScope, PlatformRoleId};

/// A platform role assignment together with its explicit grant scope.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlatformRoleAssignment {
    /// Assigned platform role.
    pub role: PlatformRoleId,
    /// Scope granted by this platform assignment.
    pub scope: PlatformGrantScope,
}

impl PlatformRoleAssignment {
    /// Creates a platform role assignment.
    pub fn new(role: PlatformRoleId, scope: PlatformGrantScope) -> Self {
        Self { role, scope }
    }
}

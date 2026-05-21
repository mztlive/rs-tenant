use crate::ids::RoleId;
use crate::scope::GrantScope;

/// A role assignment together with its explicit grant scope.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoleAssignment {
    /// Assigned role.
    pub role: RoleId,
    /// Scope granted by this assignment.
    pub scope: GrantScope,
}

impl RoleAssignment {
    /// Creates a role assignment.
    pub fn new(role: RoleId, scope: GrantScope) -> Self {
        Self { role, scope }
    }
}

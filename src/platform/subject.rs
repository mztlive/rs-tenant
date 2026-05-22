use super::PlatformPrincipalId;

/// Platform principal status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PlatformPrincipalStatus {
    /// Principal may receive platform grants.
    Active,
    /// Principal is missing or disabled.
    Inactive,
}

/// Platform-scoped subject for authorization.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlatformSubject {
    /// Platform principal identifier.
    pub principal: PlatformPrincipalId,
}

impl PlatformSubject {
    /// Creates a platform subject.
    pub fn new(principal: PlatformPrincipalId) -> Self {
        Self { principal }
    }
}

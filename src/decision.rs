use crate::scope::AccessScope;

/// Final allow/deny decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDecision {
    /// Access is allowed.
    Allow,
    /// Access is denied.
    Deny,
}

/// High-level deny reason for explanation and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    /// Tenant is inactive or missing.
    TenantInactive,
    /// Principal membership is inactive or missing.
    PrincipalInactive,
    /// No matching permission grant exists.
    PermissionMissing,
    /// Tenant-level API was called with only path-scoped grants.
    TargetScopeRequired,
    /// Target path is outside the accessible roots.
    ScopeDenied,
}

/// Lightweight explanation for an authorization decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessExplanation {
    /// Final decision.
    pub decision: AccessDecision,
    /// Deny reason when decision is [`AccessDecision::Deny`].
    pub reason: Option<DenyReason>,
    /// Effective scope computed during the check.
    pub scope: AccessScope,
}

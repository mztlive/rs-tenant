use crate::ids::{RoleId, TenantId};
use thiserror::Error;

/// Authorization source error type.
pub type SourceError = Box<dyn std::error::Error + Send + Sync>;

/// Crate result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by this crate.
#[derive(Debug, Error)]
pub enum Error {
    /// Authorization source error wrapper.
    #[error("source error: {0}")]
    Source(#[source] SourceError),
    /// Invalid identifier input.
    #[error("invalid id: {0}")]
    InvalidId(String),
    /// Invalid permission input.
    #[error("invalid permission: {0}")]
    InvalidPermission(String),
    /// Invalid scope input.
    #[error("invalid scope: {0}")]
    InvalidScope(String),
    /// Role inheritance cycle detected.
    #[error("role cycle detected for tenant {tenant} at role {role}")]
    RoleCycleDetected { tenant: TenantId, role: RoleId },
    /// Role inheritance depth exceeded.
    #[error(
        "role inheritance depth exceeded for tenant {tenant} at role {role}; max depth {max_depth}"
    )]
    RoleDepthExceeded {
        tenant: TenantId,
        role: RoleId,
        max_depth: usize,
    },
}

impl From<SourceError> for Error {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

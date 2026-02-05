use crate::types::{RoleId, TenantId};
use thiserror::Error;

/// Store-layer error type.
pub type StoreError = Box<dyn std::error::Error + Send + Sync>;

/// Crate result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by this crate.
#[derive(Debug, Error)]
pub enum Error {
    /// Store error wrapper.
    #[error("store error: {0}")]
    Store(#[source] StoreError),
    /// Invalid identifier input.
    #[error("invalid id: {0}")]
    InvalidId(String),
    /// Invalid permission input.
    #[error("invalid permission: {0}")]
    InvalidPermission(String),
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

impl From<StoreError> for Error {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

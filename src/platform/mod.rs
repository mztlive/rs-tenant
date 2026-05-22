//! Platform authorization domain.
//!
//! This module is a sibling of the tenant-scoped [`crate::Engine`]. It models
//! platform principals, platform roles, platform-owned permissions, and the
//! tenant data ranges a platform principal may manage.

mod engine;
mod ids;
#[cfg(feature = "memory-store")]
mod memory_source;
mod request;
mod role;
mod scope;
mod source;
mod subject;

pub use self::engine::{PlatformEngine, PlatformEngineBuilder, PlatformEngineConfig};
pub use self::ids::{PlatformPrincipalId, PlatformRoleId};
#[cfg(feature = "memory-store")]
pub use self::memory_source::MemoryPlatformSource;
pub use self::request::{
    PlatformAccessRequest, TenantDataAccessRequest, TenantDataScopeQuery,
    TenantScopedDataAccessRequest,
};
pub use self::role::PlatformRoleAssignment;
pub use self::scope::{
    PlatformGrantScope, TenantDataAccessScope, TenantScopeRoots, TenantScopedRoots, TenantSet,
};
pub use self::source::PlatformAuthorizationSource;
pub use self::subject::{PlatformPrincipalStatus, PlatformSubject};

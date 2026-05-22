//! 平台级授权领域。
//!
//! 本模块是租户级 [`crate::Engine`] 的同级模块，用于建模平台主体、平台角色、平台自有权限，
//! 以及平台主体可以管理的租户数据范围。

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

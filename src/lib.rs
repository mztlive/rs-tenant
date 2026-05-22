//! Tenant-scoped RBAC authorization kernel.
//!
//! This crate answers one core question: given a tenant, a principal, and a
//! permission, what access scope is granted by tenant role assignments?
//!
//! v0.4 keeps the tenant-scoped core API as [`Engine::accessible_scope`],
//! [`Engine::can_access_scope`], and [`Engine::can_tenant`].
//! Platform authorization is available behind the `platform` feature through
//! the sibling [`platform`] module.
#![forbid(unsafe_code)]

mod cache;
mod decision;
mod engine;
mod error;
mod ids;
#[cfg(feature = "memory-cache")]
mod memory_cache;
#[cfg(feature = "memory-store")]
mod memory_source;
mod permission;
#[cfg(feature = "platform")]
pub mod platform;
mod request;
mod role;
mod scope;
mod source;

#[cfg(feature = "axum")]
pub mod axum;

pub use crate::cache::{Cache, EffectiveGrant, NoCache};
pub use crate::decision::{AccessDecision, AccessExplanation, DenyReason};
pub use crate::engine::{Engine, EngineBuilder, EngineConfig};
pub use crate::error::{Error, Result, SourceError};
pub use crate::ids::{PrincipalId, RoleId, TenantId};
pub use crate::permission::{Action, Permission, Resource};
pub use crate::request::{AuthSubject, ScopeQuery, ScopedAccessRequest, TenantAccessRequest};
pub use crate::role::RoleAssignment;
pub use crate::scope::{AccessScope, GrantScope, ScopePath, ScopeRoots};
pub use crate::source::{AuthorizationSource, MembershipStatus, TenantStatus};

#[cfg(feature = "memory-store")]
pub use crate::memory_source::MemorySource;

#[cfg(feature = "memory-cache")]
pub use crate::memory_cache::MemoryCache;

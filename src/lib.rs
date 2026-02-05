//! Multi-tenant RBAC authorization library.
//!
//! This crate provides strong-typed identifiers, permission parsing and matching,
//! and a pluggable async store interface. The default behavior is deny-by-default.
//! Use [`Engine`] for authorization and [`Scope`] for resource scoping.
//!
//! # Examples
//!
//! Basic authorization flow using the in-memory store (enable `memory-store`):
//! ```no_run
//! use rs_tenant::{EngineBuilder, Permission, PrincipalId, TenantId};
//! # #[cfg(feature = "memory-store")]
//! # {
//! use rs_tenant::MemoryStore;
//! let store = MemoryStore::new();
//! let engine = EngineBuilder::new(store).build();
//! let tenant = TenantId::try_from("tenant_1").unwrap();
//! let principal = PrincipalId::try_from("user_1").unwrap();
//! let permission = Permission::try_from("invoice:read").unwrap();
//! let _ = engine.authorize(tenant, principal, permission);
//! # }
//! ```
//!
//! Creating a process-local cache (enable `memory-cache`):
//! ```no_run
//! # #[cfg(feature = "memory-cache")]
//! # {
//! use rs_tenant::MemoryCache;
//! use std::time::Duration;
//! let cache = MemoryCache::new(1024).with_ttl(Duration::from_secs(30));
//! # let _ = cache;
//! # }
//! ```
#![forbid(unsafe_code)]

mod cache;
mod engine;
mod error;
mod permission;
mod store;
mod types;
#[cfg(feature = "memory-cache")]
mod memory_cache;

#[cfg(feature = "memory-store")]
mod memory_store;

#[cfg(feature = "axum")]
pub mod axum;

pub use crate::cache::{Cache, NoCache};
pub use crate::engine::{Decision, Engine, EngineBuilder, Scope};
pub use crate::error::{Error, Result, StoreError};
pub use crate::permission::{DefaultPermissionValidator, Permission, PermissionValidator};
pub use crate::store::{GlobalRoleStore, RoleStore, Store, TenantStore};
pub use crate::types::{GlobalRoleId, PrincipalId, ResourceName, RoleId, TenantId};

#[cfg(feature = "memory-store")]
pub use crate::memory_store::MemoryStore;

#[cfg(feature = "memory-cache")]
pub use crate::memory_cache::MemoryCache;

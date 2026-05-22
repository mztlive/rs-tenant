//! 租户级 RBAC 授权内核。
//!
//! 这个 crate 回答一个核心问题：给定租户、主体和权限后，租户角色分配到底授予了什么访问范围。
//!
//! v0.4 保留租户级核心 API：[`Engine::accessible_scope`]、[`Engine::can_access_scope`]
//! 和 [`Engine::can_tenant`]。
//! 平台级授权通过 `platform` feature 下的同级 [`platform`] 模块提供。
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

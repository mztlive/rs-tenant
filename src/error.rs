use crate::ids::{RoleId, TenantId};
#[cfg(feature = "platform")]
use crate::platform::PlatformRoleId;
use thiserror::Error;

/// 授权数据源错误类型。
pub type SourceError = Box<dyn std::error::Error + Send + Sync>;

/// crate 内统一使用的结果类型。
pub type Result<T> = std::result::Result<T, Error>;

/// 这个 crate 返回的错误。
#[derive(Debug, Error)]
pub enum Error {
    /// 授权数据源错误包装。
    #[error("source error: {0}")]
    Source(#[source] SourceError),
    /// 标识符输入非法。
    #[error("invalid id: {0}")]
    InvalidId(String),
    /// 权限输入非法。
    #[error("invalid permission: {0}")]
    InvalidPermission(String),
    /// 范围输入非法。
    #[error("invalid scope: {0}")]
    InvalidScope(String),
    /// 检测到角色继承环。
    #[error("role cycle detected for tenant {tenant} at role {role}")]
    RoleCycleDetected { tenant: TenantId, role: RoleId },
    /// 角色继承深度超过限制。
    #[error(
        "role inheritance depth exceeded for tenant {tenant} at role {role}; max depth {max_depth}"
    )]
    RoleDepthExceeded {
        tenant: TenantId,
        role: RoleId,
        max_depth: usize,
    },
    /// 检测到平台角色继承环。
    #[cfg(feature = "platform")]
    #[error("platform role cycle detected at role {role}")]
    PlatformRoleCycleDetected {
        /// 检测到环的角色。
        role: PlatformRoleId,
    },
    /// 平台角色继承深度超过限制。
    #[cfg(feature = "platform")]
    #[error("platform role inheritance depth exceeded at role {role}; max depth {max_depth}")]
    PlatformRoleDepthExceeded {
        /// 超过深度限制的角色。
        role: PlatformRoleId,
        /// 配置的最大继承深度。
        max_depth: usize,
    },
}

impl From<SourceError> for Error {
    /// 将授权数据源错误包装为 crate 错误。
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

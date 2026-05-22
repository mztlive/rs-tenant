use super::PlatformSubject;
use crate::{Permission, ScopePath, TenantId};

/// 平台自有资源访问请求。
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlatformAccessRequest {
    /// 平台主体。
    pub subject: PlatformSubject,
    /// 包含动作的完整权限。
    pub permission: Permission,
}

/// 查询平台主体可访问的租户数据范围。
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TenantDataScopeQuery {
    /// 平台主体。
    pub subject: PlatformSubject,
    /// 包含动作的完整权限。
    pub permission: Permission,
}

/// 平台主体的租户级数据访问请求。
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TenantDataAccessRequest {
    /// 平台主体。
    pub subject: PlatformSubject,
    /// 包含动作的完整权限。
    pub permission: Permission,
    /// 目标租户。
    pub tenant: TenantId,
}

/// 平台主体的租户路径数据访问请求。
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TenantScopedDataAccessRequest {
    /// 平台主体。
    pub subject: PlatformSubject,
    /// 包含动作的完整权限。
    pub permission: Permission,
    /// 目标租户。
    pub tenant: TenantId,
    /// 租户内的目标路径。
    pub target: ScopePath,
}

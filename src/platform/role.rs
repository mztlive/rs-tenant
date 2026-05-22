use super::{PlatformGrantScope, PlatformRoleId};

/// 平台角色分配及其显式授权范围。
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlatformRoleAssignment {
    /// 被分配的平台角色。
    pub role: PlatformRoleId,
    /// 该平台分配授予的范围。
    pub scope: PlatformGrantScope,
}

impl PlatformRoleAssignment {
    /// 创建平台角色分配。
    pub fn new(role: PlatformRoleId, scope: PlatformGrantScope) -> Self {
        Self { role, scope }
    }
}

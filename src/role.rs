use crate::ids::RoleId;
use crate::scope::GrantScope;

/// 角色分配及其显式授权范围。
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoleAssignment {
    /// 被分配的角色。
    pub role: RoleId,
    /// 该分配授予的范围。
    pub scope: GrantScope,
}

impl RoleAssignment {
    /// 创建角色分配。
    pub fn new(role: RoleId, scope: GrantScope) -> Self {
        Self { role, scope }
    }
}

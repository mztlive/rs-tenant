use crate::ids::{PrincipalId, TenantId};
use crate::permission::Permission;
use crate::scope::ScopePath;

/// 用于授权的租户级主体。
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AuthSubject {
    /// 租户标识符。
    pub tenant: TenantId,
    /// 主体标识符。
    pub principal: PrincipalId,
}

impl AuthSubject {
    /// 创建授权主体。
    pub fn new(tenant: TenantId, principal: PrincipalId) -> Self {
        Self { tenant, principal }
    }
}

/// 查询某个权限可访问的数据范围。
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScopeQuery {
    /// 租户级主体。
    pub subject: AuthSubject,
    /// 包含动作的完整权限。
    pub permission: Permission,
}

/// 租户级访问请求。
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TenantAccessRequest {
    /// 租户级主体。
    pub subject: AuthSubject,
    /// 包含动作的完整权限。
    pub permission: Permission,
}

/// 路径级访问请求。
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScopedAccessRequest {
    /// 租户级主体。
    pub subject: AuthSubject,
    /// 包含动作的完整权限。
    pub permission: Permission,
    /// 正在访问的目标路径。
    pub target: ScopePath,
}

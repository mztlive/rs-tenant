use crate::error::SourceError;
use crate::ids::{RoleId, TenantId};
use crate::permission::Permission;
use crate::request::AuthSubject;
use crate::role::RoleAssignment;
use async_trait::async_trait;

/// 授权使用的租户状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TenantStatus {
    /// 租户可以参与授权。
    Active,
    /// 租户不存在、被禁用或不可用。
    Inactive,
}

/// 主体在租户内的成员关系状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MembershipStatus {
    /// 主体可以参与租户授权。
    Active,
    /// 主体不存在、被禁用或不可用。
    Inactive,
}

/// 只读授权数据源。
#[async_trait]
pub trait AuthorizationSource: Send + Sync {
    /// 返回租户状态。
    async fn tenant_status(
        &self,
        tenant: &TenantId,
    ) -> std::result::Result<TenantStatus, SourceError>;

    /// 返回主体成员关系状态。
    async fn membership_status(
        &self,
        subject: &AuthSubject,
    ) -> std::result::Result<MembershipStatus, SourceError>;

    /// 返回主体的带范围角色分配。
    async fn role_assignments(
        &self,
        subject: &AuthSubject,
    ) -> std::result::Result<Vec<RoleAssignment>, SourceError>;

    /// 返回绑定到租户角色的权限。
    async fn role_permissions(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> std::result::Result<Vec<Permission>, SourceError>;

    /// 返回用于角色继承的直接父角色。
    async fn parent_roles(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> std::result::Result<Vec<RoleId>, SourceError>;
}

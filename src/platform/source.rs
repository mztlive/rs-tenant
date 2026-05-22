use super::{PlatformPrincipalStatus, PlatformRoleAssignment, PlatformRoleId, PlatformSubject};
use crate::{Permission, SourceError};
use async_trait::async_trait;

/// 平台授权数据的只读数据源。
#[async_trait]
pub trait PlatformAuthorizationSource: Send + Sync {
    /// 返回平台主体状态。
    async fn platform_principal_status(
        &self,
        subject: &PlatformSubject,
    ) -> std::result::Result<PlatformPrincipalStatus, SourceError>;

    /// 返回平台主体的直接平台角色分配。
    async fn platform_role_assignments(
        &self,
        subject: &PlatformSubject,
    ) -> std::result::Result<Vec<PlatformRoleAssignment>, SourceError>;

    /// 返回平台角色授予的权限。
    async fn platform_role_permissions(
        &self,
        role: &PlatformRoleId,
    ) -> std::result::Result<Vec<Permission>, SourceError>;

    /// 返回平台角色的直接父角色。
    async fn platform_parent_roles(
        &self,
        role: &PlatformRoleId,
    ) -> std::result::Result<Vec<PlatformRoleId>, SourceError>;
}

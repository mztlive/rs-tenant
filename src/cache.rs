use crate::ids::{PrincipalId, RoleId, TenantId};
use crate::permission::Permission;
use crate::scope::GrantScope;
use async_trait::async_trait;

/// 按租户主体和引擎配置缓存的内部有效授权。
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveGrant {
    /// 读取到权限的来源角色。
    pub role: RoleId,
    /// 角色授予的权限。
    pub permission: Permission,
    /// 原始角色分配附带的范围。
    pub scope: GrantScope,
}

impl EffectiveGrant {
    /// 创建有效授权。
    pub fn new(role: RoleId, permission: Permission, scope: GrantScope) -> Self {
        Self {
            role,
            permission,
            scope,
        }
    }
}

/// 有效授权的缓存接口。
#[async_trait]
pub trait Cache: Send + Sync {
    /// 按配置签名获取租户主体的缓存授权。
    async fn get_effective_grants(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
        config_signature: &str,
    ) -> Option<Vec<EffectiveGrant>>;

    /// 按配置签名写入租户主体的缓存授权。
    async fn set_effective_grants(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
        config_signature: &str,
        grants: Vec<EffectiveGrant>,
    );

    /// 失效某个主体的缓存。
    async fn invalidate_principal(&self, tenant: &TenantId, principal: &PrincipalId);

    /// 失效某个角色相关的缓存。
    async fn invalidate_role(&self, tenant: &TenantId, role: &RoleId);

    /// 失效某个租户的缓存。
    async fn invalidate_tenant(&self, tenant: &TenantId);

    /// 失效所有缓存授权。
    async fn invalidate_all(&self);
}

/// 空操作缓存实现。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoCache;

#[async_trait]
impl Cache for NoCache {
    /// 始终返回缓存未命中。
    async fn get_effective_grants(
        &self,
        _tenant: &TenantId,
        _principal: &PrincipalId,
        _config_signature: &str,
    ) -> Option<Vec<EffectiveGrant>> {
        None
    }

    /// 忽略缓存写入。
    async fn set_effective_grants(
        &self,
        _tenant: &TenantId,
        _principal: &PrincipalId,
        _config_signature: &str,
        _grants: Vec<EffectiveGrant>,
    ) {
    }

    /// 忽略主体级缓存失效。
    async fn invalidate_principal(&self, _tenant: &TenantId, _principal: &PrincipalId) {}

    /// 忽略角色级缓存失效。
    async fn invalidate_role(&self, _tenant: &TenantId, _role: &RoleId) {}

    /// 忽略租户级缓存失效。
    async fn invalidate_tenant(&self, _tenant: &TenantId) {}

    /// 忽略全量缓存失效。
    async fn invalidate_all(&self) {}
}

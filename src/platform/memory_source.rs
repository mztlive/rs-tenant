use super::{
    PlatformAuthorizationSource, PlatformGrantScope, PlatformPrincipalId, PlatformPrincipalStatus,
    PlatformRoleAssignment, PlatformRoleId, PlatformSubject,
};
use crate::{Permission, SourceError};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// 用于测试和演示的内存平台授权数据源。
#[derive(Debug, Default, Clone)]
pub struct MemoryPlatformSource {
    inner: Arc<Inner>,
}

/// 内存平台数据源的共享内部状态。
#[derive(Debug, Default)]
struct Inner {
    principals: RwLock<HashMap<PlatformPrincipalId, PlatformPrincipalStatus>>,
    assignments: RwLock<HashMap<PlatformPrincipalId, Vec<PlatformRoleAssignment>>>,
    role_permissions: RwLock<HashMap<PlatformRoleId, HashSet<Permission>>>,
    parent_roles: RwLock<HashMap<PlatformRoleId, HashSet<PlatformRoleId>>>,
}

/// 获取读锁，并在锁中毒时恢复内部值。
fn read_guard<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// 获取写锁，并在锁中毒时恢复内部值。
fn write_guard<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl MemoryPlatformSource {
    /// 创建空平台数据源。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置平台主体状态。
    pub fn set_principal_status(
        &self,
        principal: PlatformPrincipalId,
        status: PlatformPrincipalStatus,
    ) {
        write_guard(&self.inner.principals).insert(principal, status);
    }

    /// 添加平台角色分配。
    pub fn add_role_assignment(
        &self,
        principal: PlatformPrincipalId,
        role: PlatformRoleId,
        scope: PlatformGrantScope,
    ) {
        write_guard(&self.inner.assignments)
            .entry(principal)
            .or_default()
            .push(PlatformRoleAssignment::new(role, scope));
    }

    /// 为平台角色添加权限。
    pub fn add_role_permission(&self, role: PlatformRoleId, permission: Permission) {
        write_guard(&self.inner.role_permissions)
            .entry(role)
            .or_default()
            .insert(permission);
    }

    /// 添加直接父平台角色。
    pub fn add_parent_role(&self, role: PlatformRoleId, parent: PlatformRoleId) {
        write_guard(&self.inner.parent_roles)
            .entry(role)
            .or_default()
            .insert(parent);
    }
}

#[async_trait]
impl PlatformAuthorizationSource for MemoryPlatformSource {
    /// 查询平台主体状态，未配置时默认为未激活。
    async fn platform_principal_status(
        &self,
        subject: &PlatformSubject,
    ) -> std::result::Result<PlatformPrincipalStatus, SourceError> {
        Ok(read_guard(&self.inner.principals)
            .get(&subject.principal)
            .copied()
            .unwrap_or(PlatformPrincipalStatus::Inactive))
    }

    /// 查询平台主体的角色分配。
    async fn platform_role_assignments(
        &self,
        subject: &PlatformSubject,
    ) -> std::result::Result<Vec<PlatformRoleAssignment>, SourceError> {
        Ok(read_guard(&self.inner.assignments)
            .get(&subject.principal)
            .cloned()
            .unwrap_or_default())
    }

    /// 查询平台角色拥有的权限集合。
    async fn platform_role_permissions(
        &self,
        role: &PlatformRoleId,
    ) -> std::result::Result<Vec<Permission>, SourceError> {
        Ok(read_guard(&self.inner.role_permissions)
            .get(role)
            .map(|permissions| permissions.iter().cloned().collect())
            .unwrap_or_default())
    }

    /// 查询平台角色的直接父角色集合。
    async fn platform_parent_roles(
        &self,
        role: &PlatformRoleId,
    ) -> std::result::Result<Vec<PlatformRoleId>, SourceError> {
        Ok(read_guard(&self.inner.parent_roles)
            .get(role)
            .map(|parents| parents.iter().cloned().collect())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn memory_platform_source_should_default_to_inactive() {
        let source = MemoryPlatformSource::new();
        let subject =
            PlatformSubject::new(PlatformPrincipalId::parse("platform_admin").expect("principal"));
        let status = block_on(source.platform_principal_status(&subject)).expect("status");

        assert_eq!(status, PlatformPrincipalStatus::Inactive);
    }

    #[test]
    fn memory_platform_source_should_return_configured_authorization_data() {
        let source = MemoryPlatformSource::new();
        let principal = PlatformPrincipalId::parse("platform_admin").expect("principal");
        let role = PlatformRoleId::parse("support").expect("role");
        let parent = PlatformRoleId::parse("support_parent").expect("role");
        let permission = Permission::parse("platform/role:read").expect("permission");
        let subject = PlatformSubject::new(principal.clone());

        source.set_principal_status(principal.clone(), PlatformPrincipalStatus::Active);
        source.add_role_assignment(principal, role.clone(), PlatformGrantScope::platform());
        source.add_role_permission(role.clone(), permission.clone());
        source.add_parent_role(role.clone(), parent.clone());

        assert_eq!(
            block_on(source.platform_principal_status(&subject)).expect("status"),
            PlatformPrincipalStatus::Active
        );
        assert_eq!(
            block_on(source.platform_role_assignments(&subject)).expect("assignments"),
            vec![PlatformRoleAssignment::new(
                role.clone(),
                PlatformGrantScope::platform()
            )]
        );
        assert_eq!(
            block_on(source.platform_role_permissions(&role)).expect("permissions"),
            vec![permission]
        );
        assert_eq!(
            block_on(source.platform_parent_roles(&role)).expect("parents"),
            vec![parent]
        );
    }

    #[test]
    fn memory_platform_source_should_recover_from_poisoned_lock() {
        let source = MemoryPlatformSource::new();
        let inner = source.inner.clone();
        let _ = std::thread::spawn(move || {
            let _guard = inner.principals.write().unwrap();
            panic!("poison principals lock");
        })
        .join();

        let principal = PlatformPrincipalId::parse("platform_admin").expect("principal");
        source.set_principal_status(principal.clone(), PlatformPrincipalStatus::Active);
        let subject = PlatformSubject::new(principal);
        let status = block_on(source.platform_principal_status(&subject)).expect("status");

        assert_eq!(status, PlatformPrincipalStatus::Active);
    }
}

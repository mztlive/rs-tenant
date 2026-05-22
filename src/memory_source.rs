use crate::ids::{PrincipalId, RoleId, TenantId};
use crate::permission::Permission;
use crate::request::AuthSubject;
use crate::role::RoleAssignment;
use crate::scope::GrantScope;
use crate::source::{AuthorizationSource, MembershipStatus, TenantStatus};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// 用于测试和演示的内存授权数据源。
#[derive(Debug, Default, Clone)]
pub struct MemorySource {
    inner: Arc<Inner>,
}

/// 内存数据源的共享内部状态。
#[derive(Debug, Default)]
struct Inner {
    tenants: RwLock<HashMap<TenantId, TenantStatus>>,
    memberships: RwLock<HashMap<TenantId, HashMap<PrincipalId, MembershipStatus>>>,
    assignments: RwLock<HashMap<TenantId, HashMap<PrincipalId, Vec<RoleAssignment>>>>,
    role_permissions: RwLock<HashMap<TenantId, HashMap<RoleId, HashSet<Permission>>>>,
    parent_roles: RwLock<HashMap<TenantId, HashMap<RoleId, HashSet<RoleId>>>>,
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

impl MemorySource {
    /// 创建空数据源。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置租户状态。
    pub fn set_tenant_status(&self, tenant: TenantId, status: TenantStatus) {
        write_guard(&self.inner.tenants).insert(tenant, status);
    }

    /// 设置主体成员关系状态。
    pub fn set_membership_status(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
        status: MembershipStatus,
    ) {
        write_guard(&self.inner.memberships)
            .entry(tenant)
            .or_default()
            .insert(principal, status);
    }

    /// 添加带范围的角色分配。
    pub fn add_role_assignment(
        &self,
        tenant: TenantId,
        principal: PrincipalId,
        role: RoleId,
        scope: GrantScope,
    ) {
        write_guard(&self.inner.assignments)
            .entry(tenant)
            .or_default()
            .entry(principal)
            .or_default()
            .push(RoleAssignment::new(role, scope));
    }

    /// 为角色添加权限。
    pub fn add_role_permission(&self, tenant: TenantId, role: RoleId, permission: Permission) {
        write_guard(&self.inner.role_permissions)
            .entry(tenant)
            .or_default()
            .entry(role)
            .or_default()
            .insert(permission);
    }

    /// 添加直接父角色。
    pub fn add_parent_role(&self, tenant: TenantId, role: RoleId, parent: RoleId) {
        write_guard(&self.inner.parent_roles)
            .entry(tenant)
            .or_default()
            .entry(role)
            .or_default()
            .insert(parent);
    }
}

#[async_trait]
impl AuthorizationSource for MemorySource {
    /// 查询租户状态，未配置时默认为未激活。
    async fn tenant_status(
        &self,
        tenant: &TenantId,
    ) -> std::result::Result<TenantStatus, crate::SourceError> {
        Ok(read_guard(&self.inner.tenants)
            .get(tenant)
            .copied()
            .unwrap_or(TenantStatus::Inactive))
    }

    /// 查询主体成员关系状态，未配置时默认为未激活。
    async fn membership_status(
        &self,
        subject: &AuthSubject,
    ) -> std::result::Result<MembershipStatus, crate::SourceError> {
        Ok(read_guard(&self.inner.memberships)
            .get(&subject.tenant)
            .and_then(|memberships| memberships.get(&subject.principal))
            .copied()
            .unwrap_or(MembershipStatus::Inactive))
    }

    /// 查询主体的角色分配。
    async fn role_assignments(
        &self,
        subject: &AuthSubject,
    ) -> std::result::Result<Vec<RoleAssignment>, crate::SourceError> {
        Ok(read_guard(&self.inner.assignments)
            .get(&subject.tenant)
            .and_then(|assignments| assignments.get(&subject.principal))
            .cloned()
            .unwrap_or_default())
    }

    /// 查询角色拥有的权限集合。
    async fn role_permissions(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> std::result::Result<Vec<Permission>, crate::SourceError> {
        Ok(read_guard(&self.inner.role_permissions)
            .get(tenant)
            .and_then(|permissions| permissions.get(role))
            .map(|permissions| permissions.iter().cloned().collect())
            .unwrap_or_default())
    }

    /// 查询角色的直接父角色集合。
    async fn parent_roles(
        &self,
        tenant: &TenantId,
        role: &RoleId,
    ) -> std::result::Result<Vec<RoleId>, crate::SourceError> {
        Ok(read_guard(&self.inner.parent_roles)
            .get(tenant)
            .and_then(|parents| parents.get(role))
            .map(|parents| parents.iter().cloned().collect())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn memory_source_should_default_to_inactive() {
        let source = MemorySource::new();
        let tenant = TenantId::parse("tenant_1").expect("tenant");
        let status = block_on(source.tenant_status(&tenant)).expect("status");

        assert_eq!(status, TenantStatus::Inactive);
    }

    #[test]
    fn memory_source_should_recover_from_poisoned_lock() {
        let source = MemorySource::new();
        let inner = source.inner.clone();
        let _ = std::thread::spawn(move || {
            let _guard = inner.tenants.write().unwrap();
            panic!("poison tenants lock");
        })
        .join();

        let tenant = TenantId::parse("tenant_1").expect("tenant");
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        let status = block_on(source.tenant_status(&tenant)).expect("status");

        assert_eq!(status, TenantStatus::Active);
    }
}

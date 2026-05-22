use futures::executor::block_on;
use rs_tenant::{
    AccessDecision, AuthSubject, EngineBuilder, GrantScope, MembershipStatus, MemorySource,
    Permission, PrincipalId, RoleId, ScopePath, ScopedAccessRequest, TenantId, TenantStatus,
};

fn main() -> rs_tenant::Result<()> {
    block_on(async {
        let tenant = TenantId::parse("tenant_demo")?;
        let principal = PrincipalId::parse("user_demo")?;
        let role = RoleId::parse("store_reader")?;
        let permission = Permission::parse("invoice:read")?;
        let root = ScopePath::parse("agent/1")?;
        let target = ScopePath::parse("agent/1/store/9")?;

        let source = MemorySource::new();
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            role.clone(),
            GrantScope::paths(vec![root])?,
        );
        source.add_role_permission(tenant.clone(), role, permission.clone());

        let engine = EngineBuilder::new(source).build();
        let decision = engine
            .can_access_scope(ScopedAccessRequest {
                subject: AuthSubject::new(tenant, principal),
                permission,
                target,
            })
            .await?;

        assert_eq!(decision, AccessDecision::Allow);
        Ok(())
    })
}

use futures::executor::block_on;
use rs_tenant::{
    AccessDecision, AuthSubject, EngineBuilder, GrantScope, MembershipStatus, MemoryCache,
    MemorySource, Permission, PrincipalId, RoleId, TenantAccessRequest, TenantId, TenantStatus,
};

fn main() -> rs_tenant::Result<()> {
    block_on(async {
        let tenant = TenantId::parse("tenant_cache_demo")?;
        let principal = PrincipalId::parse("user_cache_demo")?;
        let role = RoleId::parse("reader")?;

        let source = MemorySource::new();
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            role.clone(),
            GrantScope::tenant(),
        );
        source.add_role_permission(
            tenant.clone(),
            role.clone(),
            Permission::parse("invoice:read")?,
        );

        let engine = EngineBuilder::new(source)
            .cache(MemoryCache::new(1_024))
            .build();
        let request = TenantAccessRequest {
            subject: AuthSubject::new(tenant.clone(), principal),
            permission: Permission::parse("invoice:read")?,
        };

        assert_eq!(engine.can_tenant(request).await?, AccessDecision::Allow);
        engine.invalidate_role(&tenant, &role).await;
        Ok(())
    })
}

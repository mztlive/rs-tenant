use futures::executor::block_on;
use rs_tenant::{
    AccessDecision, Permission, TenantId,
    platform::{
        MemoryPlatformSource, PlatformEngineBuilder, PlatformGrantScope, PlatformPrincipalId,
        PlatformPrincipalStatus, PlatformRoleId, PlatformSubject, TenantDataAccessRequest,
    },
};

fn main() -> rs_tenant::Result<()> {
    block_on(async {
        let principal = PlatformPrincipalId::parse("platform_support")?;
        let role = PlatformRoleId::parse("tenant_support")?;
        let tenant = TenantId::parse("tenant_demo")?;
        let permission = Permission::parse("tenant/order:read")?;

        let source = MemoryPlatformSource::new();
        source.set_principal_status(principal.clone(), PlatformPrincipalStatus::Active);
        source.add_role_assignment(
            principal.clone(),
            role.clone(),
            PlatformGrantScope::tenants(vec![tenant.clone()])?,
        );
        source.add_role_permission(role, permission.clone());

        let engine = PlatformEngineBuilder::new(source).build();
        let decision = engine
            .can_access_tenant(TenantDataAccessRequest {
                subject: PlatformSubject::new(principal),
                permission,
                tenant,
            })
            .await?;

        assert_eq!(decision, AccessDecision::Allow);
        Ok(())
    })
}

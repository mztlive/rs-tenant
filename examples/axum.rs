use std::sync::Arc;

use futures::executor::block_on;
use rs_tenant::{
    AccessDecision, AuthSubject, EngineBuilder, GrantScope, MembershipStatus, MemorySource,
    Permission, PrincipalId, RoleId, ScopePath, TenantId, TenantStatus,
    axum::{AuthContext, TenantAuthorizeLayer, can_access_scope},
};

fn main() -> rs_tenant::Result<()> {
    block_on(async {
        let tenant = TenantId::parse("tenant_axum_demo")?;
        let principal = PrincipalId::parse("user_axum_demo")?;
        let role = RoleId::parse("reader")?;
        let permission = Permission::parse("invoice:read")?;

        let source = MemorySource::new();
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            role.clone(),
            GrantScope::tenant(),
        );
        source.add_role_permission(tenant.clone(), role, permission.clone());

        let engine = Arc::new(EngineBuilder::new(source).build());
        let _context = AuthContext::new(tenant.clone(), principal.clone());
        let _layer = TenantAuthorizeLayer::new(engine.clone(), permission.clone());
        let decision = can_access_scope(
            engine.as_ref(),
            AuthSubject::new(tenant, principal),
            permission,
            ScopePath::parse("agent/1")?,
        )
        .await?;

        assert_eq!(decision, AccessDecision::Allow);
        Ok(())
    })
}

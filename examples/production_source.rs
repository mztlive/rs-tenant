use async_trait::async_trait;
use futures::executor::block_on;
use rs_tenant::{
    AuthSubject, AuthorizationSource, EngineBuilder, GrantScope, MembershipStatus, Permission,
    RoleAssignment, RoleId, SourceError, TenantAccessRequest, TenantId, TenantStatus,
};

#[derive(Clone)]
struct DbAuthorizationSource;

#[async_trait]
impl AuthorizationSource for DbAuthorizationSource {
    async fn tenant_status(
        &self,
        _tenant: &TenantId,
    ) -> std::result::Result<TenantStatus, SourceError> {
        Ok(TenantStatus::Active)
    }

    async fn membership_status(
        &self,
        _subject: &AuthSubject,
    ) -> std::result::Result<MembershipStatus, SourceError> {
        Ok(MembershipStatus::Active)
    }

    async fn role_assignments(
        &self,
        _subject: &AuthSubject,
    ) -> std::result::Result<Vec<RoleAssignment>, SourceError> {
        let role = RoleId::parse("reader").map_err(|err| Box::new(err) as SourceError)?;
        Ok(vec![RoleAssignment::new(role, GrantScope::tenant())])
    }

    async fn role_permissions(
        &self,
        _tenant: &TenantId,
        _role: &RoleId,
    ) -> std::result::Result<Vec<Permission>, SourceError> {
        let permission =
            Permission::parse("invoice:read").map_err(|err| Box::new(err) as SourceError)?;
        Ok(vec![permission])
    }

    async fn parent_roles(
        &self,
        _tenant: &TenantId,
        _role: &RoleId,
    ) -> std::result::Result<Vec<RoleId>, SourceError> {
        Ok(Vec::new())
    }
}

fn main() -> rs_tenant::Result<()> {
    block_on(async {
        let engine = EngineBuilder::new(DbAuthorizationSource).build();
        let _decision = engine
            .can_tenant(TenantAccessRequest {
                subject: AuthSubject::new(
                    TenantId::parse("tenant_demo")?,
                    rs_tenant::PrincipalId::parse("user_demo")?,
                ),
                permission: Permission::parse("invoice:read")?,
            })
            .await?;
        Ok(())
    })
}

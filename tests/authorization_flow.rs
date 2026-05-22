#![cfg(feature = "memory-store")]

use futures::executor::block_on;
use rs_tenant::{
    AccessDecision, AccessScope, AuthSubject, DenyReason, EngineBuilder, GrantScope,
    MembershipStatus, MemorySource, Permission, PrincipalId, RoleId, ScopePath, ScopeQuery,
    ScopedAccessRequest, TenantAccessRequest, TenantId, TenantStatus,
};

/// 解析测试租户标识符。
fn parse_tenant(value: &str) -> TenantId {
    TenantId::parse(value).expect("tenant")
}

/// 解析测试主体标识符。
fn parse_principal(value: &str) -> PrincipalId {
    PrincipalId::parse(value).expect("principal")
}

/// 解析测试角色标识符。
fn parse_role(value: &str) -> RoleId {
    RoleId::parse(value).expect("role")
}

/// 解析测试权限。
fn parse_permission(value: &str) -> Permission {
    Permission::parse(value).expect("permission")
}

/// 解析测试范围路径。
fn parse_path(value: &str) -> ScopePath {
    ScopePath::parse(value).expect("scope path")
}

/// 创建一个已经激活租户和成员关系的测试数据源。
fn active_tenant_source(tenant: &TenantId, principal: &PrincipalId) -> MemorySource {
    let source = MemorySource::new();
    source.set_tenant_status(tenant.clone(), TenantStatus::Active);
    source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
    source
}

/// 创建租户内授权主体。
fn subject(tenant: &TenantId, principal: &PrincipalId) -> AuthSubject {
    AuthSubject::new(tenant.clone(), principal.clone())
}

#[test]
fn tenant_standard_authorization_flow_should_return_scope_and_allow_target() {
    let tenant = parse_tenant("tenant_flow");
    let principal = parse_principal("user_flow");
    let role = parse_role("store_reader");
    let source = active_tenant_source(&tenant, &principal);
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        role.clone(),
        GrantScope::paths(vec![parse_path("agent/1")]).expect("grant scope"),
    );
    source.add_role_permission(tenant.clone(), role, parse_permission("invoice:read"));
    let engine = EngineBuilder::new(source).build();
    let subject = subject(&tenant, &principal);

    // 标准流程先计算可访问数据范围，再用同一权限判断具体目标路径。
    let scope = block_on(engine.accessible_scope(ScopeQuery {
        subject: subject.clone(),
        permission: parse_permission("invoice:read"),
    }))
    .expect("scope");
    assert_eq!(
        scope,
        AccessScope::Paths {
            tenant: tenant.clone(),
            roots: vec![parse_path("agent/1")],
        }
    );

    let decision = block_on(engine.can_access_scope(ScopedAccessRequest {
        subject,
        permission: parse_permission("invoice:read"),
        target: parse_path("agent/1/store/9"),
    }))
    .expect("decision");
    assert_eq!(decision, AccessDecision::Allow);
}

#[test]
fn tenant_deny_by_default_flow_should_explain_every_gate() {
    let required_permission = parse_permission("invoice:read");

    // 租户未激活时，授权流程在最前置租户状态检查处拒绝。
    let tenant = parse_tenant("tenant_inactive_flow");
    let principal = parse_principal("user_inactive_flow");
    let source = MemorySource::new();
    source.set_tenant_status(tenant.clone(), TenantStatus::Inactive);
    let engine = EngineBuilder::new(source).build();
    let explanation = block_on(engine.explain_tenant(TenantAccessRequest {
        subject: subject(&tenant, &principal),
        permission: required_permission.clone(),
    }))
    .expect("explanation");
    assert_eq!(explanation.decision, AccessDecision::Deny);
    assert_eq!(explanation.reason, Some(DenyReason::TenantInactive));

    // 成员关系未激活时，即使租户有效也必须拒绝。
    let tenant = parse_tenant("tenant_member_inactive_flow");
    let principal = parse_principal("user_member_inactive_flow");
    let source = MemorySource::new();
    source.set_tenant_status(tenant.clone(), TenantStatus::Active);
    source.set_membership_status(
        tenant.clone(),
        principal.clone(),
        MembershipStatus::Inactive,
    );
    let engine = EngineBuilder::new(source).build();
    let explanation = block_on(engine.explain_tenant(TenantAccessRequest {
        subject: subject(&tenant, &principal),
        permission: required_permission.clone(),
    }))
    .expect("explanation");
    assert_eq!(explanation.decision, AccessDecision::Deny);
    assert_eq!(explanation.reason, Some(DenyReason::PrincipalInactive));

    // 没有角色分配时，最终表现为权限缺失。
    let tenant = parse_tenant("tenant_no_role_flow");
    let principal = parse_principal("user_no_role_flow");
    let source = active_tenant_source(&tenant, &principal);
    let engine = EngineBuilder::new(source).build();
    let explanation = block_on(engine.explain_tenant(TenantAccessRequest {
        subject: subject(&tenant, &principal),
        permission: required_permission.clone(),
    }))
    .expect("explanation");
    assert_eq!(explanation.decision, AccessDecision::Deny);
    assert_eq!(explanation.reason, Some(DenyReason::PermissionMissing));

    // 有角色但没有匹配权限时，仍然按权限缺失拒绝。
    let tenant = parse_tenant("tenant_permission_missing_flow");
    let principal = parse_principal("user_permission_missing_flow");
    let reader_role = parse_role("reader");
    let source = active_tenant_source(&tenant, &principal);
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        reader_role.clone(),
        GrantScope::tenant(),
    );
    source.add_role_permission(tenant.clone(), reader_role, parse_permission("order:read"));
    let engine = EngineBuilder::new(source).build();
    let explanation = block_on(engine.explain_tenant(TenantAccessRequest {
        subject: subject(&tenant, &principal),
        permission: required_permission.clone(),
    }))
    .expect("explanation");
    assert_eq!(explanation.decision, AccessDecision::Deny);
    assert_eq!(explanation.reason, Some(DenyReason::PermissionMissing));

    // 路径授权不能越权访问兄弟路径。
    let tenant = parse_tenant("tenant_scope_denied_flow");
    let principal = parse_principal("user_scope_denied_flow");
    let store_reader_role = parse_role("store_reader");
    let source = active_tenant_source(&tenant, &principal);
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        store_reader_role.clone(),
        GrantScope::paths(vec![parse_path("agent/1")]).expect("grant scope"),
    );
    source.add_role_permission(
        tenant.clone(),
        store_reader_role,
        required_permission.clone(),
    );
    let engine = EngineBuilder::new(source).build();
    let explanation = block_on(engine.explain_access_scope(ScopedAccessRequest {
        subject: subject(&tenant, &principal),
        permission: required_permission,
        target: parse_path("agent/2/store/9"),
    }))
    .expect("explanation");
    assert_eq!(explanation.decision, AccessDecision::Deny);
    assert_eq!(explanation.reason, Some(DenyReason::ScopeDenied));
}

#[test]
fn role_hierarchy_flow_should_require_engine_switch() {
    let tenant = parse_tenant("tenant_hierarchy_flow");
    let principal = parse_principal("user_hierarchy_flow");
    let child = parse_role("store_reader");
    let parent = parse_role("invoice_reader");
    let source = active_tenant_source(&tenant, &principal);
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        child.clone(),
        GrantScope::paths(vec![parse_path("agent/1")]).expect("grant scope"),
    );
    source.add_parent_role(tenant.clone(), child, parent.clone());
    source.add_role_permission(tenant.clone(), parent, parse_permission("invoice:read"));

    // 未开启角色继承时，父角色权限不参与授权。
    let strict_engine = EngineBuilder::new(source.clone()).build();
    let strict = block_on(strict_engine.can_access_scope(ScopedAccessRequest {
        subject: subject(&tenant, &principal),
        permission: parse_permission("invoice:read"),
        target: parse_path("agent/1/store/9"),
    }))
    .expect("decision");
    assert_eq!(strict, AccessDecision::Deny);

    // 开启角色继承后，父角色权限沿用子角色分配时的范围。
    let hierarchy_engine = EngineBuilder::new(source)
        .enable_role_hierarchy(true)
        .build();
    let inherited = block_on(hierarchy_engine.can_access_scope(ScopedAccessRequest {
        subject: subject(&tenant, &principal),
        permission: parse_permission("invoice:read"),
        target: parse_path("agent/1/store/9"),
    }))
    .expect("decision");
    assert_eq!(inherited, AccessDecision::Allow);
}

#[cfg(feature = "memory-cache")]
#[test]
fn cache_invalidation_flow_should_refresh_changed_authorization() {
    use rs_tenant::MemoryCache;

    let tenant = parse_tenant("tenant_cache_flow");
    let principal = parse_principal("user_cache_flow");
    let role = parse_role("reader");
    let source = active_tenant_source(&tenant, &principal);
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        role.clone(),
        GrantScope::tenant(),
    );
    let engine = EngineBuilder::new(source.clone())
        .cache(MemoryCache::new(16))
        .build();
    let request = TenantAccessRequest {
        subject: subject(&tenant, &principal),
        permission: parse_permission("invoice:read"),
    };

    // 第一次查询会把“没有权限”的有效授权写入缓存。
    let first = block_on(engine.can_tenant(request.clone())).expect("decision");
    assert_eq!(first, AccessDecision::Deny);

    // 数据源变更后，如果没有失效缓存，旧授权结果仍会被命中。
    source.add_role_permission(
        tenant.clone(),
        role.clone(),
        parse_permission("invoice:read"),
    );
    let stale = block_on(engine.can_tenant(request.clone())).expect("decision");
    assert_eq!(stale, AccessDecision::Deny);

    // 角色失效当前退化为租户失效；失效完成后必须重新读取数据源。
    block_on(engine.invalidate_role(&tenant, &role));
    let refreshed = block_on(engine.can_tenant(request)).expect("decision");
    assert_eq!(refreshed, AccessDecision::Allow);
}

#[cfg(feature = "platform")]
mod platform_flows {
    use super::{parse_path, parse_permission, parse_tenant};
    use futures::executor::block_on;
    use rs_tenant::platform::{
        MemoryPlatformSource, PlatformEngineBuilder, PlatformGrantScope, PlatformPrincipalId,
        PlatformPrincipalStatus, PlatformRoleId, PlatformSubject, TenantDataAccessRequest,
        TenantScopedDataAccessRequest, TenantScopedRoots,
    };
    use rs_tenant::{AccessDecision, ScopeRoots};

    /// 解析测试平台主体标识符。
    fn platform_principal(value: &str) -> PlatformPrincipalId {
        PlatformPrincipalId::parse(value).expect("platform principal")
    }

    /// 解析测试平台角色标识符。
    fn platform_role(value: &str) -> PlatformRoleId {
        PlatformRoleId::parse(value).expect("platform role")
    }

    #[test]
    fn platform_tenant_data_flow_should_apply_all_tenants_and_path_rules() {
        let source = MemoryPlatformSource::new();
        let principal = platform_principal("platform_support_flow");
        let all_tenants_role = platform_role("all_tenants_support");
        let path_role = platform_role("tenant_path_support");
        source.set_principal_status(principal.clone(), PlatformPrincipalStatus::Active);
        source.add_role_assignment(
            principal.clone(),
            all_tenants_role.clone(),
            PlatformGrantScope::all_tenants(),
        );
        source.add_role_permission(all_tenants_role, parse_permission("tenant/order:read"));
        source.add_role_assignment(
            principal.clone(),
            path_role.clone(),
            PlatformGrantScope::tenant_paths(vec![TenantScopedRoots::new(
                parse_tenant("tenant_path_only"),
                ScopeRoots::new(vec![parse_path("agent/1")]).expect("roots"),
            )])
            .expect("scope"),
        );
        source.add_role_permission(path_role, parse_permission("tenant/invoice:read"));
        let engine = PlatformEngineBuilder::new(source).build();
        let subject = PlatformSubject::new(principal);

        // AllTenants 可以访问任意租户级数据。
        let tenant_decision = block_on(engine.can_access_tenant(TenantDataAccessRequest {
            subject: subject.clone(),
            permission: parse_permission("tenant/order:read"),
            tenant: parse_tenant("tenant_any"),
        }))
        .expect("decision");
        assert_eq!(tenant_decision, AccessDecision::Allow);

        // TenantPaths 只允许指定租户下指定根路径的子孙路径。
        let inside_path = block_on(
            engine.can_access_tenant_scope(TenantScopedDataAccessRequest {
                subject: subject.clone(),
                permission: parse_permission("tenant/invoice:read"),
                tenant: parse_tenant("tenant_path_only"),
                target: parse_path("agent/1/store/9"),
            }),
        )
        .expect("decision");
        assert_eq!(inside_path, AccessDecision::Allow);

        let sibling_path = block_on(engine.can_access_tenant_scope(
            TenantScopedDataAccessRequest {
                subject,
                permission: parse_permission("tenant/invoice:read"),
                tenant: parse_tenant("tenant_path_only"),
                target: parse_path("agent/2/store/9"),
            },
        ))
        .expect("decision");
        assert_eq!(sibling_path, AccessDecision::Deny);
    }
}

#[cfg(feature = "axum")]
mod axum_flows {
    use super::{
        AuthSubject, EngineBuilder, GrantScope, MembershipStatus, Permission, TenantId,
        TenantStatus, active_tenant_source, parse_permission, parse_principal, parse_role,
        parse_tenant, subject,
    };
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::response::{IntoResponse, Response};
    use futures::executor::block_on;
    use rs_tenant::axum::{AuthContext, TenantAuthorizeLayer};
    use rs_tenant::{
        AuthorizationSource, MembershipStatus as SourceMembershipStatus, RoleAssignment, RoleId,
        SourceError,
    };
    use std::convert::Infallible;
    use std::future::{Ready, ready};
    use std::sync::Arc;
    use std::task::{Context, Poll};
    use tower::{Layer, Service};

    /// 用于中间件流程测试的成功响应服务。
    #[derive(Clone)]
    struct OkService;

    impl Service<Request<Body>> for OkService {
        type Response = Response;
        type Error = Infallible;
        type Future = Ready<Result<Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request<Body>) -> Self::Future {
            ready(Ok(StatusCode::NO_CONTENT.into_response()))
        }
    }

    /// 用于验证 source error 会被 HTTP 授权层映射为 500。
    #[derive(Clone)]
    struct FailingSource;

    #[async_trait]
    impl AuthorizationSource for FailingSource {
        async fn tenant_status(
            &self,
            _tenant: &TenantId,
        ) -> std::result::Result<TenantStatus, SourceError> {
            Err(Box::new(std::io::Error::other("source down")))
        }

        async fn membership_status(
            &self,
            _subject: &AuthSubject,
        ) -> std::result::Result<SourceMembershipStatus, SourceError> {
            Ok(MembershipStatus::Active)
        }

        async fn role_assignments(
            &self,
            _subject: &AuthSubject,
        ) -> std::result::Result<Vec<RoleAssignment>, SourceError> {
            Ok(Vec::new())
        }

        async fn role_permissions(
            &self,
            _tenant: &TenantId,
            _role: &RoleId,
        ) -> std::result::Result<Vec<Permission>, SourceError> {
            Ok(Vec::new())
        }

        async fn parent_roles(
            &self,
            _tenant: &TenantId,
            _role: &RoleId,
        ) -> std::result::Result<Vec<RoleId>, SourceError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn tenant_authorize_layer_flow_should_map_auth_results_to_status_codes() {
        let tenant = parse_tenant("tenant_http_flow");
        let principal = parse_principal("user_http_flow");
        let role = parse_role("admin");
        let source = active_tenant_source(&tenant, &principal);
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            role.clone(),
            GrantScope::tenant(),
        );
        source.add_role_permission(tenant.clone(), role, parse_permission("invoice:read"));
        let engine = EngineBuilder::new(source).build();
        let layer = TenantAuthorizeLayer::new(Arc::new(engine), parse_permission("invoice:read"));
        let mut service = layer.layer(OkService);

        // 请求带有 AuthContext 且权限命中时，请求进入内层服务。
        let mut allowed_req = Request::new(Body::empty());
        allowed_req
            .extensions_mut()
            .insert(AuthContext::new(tenant.clone(), principal.clone()));
        let allowed = block_on(service.call(allowed_req)).expect("response");
        assert_eq!(allowed.status(), StatusCode::NO_CONTENT);

        // 缺少认证上下文时，授权层直接返回 401。
        let missing = block_on(service.call(Request::new(Body::empty()))).expect("response");
        assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

        // 权限不匹配时，授权层返回 403。
        let denied_layer = TenantAuthorizeLayer::new(
            Arc::new(EngineBuilder::new(active_tenant_source(&tenant, &principal)).build()),
            parse_permission("invoice:delete"),
        );
        let mut denied_service = denied_layer.layer(OkService);
        let mut denied_req = Request::new(Body::empty());
        denied_req
            .extensions_mut()
            .insert(subject(&tenant, &principal));
        let denied = block_on(denied_service.call(denied_req)).expect("response");
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);

        // 数据源读取失败时，授权层按 fail-closed 返回 500。
        let failing_engine = EngineBuilder::new(FailingSource).build();
        let failing_layer =
            TenantAuthorizeLayer::new(Arc::new(failing_engine), parse_permission("invoice:read"));
        let mut failing_service = failing_layer.layer(OkService);
        let mut failing_req = Request::new(Body::empty());
        failing_req
            .extensions_mut()
            .insert(AuthContext::new(tenant, principal));
        let failed = block_on(failing_service.call(failing_req)).expect("response");
        assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[cfg(feature = "axum-jwt")]
    mod jwt_flows {
        use super::{
            Body, EngineBuilder, GrantScope, OkService, Request, StatusCode, TenantAuthorizeLayer,
            active_tenant_source, parse_permission, parse_principal, parse_role, parse_tenant,
        };
        use axum::http::header::AUTHORIZATION;
        use futures::executor::block_on;
        use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, encode};
        use rs_tenant::axum::jwt::{DefaultClaims, JwtAuthLayer, JwtAuthState};
        use std::sync::Arc;
        use std::time::{SystemTime, UNIX_EPOCH};
        use tower::{Layer, Service};

        /// 用于生成测试 JWT 的声明结构。
        #[derive(serde::Serialize)]
        struct TestClaims {
            tenant_id: String,
            principal_id: String,
            exp: usize,
        }

        /// 为指定租户和主体签发一个短期测试令牌。
        fn token_for(secret: &[u8], tenant_id: &str, principal_id: &str) -> String {
            // jsonwebtoken 10 需要应用显式安装一次进程级 crypto provider。
            let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
            let exp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs() as usize
                + 3600;
            encode(
                &Header::new(Algorithm::HS256),
                &TestClaims {
                    tenant_id: tenant_id.to_string(),
                    principal_id: principal_id.to_string(),
                    exp,
                },
                &EncodingKey::from_secret(secret),
            )
            .expect("token")
        }

        #[test]
        fn jwt_to_tenant_authorize_flow_should_decode_context_and_authorize_request() {
            let tenant = parse_tenant("tenant_jwt_flow");
            let principal = parse_principal("user_jwt_flow");
            let role = parse_role("admin");
            let source = active_tenant_source(&tenant, &principal);
            source.add_role_assignment(
                tenant.clone(),
                principal.clone(),
                role.clone(),
                GrantScope::tenant(),
            );
            source.add_role_permission(tenant.clone(), role, parse_permission("invoice:read"));
            let engine = EngineBuilder::new(source).build();
            let authorize_layer =
                TenantAuthorizeLayer::new(Arc::new(engine), parse_permission("invoice:read"));
            let secret = b"flow-secret";
            let jwt_layer = JwtAuthLayer::<DefaultClaims>::new(JwtAuthState::new(
                DecodingKey::from_secret(secret),
                Validation::new(Algorithm::HS256),
            ));
            let mut service = jwt_layer.layer(authorize_layer.layer(OkService));

            // JWT 层先解码并写入 AuthContext，租户授权层随后完成权限检查。
            let token = token_for(secret, tenant.as_str(), principal.as_str());
            let req = Request::builder()
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .expect("request");
            let response = block_on(service.call(req)).expect("response");
            assert_eq!(response.status(), StatusCode::NO_CONTENT);

            // 缺少 JWT 时，请求不会进入租户授权层。
            let missing = block_on(service.call(Request::new(Body::empty()))).expect("response");
            assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
        }
    }
}

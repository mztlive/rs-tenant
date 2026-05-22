#![cfg(feature = "memory-store")]

use async_trait::async_trait;
use futures::executor::block_on;
use rs_tenant::{
    AccessDecision, AccessScope, AuthSubject, AuthorizationSource, DenyReason, EngineBuilder,
    Error, GrantScope, MembershipStatus, MemorySource, Permission, PrincipalId, RoleAssignment,
    RoleId, ScopePath, ScopeQuery, ScopedAccessRequest, SourceError, TenantAccessRequest, TenantId,
    TenantStatus,
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

/// 租户数据源失败注入点。
#[derive(Clone, Copy)]
enum TenantSourceFailure {
    TenantStatus,
    MembershipStatus,
    RoleAssignments,
    RolePermissions,
    ParentRoles,
}

/// 用于验证 Engine 不会把数据源错误误转成普通拒绝的数据源。
struct FailingTenantSource {
    fail_at: TenantSourceFailure,
}

impl FailingTenantSource {
    /// 创建指定失败注入点的数据源。
    fn new(fail_at: TenantSourceFailure) -> Self {
        Self { fail_at }
    }

    /// 构造统一的数据源错误。
    fn source_error() -> SourceError {
        Box::new(std::io::Error::other("source down"))
    }
}

#[async_trait]
impl AuthorizationSource for FailingTenantSource {
    async fn tenant_status(
        &self,
        _tenant: &TenantId,
    ) -> std::result::Result<TenantStatus, SourceError> {
        if matches!(self.fail_at, TenantSourceFailure::TenantStatus) {
            return Err(Self::source_error());
        }
        Ok(TenantStatus::Active)
    }

    async fn membership_status(
        &self,
        _subject: &AuthSubject,
    ) -> std::result::Result<MembershipStatus, SourceError> {
        if matches!(self.fail_at, TenantSourceFailure::MembershipStatus) {
            return Err(Self::source_error());
        }
        Ok(MembershipStatus::Active)
    }

    async fn role_assignments(
        &self,
        _subject: &AuthSubject,
    ) -> std::result::Result<Vec<RoleAssignment>, SourceError> {
        if matches!(self.fail_at, TenantSourceFailure::RoleAssignments) {
            return Err(Self::source_error());
        }
        Ok(vec![RoleAssignment::new(
            parse_role("reader"),
            GrantScope::tenant(),
        )])
    }

    async fn role_permissions(
        &self,
        _tenant: &TenantId,
        _role: &RoleId,
    ) -> std::result::Result<Vec<Permission>, SourceError> {
        if matches!(self.fail_at, TenantSourceFailure::RolePermissions) {
            return Err(Self::source_error());
        }
        Ok(vec![parse_permission("invoice:read")])
    }

    async fn parent_roles(
        &self,
        _tenant: &TenantId,
        _role: &RoleId,
    ) -> std::result::Result<Vec<RoleId>, SourceError> {
        if matches!(self.fail_at, TenantSourceFailure::ParentRoles) {
            return Err(Self::source_error());
        }
        Ok(vec![parse_role("parent_reader")])
    }
}

/// 断言授权流程把数据源错误作为 Error::Source 透传。
fn assert_source_error(result: rs_tenant::Result<AccessDecision>) {
    assert!(matches!(result, Err(Error::Source(_))));
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

#[test]
fn tenant_source_error_flow_should_return_error_instead_of_deny() {
    let subject = subject(
        &parse_tenant("tenant_source_error_flow"),
        &parse_principal("user_source_error_flow"),
    );
    let request = TenantAccessRequest {
        subject: subject.clone(),
        permission: parse_permission("invoice:read"),
    };

    // 每个数据源读取点失败时，Engine 都必须 fail closed 并返回 Error::Source。
    for fail_at in [
        TenantSourceFailure::TenantStatus,
        TenantSourceFailure::MembershipStatus,
        TenantSourceFailure::RoleAssignments,
        TenantSourceFailure::RolePermissions,
    ] {
        let engine = EngineBuilder::new(FailingTenantSource::new(fail_at)).build();
        assert_source_error(block_on(engine.can_tenant(request.clone())));
    }

    // parent_roles 只有在角色继承开启后才会被读取。
    let hierarchy_engine =
        EngineBuilder::new(FailingTenantSource::new(TenantSourceFailure::ParentRoles))
            .enable_role_hierarchy(true)
            .build();
    assert_source_error(block_on(hierarchy_engine.can_tenant(request)));
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

#[cfg(feature = "memory-cache")]
#[test]
fn cache_invalidation_matrix_should_keep_scope_boundaries() {
    use rs_tenant::MemoryCache;

    let tenant = parse_tenant("tenant_cache_matrix");
    let other_tenant = parse_tenant("tenant_cache_matrix_other");
    let principal = parse_principal("user_cache_matrix");
    let other_principal = parse_principal("user_cache_matrix_other");
    let role = parse_role("reader");
    let source = MemorySource::new();
    for (tenant, principal) in [
        (&tenant, &principal),
        (&tenant, &other_principal),
        (&other_tenant, &principal),
    ] {
        source.set_tenant_status(tenant.clone(), TenantStatus::Active);
        source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            role.clone(),
            GrantScope::tenant(),
        );
    }
    let engine = EngineBuilder::new(source.clone())
        .cache(MemoryCache::new(32))
        .build();
    let request = |tenant: &TenantId, principal: &PrincipalId| TenantAccessRequest {
        subject: subject(tenant, principal),
        permission: parse_permission("invoice:read"),
    };

    // 先缓存三个拒绝结果，后续通过失效 API 验证影响范围。
    assert_eq!(
        block_on(engine.can_tenant(request(&tenant, &principal))).expect("decision"),
        AccessDecision::Deny
    );
    assert_eq!(
        block_on(engine.can_tenant(request(&tenant, &other_principal))).expect("decision"),
        AccessDecision::Deny
    );
    assert_eq!(
        block_on(engine.can_tenant(request(&other_tenant, &principal))).expect("decision"),
        AccessDecision::Deny
    );

    source.add_role_permission(
        tenant.clone(),
        role.clone(),
        parse_permission("invoice:read"),
    );
    block_on(engine.invalidate_principal(&tenant, &principal));
    assert_eq!(
        block_on(engine.can_tenant(request(&tenant, &principal))).expect("decision"),
        AccessDecision::Allow
    );
    assert_eq!(
        block_on(engine.can_tenant(request(&tenant, &other_principal))).expect("decision"),
        AccessDecision::Deny
    );

    block_on(engine.invalidate_tenant(&tenant));
    assert_eq!(
        block_on(engine.can_tenant(request(&tenant, &other_principal))).expect("decision"),
        AccessDecision::Allow
    );
    assert_eq!(
        block_on(engine.can_tenant(request(&other_tenant, &principal))).expect("decision"),
        AccessDecision::Deny
    );

    source.add_role_permission(
        other_tenant.clone(),
        role.clone(),
        parse_permission("invoice:read"),
    );
    block_on(engine.invalidate_all());
    assert_eq!(
        block_on(engine.can_tenant(request(&other_tenant, &principal))).expect("decision"),
        AccessDecision::Allow
    );
}

#[cfg(feature = "memory-cache")]
#[test]
fn cache_configuration_flow_should_isolate_signatures_and_support_disabled_cache() {
    use rs_tenant::MemoryCache;

    let tenant = parse_tenant("tenant_cache_config");
    let principal = parse_principal("user_cache_config");
    let role = parse_role("wildcard_reader");
    let source = active_tenant_source(&tenant, &principal);
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        role.clone(),
        GrantScope::tenant(),
    );
    source.add_role_permission(tenant.clone(), role.clone(), parse_permission("invoice:*"));
    let cache = MemoryCache::new(16);
    let strict_engine = EngineBuilder::new(source.clone())
        .cache(cache.clone())
        .build();
    let wildcard_engine = EngineBuilder::new(source.clone())
        .enable_wildcard(true)
        .cache(cache)
        .build();
    let request = TenantAccessRequest {
        subject: subject(&tenant, &principal),
        permission: parse_permission("invoice:read"),
    };

    // 不同 EngineConfig 的缓存签名必须隔离，wildcard 关闭的拒绝不能污染开启后的允许。
    assert_eq!(
        block_on(strict_engine.can_tenant(request.clone())).expect("decision"),
        AccessDecision::Deny
    );
    assert_eq!(
        block_on(wildcard_engine.can_tenant(request)).expect("decision"),
        AccessDecision::Allow
    );

    let disabled_source = active_tenant_source(&tenant, &principal);
    let disabled_role = parse_role("disabled_cache_reader");
    disabled_source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        disabled_role.clone(),
        GrantScope::tenant(),
    );
    let disabled_engine = EngineBuilder::new(disabled_source.clone())
        .cache(MemoryCache::new(0))
        .build();
    let request = TenantAccessRequest {
        subject: subject(&tenant, &principal),
        permission: parse_permission("order:read"),
    };
    assert_eq!(
        block_on(disabled_engine.can_tenant(request.clone())).expect("decision"),
        AccessDecision::Deny
    );
    disabled_source.add_role_permission(tenant, disabled_role, parse_permission("order:read"));
    assert_eq!(
        block_on(disabled_engine.can_tenant(request)).expect("decision"),
        AccessDecision::Allow
    );
}

#[cfg(feature = "memory-cache")]
#[test]
#[ignore = "并发压力测试；需要手动运行以避免拖慢默认测试"]
fn cache_concurrent_access_and_invalidation_should_not_deadlock_or_panic() {
    use rs_tenant::MemoryCache;
    use std::sync::Arc;

    let tenant = parse_tenant("tenant_cache_concurrent");
    let principal = parse_principal("user_cache_concurrent");
    let role = parse_role("reader");
    let source = active_tenant_source(&tenant, &principal);
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        role.clone(),
        GrantScope::tenant(),
    );
    source.add_role_permission(
        tenant.clone(),
        role.clone(),
        parse_permission("invoice:read"),
    );
    let engine = Arc::new(
        EngineBuilder::new(source)
            .cache(MemoryCache::new(128).with_shards(4))
            .build(),
    );

    // 多线程同时读授权和失效缓存，验证内存缓存不会死锁或 panic。
    let mut joins = Vec::new();
    for _ in 0..4 {
        let engine = Arc::clone(&engine);
        let tenant = tenant.clone();
        let principal = principal.clone();
        joins.push(std::thread::spawn(move || {
            for _ in 0..1_000 {
                let decision = block_on(engine.can_tenant(TenantAccessRequest {
                    subject: subject(&tenant, &principal),
                    permission: parse_permission("invoice:read"),
                }))
                .expect("decision");
                assert_eq!(decision, AccessDecision::Allow);
            }
        }));
    }

    let invalidator = {
        let engine = Arc::clone(&engine);
        let tenant = tenant.clone();
        std::thread::spawn(move || {
            for _ in 0..1_000 {
                block_on(engine.invalidate_role(&tenant, &role));
            }
        })
    };

    for join in joins {
        join.join().expect("reader thread should not panic");
    }
    invalidator
        .join()
        .expect("invalidator thread should not panic");
}

#[cfg(feature = "serde")]
mod serde_contracts {
    use super::{parse_path, parse_permission, parse_principal, parse_role, parse_tenant, subject};
    use rs_tenant::{
        GrantScope, RoleAssignment, ScopeQuery, ScopedAccessRequest, TenantAccessRequest,
    };

    #[test]
    fn tenant_public_api_should_roundtrip_through_json() {
        let tenant = parse_tenant("tenant_serde_flow");
        let principal = parse_principal("user_serde_flow");
        let auth_subject = subject(&tenant, &principal);
        let role_assignment = RoleAssignment::new(
            parse_role("reader"),
            GrantScope::paths(vec![parse_path("agent/1")]).expect("grant scope"),
        );
        let scope_query = ScopeQuery {
            subject: auth_subject.clone(),
            permission: parse_permission("invoice:read"),
        };
        let tenant_request = TenantAccessRequest {
            subject: auth_subject.clone(),
            permission: parse_permission("invoice:update"),
        };
        let scoped_request = ScopedAccessRequest {
            subject: auth_subject,
            permission: parse_permission("invoice:read"),
            target: parse_path("agent/1/store/9"),
        };

        // 公共租户 DTO 的 JSON roundtrip 必须保持强类型字段和值对象校验。
        let encoded = serde_json::to_string(&role_assignment).expect("serialize role assignment");
        let decoded: RoleAssignment =
            serde_json::from_str(&encoded).expect("deserialize role assignment");
        assert_eq!(decoded, role_assignment);
        let encoded = serde_json::to_string(&scope_query).expect("serialize scope query");
        let decoded: ScopeQuery = serde_json::from_str(&encoded).expect("deserialize scope query");
        assert_eq!(decoded, scope_query);
        let encoded = serde_json::to_string(&tenant_request).expect("serialize tenant request");
        let decoded: TenantAccessRequest =
            serde_json::from_str(&encoded).expect("deserialize tenant request");
        assert_eq!(decoded, tenant_request);
        let encoded = serde_json::to_string(&scoped_request).expect("serialize scoped request");
        let decoded: ScopedAccessRequest =
            serde_json::from_str(&encoded).expect("deserialize scoped request");
        assert_eq!(decoded, scoped_request);
    }

    #[test]
    fn tenant_public_api_should_reject_invalid_json_values() {
        // serde 反序列化必须复用值对象构造校验，不能让非法 ID、权限或路径绕进去。
        let invalid_subject = r#"{"tenant":"bad/tenant","principal":"user_1"}"#;
        assert!(serde_json::from_str::<rs_tenant::AuthSubject>(invalid_subject).is_err());

        let invalid_permission = r#"{
            "subject":{"tenant":"tenant_serde_flow","principal":"user_serde_flow"},
            "permission":"invoice:read:extra"
        }"#;
        assert!(serde_json::from_str::<ScopeQuery>(invalid_permission).is_err());

        let invalid_path = r#"{
            "subject":{"tenant":"tenant_serde_flow","principal":"user_serde_flow"},
            "permission":"invoice:read",
            "target":"agent//1"
        }"#;
        assert!(serde_json::from_str::<ScopedAccessRequest>(invalid_path).is_err());
    }

    #[cfg(feature = "platform")]
    mod platform {
        use super::{parse_path, parse_permission, parse_tenant};
        use rs_tenant::ScopeRoots;
        use rs_tenant::platform::{
            PlatformAccessRequest, PlatformGrantScope, PlatformPrincipalId, PlatformRoleAssignment,
            PlatformRoleId, PlatformSubject, TenantDataAccessRequest, TenantDataAccessScope,
            TenantDataScopeQuery, TenantScopedDataAccessRequest, TenantScopedRoots,
        };

        /// 解析测试平台主体标识符。
        fn platform_principal(value: &str) -> PlatformPrincipalId {
            PlatformPrincipalId::parse(value).expect("platform principal")
        }

        /// 解析测试平台角色标识符。
        fn platform_role(value: &str) -> PlatformRoleId {
            PlatformRoleId::parse(value).expect("platform role")
        }

        #[test]
        fn platform_public_api_should_roundtrip_through_json() {
            let subject = PlatformSubject::new(platform_principal("platform_serde_flow"));
            let role_assignment = PlatformRoleAssignment::new(
                platform_role("support"),
                PlatformGrantScope::tenant_paths(vec![TenantScopedRoots::new(
                    parse_tenant("tenant_serde_platform"),
                    ScopeRoots::new(vec![parse_path("agent/1")]).expect("roots"),
                )])
                .expect("scope"),
            );
            let platform_request = PlatformAccessRequest {
                subject: subject.clone(),
                permission: parse_permission("platform/role:read"),
            };
            let tenant_scope_query = TenantDataScopeQuery {
                subject: subject.clone(),
                permission: parse_permission("tenant/order:read"),
            };
            let tenant_request = TenantDataAccessRequest {
                subject: subject.clone(),
                permission: parse_permission("tenant/order:read"),
                tenant: parse_tenant("tenant_serde_platform"),
            };
            let scoped_request = TenantScopedDataAccessRequest {
                subject,
                permission: parse_permission("tenant/order:read"),
                tenant: parse_tenant("tenant_serde_platform"),
                target: parse_path("agent/1/store/9"),
            };
            let access_scope = TenantDataAccessScope::TenantPaths {
                entries: vec![TenantScopedRoots::new(
                    parse_tenant("tenant_serde_platform"),
                    ScopeRoots::new(vec![parse_path("agent/1")]).expect("roots"),
                )],
            };

            // 平台公共 DTO 的 JSON roundtrip 必须保持平台主体、角色和租户范围语义。
            let encoded =
                serde_json::to_string(&role_assignment).expect("serialize role assignment");
            let decoded: PlatformRoleAssignment =
                serde_json::from_str(&encoded).expect("deserialize role assignment");
            assert_eq!(decoded, role_assignment);
            let encoded =
                serde_json::to_string(&platform_request).expect("serialize platform request");
            let decoded: PlatformAccessRequest =
                serde_json::from_str(&encoded).expect("deserialize platform request");
            assert_eq!(decoded, platform_request);
            let encoded =
                serde_json::to_string(&tenant_scope_query).expect("serialize tenant scope query");
            let decoded: TenantDataScopeQuery =
                serde_json::from_str(&encoded).expect("deserialize tenant scope query");
            assert_eq!(decoded, tenant_scope_query);
            let encoded = serde_json::to_string(&tenant_request).expect("serialize tenant request");
            let decoded: TenantDataAccessRequest =
                serde_json::from_str(&encoded).expect("deserialize tenant request");
            assert_eq!(decoded, tenant_request);
            let encoded = serde_json::to_string(&scoped_request).expect("serialize scoped request");
            let decoded: TenantScopedDataAccessRequest =
                serde_json::from_str(&encoded).expect("deserialize scoped request");
            assert_eq!(decoded, scoped_request);
            let encoded = serde_json::to_string(&access_scope).expect("serialize access scope");
            let decoded: TenantDataAccessScope =
                serde_json::from_str(&encoded).expect("deserialize access scope");
            assert_eq!(decoded, access_scope);
        }

        #[test]
        fn platform_public_api_should_reject_invalid_json_values() {
            // 平台 serde 反序列化同样不能绕过平台 ID、租户 ID 和范围根校验。
            let invalid_subject = r#"{"principal":"bad/principal"}"#;
            assert!(serde_json::from_str::<PlatformSubject>(invalid_subject).is_err());

            let invalid_scope = r#"{"type":"tenant_paths","entries":[]}"#;
            assert!(serde_json::from_str::<TenantDataAccessScope>(invalid_scope).is_err());

            let invalid_request = r#"{
                "subject":{"principal":"platform_serde_flow"},
                "permission":"tenant/order:read",
                "tenant":"bad/tenant"
            }"#;
            assert!(serde_json::from_str::<TenantDataAccessRequest>(invalid_request).is_err());
        }
    }
}

#[cfg(feature = "platform")]
mod platform_flows {
    use super::{parse_path, parse_permission, parse_tenant};
    use async_trait::async_trait;
    use futures::executor::block_on;
    use rs_tenant::platform::{
        MemoryPlatformSource, PlatformAccessRequest, PlatformAuthorizationSource,
        PlatformEngineBuilder, PlatformGrantScope, PlatformPrincipalId, PlatformPrincipalStatus,
        PlatformRoleAssignment, PlatformRoleId, PlatformSubject, TenantDataAccessRequest,
        TenantDataScopeQuery, TenantScopedDataAccessRequest, TenantScopedRoots,
    };
    use rs_tenant::{AccessDecision, Error, Permission, ScopeRoots, SourceError};

    /// 解析测试平台主体标识符。
    fn platform_principal(value: &str) -> PlatformPrincipalId {
        PlatformPrincipalId::parse(value).expect("platform principal")
    }

    /// 解析测试平台角色标识符。
    fn platform_role(value: &str) -> PlatformRoleId {
        PlatformRoleId::parse(value).expect("platform role")
    }

    /// 平台数据源失败注入点。
    #[derive(Clone, Copy)]
    enum PlatformSourceFailure {
        PrincipalStatus,
        RoleAssignments,
        RolePermissions,
        ParentRoles,
    }

    /// 用于验证 PlatformEngine 不会吞掉平台数据源错误的数据源。
    struct FailingPlatformSource {
        fail_at: PlatformSourceFailure,
    }

    impl FailingPlatformSource {
        /// 创建指定失败注入点的平台数据源。
        fn new(fail_at: PlatformSourceFailure) -> Self {
            Self { fail_at }
        }

        /// 构造统一的平台数据源错误。
        fn source_error() -> SourceError {
            Box::new(std::io::Error::other("platform source down"))
        }
    }

    #[async_trait]
    impl PlatformAuthorizationSource for FailingPlatformSource {
        async fn platform_principal_status(
            &self,
            _subject: &PlatformSubject,
        ) -> std::result::Result<PlatformPrincipalStatus, SourceError> {
            if matches!(self.fail_at, PlatformSourceFailure::PrincipalStatus) {
                return Err(Self::source_error());
            }
            Ok(PlatformPrincipalStatus::Active)
        }

        async fn platform_role_assignments(
            &self,
            _subject: &PlatformSubject,
        ) -> std::result::Result<Vec<PlatformRoleAssignment>, SourceError> {
            if matches!(self.fail_at, PlatformSourceFailure::RoleAssignments) {
                return Err(Self::source_error());
            }
            Ok(vec![PlatformRoleAssignment::new(
                platform_role("platform_reader"),
                PlatformGrantScope::platform(),
            )])
        }

        async fn platform_role_permissions(
            &self,
            _role: &PlatformRoleId,
        ) -> std::result::Result<Vec<Permission>, SourceError> {
            if matches!(self.fail_at, PlatformSourceFailure::RolePermissions) {
                return Err(Self::source_error());
            }
            Ok(vec![parse_permission("platform/role:read")])
        }

        async fn platform_parent_roles(
            &self,
            _role: &PlatformRoleId,
        ) -> std::result::Result<Vec<PlatformRoleId>, SourceError> {
            if matches!(self.fail_at, PlatformSourceFailure::ParentRoles) {
                return Err(Self::source_error());
            }
            Ok(vec![platform_role("platform_parent")])
        }
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

    #[test]
    fn platform_scope_flows_should_keep_platform_and_tenant_data_separate() {
        let source = MemoryPlatformSource::new();
        let principal = platform_principal("platform_scope_flow");
        let platform_admin_role = platform_role("platform_admin");
        let tenant_limited_role = platform_role("tenant_limited_admin");
        source.set_principal_status(principal.clone(), PlatformPrincipalStatus::Active);
        source.add_role_assignment(
            principal.clone(),
            platform_admin_role.clone(),
            PlatformGrantScope::platform(),
        );
        source.add_role_permission(
            platform_admin_role,
            parse_permission("platform/role:update"),
        );
        source.add_role_assignment(
            principal.clone(),
            tenant_limited_role.clone(),
            PlatformGrantScope::tenants(vec![parse_tenant("tenant_a")]).expect("scope"),
        );
        source.add_role_permission(tenant_limited_role, parse_permission("tenant/order:read"));
        let engine = PlatformEngineBuilder::new(source).build();
        let subject = PlatformSubject::new(principal);

        // Platform scope 只允许访问平台自有资源。
        let platform_decision = block_on(engine.can_platform(PlatformAccessRequest {
            subject: subject.clone(),
            permission: parse_permission("platform/role:update"),
        }))
        .expect("decision");
        assert_eq!(platform_decision, AccessDecision::Allow);
        let platform_as_tenant_data = block_on(engine.can_access_tenant(TenantDataAccessRequest {
            subject: subject.clone(),
            permission: parse_permission("platform/role:update"),
            tenant: parse_tenant("tenant_a"),
        }))
        .expect("decision");
        assert_eq!(platform_as_tenant_data, AccessDecision::Deny);

        // Tenants([a]) 可以访问 tenant_a，拒绝 tenant_b。
        let tenant_a = block_on(engine.can_access_tenant(TenantDataAccessRequest {
            subject: subject.clone(),
            permission: parse_permission("tenant/order:read"),
            tenant: parse_tenant("tenant_a"),
        }))
        .expect("decision");
        assert_eq!(tenant_a, AccessDecision::Allow);
        let tenant_b = block_on(engine.can_access_tenant(TenantDataAccessRequest {
            subject,
            permission: parse_permission("tenant/order:read"),
            tenant: parse_tenant("tenant_b"),
        }))
        .expect("decision");
        assert_eq!(tenant_b, AccessDecision::Deny);
    }

    #[test]
    fn platform_role_hierarchy_flow_should_require_engine_switch() {
        let source = MemoryPlatformSource::new();
        let principal = platform_principal("platform_hierarchy_flow");
        let child = platform_role("support_child");
        let parent = platform_role("support_parent");
        source.set_principal_status(principal.clone(), PlatformPrincipalStatus::Active);
        source.add_role_assignment(
            principal.clone(),
            child.clone(),
            PlatformGrantScope::all_tenants(),
        );
        source.add_parent_role(child, parent.clone());
        source.add_role_permission(parent, parse_permission("tenant/order:read"));
        let subject = PlatformSubject::new(principal);
        let query = TenantDataScopeQuery {
            subject,
            permission: parse_permission("tenant/order:read"),
        };

        // 未开启平台角色继承时，父角色权限不参与授权。
        let strict_engine = PlatformEngineBuilder::new(source.clone()).build();
        let strict_scope =
            block_on(strict_engine.accessible_tenants(query.clone())).expect("tenant data scope");
        assert!(!strict_scope.allows_tenant(&parse_tenant("tenant_any")));

        // 开启平台角色继承后，父角色权限沿用子角色分配的租户数据范围。
        let hierarchy_engine = PlatformEngineBuilder::new(source)
            .enable_role_hierarchy(true)
            .build();
        let inherited_scope =
            block_on(hierarchy_engine.accessible_tenants(query)).expect("tenant data scope");
        assert!(inherited_scope.allows_tenant(&parse_tenant("tenant_any")));
    }

    #[test]
    fn platform_source_error_flow_should_return_error_instead_of_deny() {
        let subject = PlatformSubject::new(platform_principal("platform_source_error_flow"));
        let request = PlatformAccessRequest {
            subject: subject.clone(),
            permission: parse_permission("platform/role:read"),
        };

        // 每个直接平台数据源读取点失败时，PlatformEngine 都必须返回 Error::Source。
        for fail_at in [
            PlatformSourceFailure::PrincipalStatus,
            PlatformSourceFailure::RoleAssignments,
            PlatformSourceFailure::RolePermissions,
        ] {
            let engine = PlatformEngineBuilder::new(FailingPlatformSource::new(fail_at)).build();
            assert!(matches!(
                block_on(engine.can_platform(request.clone())),
                Err(Error::Source(_))
            ));
        }

        // parent_roles 只有在平台角色继承开启后才会被读取。
        let hierarchy_engine = PlatformEngineBuilder::new(FailingPlatformSource::new(
            PlatformSourceFailure::ParentRoles,
        ))
        .enable_role_hierarchy(true)
        .build();
        assert!(matches!(
            block_on(hierarchy_engine.can_platform(request)),
            Err(Error::Source(_))
        ));
    }
}

#[cfg(feature = "axum")]
mod axum_flows {
    use super::{
        AccessDecision, AuthSubject, EngineBuilder, GrantScope, MembershipStatus, Permission,
        TenantId, TenantStatus, active_tenant_source, parse_permission, parse_principal,
        parse_role, parse_tenant, subject,
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
    use std::sync::atomic::{AtomicUsize, Ordering};
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

    /// 用于验证授权失败时内层服务不会被调用的计数服务。
    #[derive(Clone)]
    struct CountingService {
        calls: Arc<AtomicUsize>,
    }

    impl CountingService {
        /// 创建共享计数服务。
        fn new(calls: Arc<AtomicUsize>) -> Self {
            Self { calls }
        }
    }

    impl Service<Request<Body>> for CountingService {
        type Response = Response;
        type Error = Infallible;
        type Future = Ready<Result<Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request<Body>) -> Self::Future {
            self.calls.fetch_add(1, Ordering::SeqCst);
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

        // 请求直接带 AuthSubject 时也可以被租户授权层识别。
        let mut subject_req = Request::new(Body::empty());
        subject_req
            .extensions_mut()
            .insert(subject(&tenant, &principal));
        let allowed = block_on(service.call(subject_req)).expect("response");
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

    #[test]
    fn tenant_authorize_layer_should_not_call_inner_service_when_request_is_rejected() {
        let tenant = parse_tenant("tenant_http_inner_flow");
        let principal = parse_principal("user_http_inner_flow");
        let source = active_tenant_source(&tenant, &principal);
        let engine = EngineBuilder::new(source).build();
        let layer = TenantAuthorizeLayer::new(Arc::new(engine), parse_permission("invoice:read"));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut service = layer.layer(CountingService::new(calls.clone()));

        // 缺认证上下文时，内层服务不能被调用。
        let missing = block_on(service.call(Request::new(Body::empty()))).expect("response");
        assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        // 有认证上下文但权限拒绝时，内层服务仍不能被调用。
        let mut denied_req = Request::new(Body::empty());
        denied_req
            .extensions_mut()
            .insert(AuthContext::new(tenant, principal));
        let denied = block_on(service.call(denied_req)).expect("response");
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn axum_scope_helper_should_match_engine_decision() {
        let tenant = parse_tenant("tenant_axum_helper_flow");
        let principal = parse_principal("user_axum_helper_flow");
        let role = parse_role("reader");
        let source = active_tenant_source(&tenant, &principal);
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            role.clone(),
            GrantScope::paths(vec![super::parse_path("agent/1")]).expect("grant scope"),
        );
        source.add_role_permission(tenant.clone(), role, parse_permission("invoice:read"));
        let engine = EngineBuilder::new(source).build();
        let decision = block_on(rs_tenant::axum::can_access_scope(
            &engine,
            subject(&tenant, &principal),
            parse_permission("invoice:read"),
            super::parse_path("agent/1/store/9"),
        ))
        .expect("decision");

        // Axum helper 只是薄封装，返回值必须与 Engine::can_access_scope 一致。
        assert_eq!(decision, AccessDecision::Allow);
    }

    #[cfg(feature = "axum-jwt")]
    mod jwt_flows {
        use super::{
            AuthContext, Body, EngineBuilder, GrantScope, OkService, Request, StatusCode,
            TenantAuthorizeLayer, active_tenant_source, parse_permission, parse_principal,
            parse_role, parse_tenant,
        };
        use axum::extract::FromRequestParts;
        use axum::http::header::AUTHORIZATION;
        use futures::executor::block_on;
        use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, encode};
        use rs_tenant::axum::jwt::{
            DefaultClaims, JwtAuth, JwtAuthLayer, JwtAuthProvider, JwtAuthState,
        };
        use std::sync::Arc;
        use std::time::{SystemTime, UNIX_EPOCH};
        use tower::{Layer, Service};

        /// 为 JWT extractor 测试提供认证状态。
        struct JwtState {
            auth: JwtAuthState<DefaultClaims>,
        }

        impl JwtAuthProvider<DefaultClaims> for JwtState {
            fn jwt_auth(&self) -> &JwtAuthState<DefaultClaims> {
                &self.auth
            }
        }

        /// 用于生成测试 JWT 的声明结构。
        #[derive(serde::Serialize)]
        struct TestClaims {
            tenant_id: String,
            principal_id: String,
            exp: usize,
        }

        /// 为指定租户和主体签发一个短期测试令牌。
        fn token_for(secret: &[u8], tenant_id: &str, principal_id: &str) -> String {
            let exp = current_timestamp() + 3600;
            token_with_exp(secret, tenant_id, principal_id, exp)
        }

        /// 返回当前 Unix 时间戳。
        fn current_timestamp() -> usize {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs() as usize
        }

        /// 使用指定过期时间签发测试令牌。
        fn token_with_exp(
            secret: &[u8],
            tenant_id: &str,
            principal_id: &str,
            exp: usize,
        ) -> String {
            // jsonwebtoken 10 需要应用显式安装一次进程级 crypto provider。
            let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
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

        #[test]
        fn jwt_flow_should_reject_invalid_tokens_before_authorization() {
            let tenant = parse_tenant("tenant_jwt_negative_flow");
            let principal = parse_principal("user_jwt_negative_flow");
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

            // 非 Bearer 格式会被 JWT 层拒绝。
            let req = Request::builder()
                .header(AUTHORIZATION, "Token not-a-bearer")
                .body(Body::empty())
                .expect("request");
            let response = block_on(service.call(req)).expect("response");
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

            // 签名密钥不匹配会被 JWT 层拒绝。
            let wrong_signature_token =
                token_for(b"wrong-secret", tenant.as_str(), principal.as_str());
            let req = Request::builder()
                .header(AUTHORIZATION, format!("Bearer {wrong_signature_token}"))
                .body(Body::empty())
                .expect("request");
            let response = block_on(service.call(req)).expect("response");
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

            // 过期令牌不能进入授权层。
            let expired_token = token_with_exp(
                secret,
                tenant.as_str(),
                principal.as_str(),
                current_timestamp() - 120,
            );
            let req = Request::builder()
                .header(AUTHORIZATION, format!("Bearer {expired_token}"))
                .body(Body::empty())
                .expect("request");
            let response = block_on(service.call(req)).expect("response");
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

            // claims 内的租户或主体标识符非法时，认证层直接拒绝。
            let invalid_claims_token = token_for(secret, "bad/tenant", principal.as_str());
            let req = Request::builder()
                .header(AUTHORIZATION, format!("Bearer {invalid_claims_token}"))
                .body(Body::empty())
                .expect("request");
            let response = block_on(service.call(req)).expect("response");
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }

        #[test]
        fn jwt_extractors_should_decode_once_and_reuse_extensions() {
            let secret = b"extractor-secret";
            let state = JwtState {
                auth: JwtAuthState::new(
                    DecodingKey::from_secret(secret),
                    Validation::new(Algorithm::HS256),
                ),
            };
            let token = token_for(secret, "tenant_extractor_flow", "user_extractor_flow");
            let request = Request::builder()
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .expect("request");
            let (mut parts, _) = request.into_parts();

            // 第一次提取会解码 JWT，并写入 JwtAuth、AuthContext 和 AuthSubject。
            let jwt_auth = block_on(JwtAuth::<DefaultClaims>::from_request_parts(
                &mut parts, &state,
            ))
            .expect("jwt auth");
            assert_eq!(
                jwt_auth.context.subject.tenant.as_str(),
                "tenant_extractor_flow"
            );
            assert!(parts.extensions.get::<JwtAuth<DefaultClaims>>().is_some());
            assert!(parts.extensions.get::<AuthContext>().is_some());
            assert!(parts.extensions.get::<rs_tenant::AuthSubject>().is_some());

            // 第二次提取 AuthContext 应复用 extensions 中已有的 JwtAuth。
            let context = block_on(AuthContext::from_request_parts(&mut parts, &state))
                .expect("auth context");
            assert_eq!(context.subject.principal.as_str(), "user_extractor_flow");
        }
    }
}

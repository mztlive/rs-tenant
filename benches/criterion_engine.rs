#![cfg(all(
    feature = "criterion-bench",
    feature = "memory-store",
    feature = "memory-cache"
))]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use futures::executor::block_on;
use rs_tenant::{
    AuthSubject, EngineBuilder, GrantScope, MembershipStatus, MemoryCache, MemorySource,
    Permission, PrincipalId, RoleId, ScopePath, ScopedAccessRequest, TenantAccessRequest, TenantId,
    TenantStatus,
};
use std::hint::black_box;
use std::time::Duration;

/// 构造路径级访问基准测试使用的扁平授权数据源。
fn setup_flat_source() -> (MemorySource, AuthSubject, Permission, ScopePath) {
    let source = MemorySource::new();
    let tenant = TenantId::parse("tenant_bench").unwrap();
    let principal = PrincipalId::parse("principal_bench").unwrap();
    let role = RoleId::parse("role_reader").unwrap();
    let permission = Permission::parse("invoice:read").unwrap();
    let scope = ScopePath::parse("agent/1").unwrap();

    source.set_tenant_status(tenant.clone(), TenantStatus::Active);
    source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        role.clone(),
        GrantScope::paths(vec![scope.clone()]).unwrap(),
    );
    source.add_role_permission(tenant.clone(), role, permission.clone());

    (
        source,
        AuthSubject::new(tenant, principal),
        permission,
        scope,
    )
}

/// 构造租户级访问基准测试使用的数据源。
fn setup_tenant_source() -> (MemorySource, AuthSubject, Permission) {
    let (source, subject, permission, _) = setup_flat_source();
    let role = RoleId::parse("tenant_admin").unwrap();
    source.add_role_assignment(
        subject.tenant.clone(),
        subject.principal.clone(),
        role.clone(),
        GrantScope::tenant(),
    );
    source.add_role_permission(subject.tenant.clone(), role, permission.clone());
    (source, subject, permission)
}

/// 构造角色继承深度基准测试使用的数据源。
fn setup_hierarchy_source(depth: usize) -> (MemorySource, AuthSubject, Permission, ScopePath) {
    let source = MemorySource::new();
    let tenant = TenantId::parse("tenant_hierarchy_bench").unwrap();
    let principal = PrincipalId::parse("principal_hierarchy_bench").unwrap();
    let permission = Permission::parse("invoice:read").unwrap();
    let scope = ScopePath::parse("agent/1").unwrap();

    source.set_tenant_status(tenant.clone(), TenantStatus::Active);
    source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);

    let first_role = RoleId::parse("role_chain_0").unwrap();
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        first_role,
        GrantScope::paths(vec![scope.clone()]).unwrap(),
    );

    for i in 0..depth {
        let current = RoleId::parse(format!("role_chain_{i}").as_str()).unwrap();
        let next = RoleId::parse(format!("role_chain_{}", i + 1).as_str()).unwrap();
        source.add_parent_role(tenant.clone(), current, next);
    }

    let tail = RoleId::parse(format!("role_chain_{depth}").as_str()).unwrap();
    source.add_role_permission(tenant.clone(), tail, permission.clone());

    (
        source,
        AuthSubject::new(tenant, principal),
        permission,
        scope,
    )
}

/// 构造多角色扇出基准测试使用的数据源。
fn setup_role_fanout_source(role_count: usize) -> (MemorySource, AuthSubject, Permission) {
    let source = MemorySource::new();
    let tenant = TenantId::parse("tenant_fanout_bench").unwrap();
    let principal = PrincipalId::parse("principal_fanout_bench").unwrap();

    source.set_tenant_status(tenant.clone(), TenantStatus::Active);
    source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);

    for i in 0..role_count {
        let role = RoleId::parse(format!("role_{i}").as_str()).unwrap();
        let permission = Permission::parse(format!("invoice_{i}:read").as_str()).unwrap();
        source.add_role_assignment(
            tenant.clone(),
            principal.clone(),
            role.clone(),
            GrantScope::tenant(),
        );
        source.add_role_permission(tenant.clone(), role, permission);
    }

    let required = Permission::parse(format!("invoice_{}:read", role_count - 1).as_str()).unwrap();
    (source, AuthSubject::new(tenant, principal), required)
}

/// 执行扁平授权和缓存命中的基准测试。
fn bench_flat(c: &mut Criterion) {
    let mut group = c.benchmark_group("v03_flat_access");
    group.sample_size(30);
    group.throughput(Throughput::Elements(1));

    let (source, subject, permission, scope) = setup_flat_source();
    let engine = EngineBuilder::new(source).build();
    group.bench_function("can_access_scope_no_cache", |b| {
        b.iter(|| {
            let decision = block_on(engine.can_access_scope(ScopedAccessRequest {
                subject: subject.clone(),
                permission: permission.clone(),
                target: scope.clone(),
            }))
            .unwrap();
            black_box(decision);
        });
    });

    let (source, subject, permission) = setup_tenant_source();
    let engine = EngineBuilder::new(source)
        .cache(
            MemoryCache::new(8_192)
                .with_shards(8)
                .with_ttl(Duration::from_secs(60)),
        )
        .build();
    group.bench_function("can_tenant_cache", |b| {
        b.iter(|| {
            let decision = block_on(engine.can_tenant(TenantAccessRequest {
                subject: subject.clone(),
                permission: permission.clone(),
            }))
            .unwrap();
            black_box(decision);
        });
    });

    group.finish();
}

/// 执行不同角色继承深度的基准测试。
fn bench_hierarchy_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("v03_hierarchy_depth");
    group.sample_size(30);
    group.throughput(Throughput::Elements(1));

    for depth in [1usize, 4, 8, 16] {
        let (source, subject, permission, scope) = setup_hierarchy_source(depth);
        let engine = EngineBuilder::new(source)
            .enable_role_hierarchy(true)
            .max_role_depth(depth + 2)
            .build();
        let id = BenchmarkId::from_parameter(depth);
        group.bench_with_input(id, &depth, |b, _| {
            b.iter(|| {
                let decision = block_on(engine.can_access_scope(ScopedAccessRequest {
                    subject: subject.clone(),
                    permission: permission.clone(),
                    target: scope.clone(),
                }))
                .unwrap();
                black_box(decision);
            });
        });
    }

    group.finish();
}

/// 执行不同角色数量扇出的基准测试。
fn bench_role_fanout(c: &mut Criterion) {
    let mut group = c.benchmark_group("v03_role_fanout");
    group.sample_size(30);
    group.throughput(Throughput::Elements(1));

    for role_count in [1usize, 8, 32, 128] {
        let (source, subject, permission) = setup_role_fanout_source(role_count);
        let engine = EngineBuilder::new(source).build();
        let id = BenchmarkId::from_parameter(role_count);
        group.bench_with_input(id, &role_count, |b, _| {
            b.iter(|| {
                let decision = block_on(engine.can_tenant(TenantAccessRequest {
                    subject: subject.clone(),
                    permission: permission.clone(),
                }))
                .unwrap();
                black_box(decision);
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_flat,
    bench_hierarchy_depth,
    bench_role_fanout
);
criterion_main!(benches);

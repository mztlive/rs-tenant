#![cfg(all(feature = "memory-store", feature = "memory-cache"))]

use futures::executor::block_on;
use rs_tenant::{
    AccessDecision, AuthSubject, EngineBuilder, GrantScope, MembershipStatus, MemoryCache,
    MemorySource, Permission, PrincipalId, RoleId, ScopePath, ScopedAccessRequest,
    TenantAccessRequest, TenantId, TenantStatus,
};
use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

const REPEATS: usize = 5;

fn benchmark_sync<F>(name: &str, iterations: usize, mut op: F)
where
    F: FnMut(),
{
    let mut samples = Vec::with_capacity(REPEATS);
    for _ in 0..REPEATS {
        let start = Instant::now();
        for _ in 0..iterations {
            op();
        }
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    let median = samples[REPEATS / 2];
    let total_ms = median.as_secs_f64() * 1_000.0;
    let ns_per_op = median.as_secs_f64() * 1_000_000_000.0 / iterations as f64;
    let ops_per_sec = iterations as f64 / median.as_secs_f64();

    println!(
        "{name}: median={total_ms:.3} ms, ns/op={ns_per_op:.1}, ops/s={ops_per_sec:.0} (iters={iterations}, repeats={REPEATS})"
    );
}

fn benchmark_parallel<F>(name: &str, threads: usize, iterations_per_thread: usize, op_factory: F)
where
    F: Fn() -> Box<dyn FnMut() + Send> + Send + Sync + 'static,
{
    let op_factory = Arc::new(op_factory);
    let mut samples = Vec::with_capacity(REPEATS);

    for _ in 0..REPEATS {
        let start = Instant::now();
        let mut joins = Vec::with_capacity(threads);
        for _ in 0..threads {
            let factory = Arc::clone(&op_factory);
            joins.push(std::thread::spawn(move || {
                let mut op = factory();
                for _ in 0..iterations_per_thread {
                    op();
                }
            }));
        }
        for join in joins {
            join.join().expect("thread panicked");
        }
        samples.push(start.elapsed());
    }

    samples.sort_unstable();
    let median = samples[REPEATS / 2];
    let total_ops = threads * iterations_per_thread;
    let total_ms = median.as_secs_f64() * 1_000.0;
    let ns_per_op = median.as_secs_f64() * 1_000_000_000.0 / total_ops as f64;
    let ops_per_sec = total_ops as f64 / median.as_secs_f64();

    println!(
        "{name}: median={total_ms:.3} ms, ns/op={ns_per_op:.1}, ops/s={ops_per_sec:.0} (threads={threads}, total_ops={total_ops}, repeats={REPEATS})"
    );
}

fn setup_flat_source() -> (MemorySource, AuthSubject, Permission, ScopePath) {
    let source = MemorySource::new();
    let tenant = TenantId::parse("tenant_perf").unwrap();
    let principal = PrincipalId::parse("principal_perf").unwrap();
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
    source.add_role_permission(tenant, role, permission.clone());

    (
        source,
        AuthSubject::new(TenantId::parse("tenant_perf").unwrap(), principal),
        permission,
        scope,
    )
}

fn setup_hierarchy_source(depth: usize) -> (MemorySource, AuthSubject, Permission, usize) {
    let source = MemorySource::new();
    let tenant = TenantId::parse("tenant_hier_perf").unwrap();
    let principal = PrincipalId::parse("principal_hier_perf").unwrap();
    let permission = Permission::parse("invoice:read").unwrap();
    let scope = ScopePath::parse("agent/1").unwrap();

    source.set_tenant_status(tenant.clone(), TenantStatus::Active);
    source.set_membership_status(tenant.clone(), principal.clone(), MembershipStatus::Active);

    let first_role = RoleId::parse("role_chain_0").unwrap();
    source.add_role_assignment(
        tenant.clone(),
        principal.clone(),
        first_role,
        GrantScope::paths(vec![scope]).unwrap(),
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
        depth,
    )
}

#[test]
#[ignore = "manual performance test; run with --ignored --nocapture"]
fn perf_can_tenant_and_scoped_access() {
    let iterations = 200_000;

    let (source, subject, permission, scope) = setup_flat_source();
    let engine = EngineBuilder::new(source).build();
    benchmark_sync("can_access_scope_flat_no_cache", iterations, || {
        let result = block_on(engine.can_access_scope(ScopedAccessRequest {
            subject: subject.clone(),
            permission: permission.clone(),
            target: scope.clone(),
        }))
        .unwrap();
        black_box(result);
    });

    let (source, subject, permission, scope) = setup_flat_source();
    let tenant_source = source.clone();
    let tenant_role = RoleId::parse("tenant_admin").unwrap();
    tenant_source.add_role_assignment(
        subject.tenant.clone(),
        subject.principal.clone(),
        tenant_role.clone(),
        GrantScope::tenant(),
    );
    tenant_source.add_role_permission(subject.tenant.clone(), tenant_role, permission.clone());
    let engine = EngineBuilder::new(source)
        .cache(MemoryCache::new(8_192).with_ttl(Duration::from_secs(60)))
        .build();
    let warm = block_on(engine.can_tenant(TenantAccessRequest {
        subject: subject.clone(),
        permission: permission.clone(),
    }))
    .unwrap();
    assert_eq!(warm, AccessDecision::Allow);
    benchmark_sync("can_tenant_hot_cache", iterations, || {
        let result = block_on(engine.can_tenant(TenantAccessRequest {
            subject: subject.clone(),
            permission: permission.clone(),
        }))
        .unwrap();
        black_box(result);
    });

    benchmark_sync("can_access_scope_hot_cache", iterations, || {
        let result = block_on(engine.can_access_scope(ScopedAccessRequest {
            subject: subject.clone(),
            permission: permission.clone(),
            target: scope.clone(),
        }))
        .unwrap();
        black_box(result);
    });

    let (source, subject, permission, depth) = setup_hierarchy_source(8);
    let engine = EngineBuilder::new(source)
        .enable_role_hierarchy(true)
        .max_role_depth(depth + 2)
        .build();
    benchmark_sync(
        "can_access_scope_hierarchy_depth8_no_cache",
        iterations / 4,
        || {
            let target = ScopePath::parse("agent/1/store/2").unwrap();
            let result = block_on(engine.can_access_scope(ScopedAccessRequest {
                subject: subject.clone(),
                permission: permission.clone(),
                target,
            }))
            .unwrap();
            black_box(result);
        },
    );

    let threads = std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4);
    let iterations_per_thread = 50_000;

    let (source, subject, permission, scope) = setup_flat_source();
    let engine = Arc::new(
        EngineBuilder::new(source)
            .cache(
                MemoryCache::new(8_192)
                    .with_shards(8)
                    .with_ttl(Duration::from_secs(60)),
            )
            .build(),
    );
    let _ = block_on(engine.can_access_scope(ScopedAccessRequest {
        subject: subject.clone(),
        permission: permission.clone(),
        target: scope.clone(),
    }))
    .unwrap();

    benchmark_parallel(
        "can_access_scope_hot_cache_parallel",
        threads,
        iterations_per_thread,
        move || {
            let engine = Arc::clone(&engine);
            let subject = subject.clone();
            let permission = permission.clone();
            let scope = scope.clone();
            Box::new(move || {
                let result = block_on(engine.can_access_scope(ScopedAccessRequest {
                    subject: subject.clone(),
                    permission: permission.clone(),
                    target: scope.clone(),
                }))
                .unwrap();
                black_box(result);
            })
        },
    );
}

#![cfg(all(feature = "memory-store", feature = "memory-cache"))]

use futures::executor::block_on;
use rs_tenant::{
    Decision, EngineBuilder, MemoryCache, MemoryStore, Permission, PrincipalId, ResourceName,
    RoleId, Scope, TenantId,
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

fn setup_flat_store() -> (MemoryStore, TenantId, PrincipalId, Permission, ResourceName) {
    let store = MemoryStore::new();
    let tenant = TenantId::try_from("tenant_perf").unwrap();
    let principal = PrincipalId::try_from("principal_perf").unwrap();
    let role = RoleId::try_from("role_reader").unwrap();
    let permission = Permission::try_from("invoice:read").unwrap();
    let resource = ResourceName::try_from("invoice").unwrap();

    store.set_tenant_active(tenant.clone(), true);
    store.set_principal_active(tenant.clone(), principal.clone(), true);
    store.add_principal_role(tenant.clone(), principal.clone(), role.clone());
    store.add_role_permission(tenant.clone(), role, permission.clone());

    (store, tenant, principal, permission, resource)
}

fn setup_hierarchy_store(
    depth: usize,
) -> (
    MemoryStore,
    TenantId,
    PrincipalId,
    Permission,
    ResourceName,
    usize,
) {
    let store = MemoryStore::new();
    let tenant = TenantId::try_from("tenant_hier_perf").unwrap();
    let principal = PrincipalId::try_from("principal_hier_perf").unwrap();
    let permission = Permission::try_from("invoice:read").unwrap();
    let resource = ResourceName::try_from("invoice").unwrap();

    store.set_tenant_active(tenant.clone(), true);
    store.set_principal_active(tenant.clone(), principal.clone(), true);

    let first_role = RoleId::try_from("role_chain_0").unwrap();
    store.add_principal_role(tenant.clone(), principal.clone(), first_role);

    for i in 0..depth {
        let current = RoleId::try_from(format!("role_chain_{i}").as_str()).unwrap();
        let next = RoleId::try_from(format!("role_chain_{}", i + 1).as_str()).unwrap();
        store.add_role_inherit(tenant.clone(), current, next);
    }

    let tail = RoleId::try_from(format!("role_chain_{depth}").as_str()).unwrap();
    store.add_role_permission(tenant.clone(), tail, permission.clone());

    (store, tenant, principal, permission, resource, depth)
}

#[test]
#[ignore = "manual performance test; run with --ignored --nocapture"]
fn perf_authorize_and_scope() {
    let iterations = 200_000;

    let (store, tenant, principal, permission, _resource) = setup_flat_store();
    let engine = EngineBuilder::new(store).build();
    benchmark_sync("authorize_flat_no_cache", iterations, || {
        let result = block_on(engine.authorize_ref(&tenant, &principal, &permission)).unwrap();
        black_box(result);
    });

    let (store, tenant, principal, permission, resource) = setup_flat_store();
    let engine = EngineBuilder::new(store)
        .cache(MemoryCache::new(8_192).with_ttl(Duration::from_secs(60)))
        .build();
    let warm = block_on(engine.authorize_ref(&tenant, &principal, &permission)).unwrap();
    assert_eq!(warm, Decision::Allow);
    benchmark_sync("authorize_flat_hot_cache", iterations, || {
        let result = block_on(engine.authorize_ref(&tenant, &principal, &permission)).unwrap();
        black_box(result);
    });

    benchmark_sync("scope_flat_hot_cache", iterations, || {
        let result = block_on(engine.scope_ref(&tenant, &principal, &resource)).unwrap();
        black_box(result);
    });

    let (store, tenant, principal, permission, _, depth) = setup_hierarchy_store(8);
    let engine = EngineBuilder::new(store)
        .enable_role_hierarchy(true)
        .max_inherit_depth(depth + 2)
        .build();
    benchmark_sync(
        "authorize_hierarchy_depth8_no_cache",
        iterations / 4,
        || {
            let result = block_on(engine.authorize_ref(&tenant, &principal, &permission)).unwrap();
            black_box(result);
        },
    );

    let threads = std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4);
    let iterations_per_thread = 50_000;

    let (store, tenant, principal, permission, _) = setup_flat_store();
    let engine_single_shard = Arc::new(
        EngineBuilder::new(store)
            .cache(
                MemoryCache::new(8_192)
                    .with_shards(1)
                    .with_ttl(Duration::from_secs(60)),
            )
            .build(),
    );
    let warm =
        block_on(engine_single_shard.authorize_ref(&tenant, &principal, &permission)).unwrap();
    assert_eq!(warm, Decision::Allow);
    let tenant_for_single_verify = tenant.clone();
    let principal_for_single_verify = principal.clone();

    let engine_single_for_parallel = Arc::clone(&engine_single_shard);
    benchmark_parallel(
        "authorize_flat_hot_cache_parallel_single_shard",
        threads,
        iterations_per_thread,
        move || {
            let engine = Arc::clone(&engine_single_for_parallel);
            let tenant = tenant.clone();
            let principal = principal.clone();
            let permission = permission.clone();
            Box::new(move || {
                let result =
                    block_on(engine.authorize_ref(&tenant, &principal, &permission)).unwrap();
                black_box(result);
            })
        },
    );

    let verify_resource = ResourceName::try_from("invoice").unwrap();
    let scope = block_on(engine_single_shard.scope_ref(
        &tenant_for_single_verify,
        &principal_for_single_verify,
        &verify_resource,
    ))
    .unwrap();
    assert!(matches!(scope, Scope::TenantOnly { .. }));

    let (store, tenant, principal, permission, _) = setup_flat_store();
    let shard_count = threads.min(16);
    let engine_sharded = Arc::new(
        EngineBuilder::new(store)
            .cache(
                MemoryCache::new(8_192)
                    .with_shards(shard_count)
                    .with_ttl(Duration::from_secs(60)),
            )
            .build(),
    );
    let warm = block_on(engine_sharded.authorize_ref(&tenant, &principal, &permission)).unwrap();
    assert_eq!(warm, Decision::Allow);

    let engine_sharded_for_parallel = Arc::clone(&engine_sharded);
    benchmark_parallel(
        "authorize_flat_hot_cache_parallel_sharded",
        threads,
        iterations_per_thread,
        move || {
            let engine = Arc::clone(&engine_sharded_for_parallel);
            let tenant = tenant.clone();
            let principal = principal.clone();
            let permission = permission.clone();
            Box::new(move || {
                let result =
                    block_on(engine.authorize_ref(&tenant, &principal, &permission)).unwrap();
                black_box(result);
            })
        },
    );
}

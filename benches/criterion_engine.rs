#![cfg(all(
    feature = "criterion-bench",
    feature = "memory-store",
    feature = "memory-cache"
))]

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use futures::executor::block_on;
use rs_tenant::{
    Decision, EngineBuilder, MemoryCache, MemoryStore, Permission, PrincipalId, ResourceName,
    RoleId, Scope, TenantId,
};
use std::time::Duration;

fn setup_flat_store() -> (MemoryStore, TenantId, PrincipalId, Permission, ResourceName) {
    let store = MemoryStore::new();
    let tenant = TenantId::try_from("tenant_bench").unwrap();
    let principal = PrincipalId::try_from("principal_bench").unwrap();
    let role = RoleId::try_from("role_reader").unwrap();
    let permission = Permission::try_from("invoice:read").unwrap();
    let resource = ResourceName::try_from("invoice").unwrap();

    store.set_tenant_active(tenant.clone(), true);
    store.set_principal_active(tenant.clone(), principal.clone(), true);
    store.add_principal_role(tenant.clone(), principal.clone(), role.clone());
    store.add_role_permission(tenant.clone(), role, permission.clone());

    (store, tenant, principal, permission, resource)
}

fn setup_hierarchy_store(depth: usize) -> (MemoryStore, TenantId, PrincipalId, Permission) {
    let store = MemoryStore::new();
    let tenant = TenantId::try_from("tenant_hierarchy_bench").unwrap();
    let principal = PrincipalId::try_from("principal_hierarchy_bench").unwrap();
    let permission = Permission::try_from("invoice:read").unwrap();

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

    (store, tenant, principal, permission)
}

fn setup_role_fanout_store(role_count: usize) -> (MemoryStore, TenantId, PrincipalId, Permission) {
    let store = MemoryStore::new();
    let tenant = TenantId::try_from("tenant_fanout_bench").unwrap();
    let principal = PrincipalId::try_from("principal_fanout_bench").unwrap();

    store.set_tenant_active(tenant.clone(), true);
    store.set_principal_active(tenant.clone(), principal.clone(), true);

    for i in 0..role_count {
        let role = RoleId::try_from(format!("role_{i}").as_str()).unwrap();
        let permission = Permission::try_from(format!("invoice_{i}:read").as_str()).unwrap();
        store.add_principal_role(tenant.clone(), principal.clone(), role.clone());
        store.add_role_permission(tenant.clone(), role, permission);
    }

    let required =
        Permission::try_from(format!("invoice_{}:read", role_count - 1).as_str()).unwrap();
    (store, tenant, principal, required)
}

fn bench_flat(c: &mut Criterion) {
    let mut group = c.benchmark_group("authorize_flat");
    group.sample_size(30);
    group.throughput(Throughput::Elements(1));

    let (store, tenant, principal, permission, resource) = setup_flat_store();
    let engine = EngineBuilder::new(store).build();
    group.bench_function("authorize_no_cache", |b| {
        b.iter(|| {
            let decision =
                block_on(engine.authorize(tenant.clone(), principal.clone(), permission.clone()))
                    .unwrap();
            black_box(decision);
        });
    });
    group.bench_function("scope_no_cache", |b| {
        b.iter(|| {
            let scope = block_on(engine.scope(tenant.clone(), principal.clone(), resource.clone()))
                .unwrap();
            black_box(scope);
        });
    });

    let (store, tenant, principal, permission, resource) = setup_flat_store();
    let cache = MemoryCache::new(8_192)
        .with_shards(1)
        .with_ttl(Duration::from_secs(60));
    let engine = EngineBuilder::new(store).cache(cache).build();
    assert_eq!(
        block_on(engine.authorize(tenant.clone(), principal.clone(), permission.clone())).unwrap(),
        Decision::Allow
    );
    group.bench_function("authorize_cache_single_shard", |b| {
        b.iter(|| {
            let decision =
                block_on(engine.authorize(tenant.clone(), principal.clone(), permission.clone()))
                    .unwrap();
            black_box(decision);
        });
    });
    group.bench_function("scope_cache_single_shard", |b| {
        b.iter(|| {
            let scope = block_on(engine.scope(tenant.clone(), principal.clone(), resource.clone()))
                .unwrap();
            black_box(scope);
        });
    });

    let (store, tenant, principal, permission, resource) = setup_flat_store();
    let cache = MemoryCache::new(8_192)
        .with_shards(8)
        .with_ttl(Duration::from_secs(60));
    let engine = EngineBuilder::new(store).cache(cache).build();
    assert_eq!(
        block_on(engine.authorize(tenant.clone(), principal.clone(), permission.clone())).unwrap(),
        Decision::Allow
    );
    group.bench_function("authorize_cache_sharded", |b| {
        b.iter(|| {
            let decision =
                block_on(engine.authorize(tenant.clone(), principal.clone(), permission.clone()))
                    .unwrap();
            black_box(decision);
        });
    });
    group.bench_function("scope_cache_sharded", |b| {
        b.iter(|| {
            let scope = block_on(engine.scope(tenant.clone(), principal.clone(), resource.clone()))
                .unwrap();
            black_box(scope);
        });
    });

    group.finish();
}

fn bench_hierarchy_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("authorize_hierarchy_depth");
    group.sample_size(30);
    group.throughput(Throughput::Elements(1));

    for depth in [1usize, 4, 8, 16] {
        let (store, tenant, principal, permission) = setup_hierarchy_store(depth);
        let engine = EngineBuilder::new(store)
            .enable_role_hierarchy(true)
            .max_inherit_depth(depth + 2)
            .build();
        let id = BenchmarkId::from_parameter(depth);
        group.bench_with_input(id, &depth, |b, _| {
            b.iter(|| {
                let decision = block_on(engine.authorize(
                    tenant.clone(),
                    principal.clone(),
                    permission.clone(),
                ))
                .unwrap();
                black_box(decision);
            });
        });
    }

    group.finish();
}

fn bench_role_fanout(c: &mut Criterion) {
    let mut group = c.benchmark_group("authorize_role_fanout");
    group.sample_size(30);
    group.throughput(Throughput::Elements(1));

    for role_count in [1usize, 8, 32, 128] {
        let (store, tenant, principal, required) = setup_role_fanout_store(role_count);
        let engine = EngineBuilder::new(store).build();

        let id = BenchmarkId::from_parameter(role_count);
        group.bench_with_input(id, &role_count, |b, _| {
            b.iter(|| {
                let decision =
                    block_on(engine.authorize(tenant.clone(), principal.clone(), required.clone()))
                        .unwrap();
                black_box(decision);
            });
        });
    }

    group.finish();
}

fn bench_scope(c: &mut Criterion) {
    let mut group = c.benchmark_group("scope_behavior");
    group.sample_size(30);
    group.throughput(Throughput::Elements(1));

    let (store, tenant, principal, permission, resource) = setup_flat_store();
    let engine = EngineBuilder::new(store)
        .cache(
            MemoryCache::new(8_192)
                .with_shards(8)
                .with_ttl(Duration::from_secs(60)),
        )
        .build();
    assert_eq!(
        block_on(engine.authorize(tenant.clone(), principal.clone(), permission.clone())).unwrap(),
        Decision::Allow
    );

    group.bench_function("scope_allow", |b| {
        b.iter(|| {
            let scope = block_on(engine.scope(tenant.clone(), principal.clone(), resource.clone()))
                .unwrap();
            black_box(scope);
        });
    });

    let denied_resource = ResourceName::try_from("customer").unwrap();
    group.bench_function("scope_deny", |b| {
        b.iter(|| {
            let scope =
                block_on(engine.scope(tenant.clone(), principal.clone(), denied_resource.clone()))
                    .unwrap();
            assert!(matches!(scope, Scope::None));
            black_box(scope);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_flat,
    bench_hierarchy_depth,
    bench_role_fanout,
    bench_scope
);
criterion_main!(benches);

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::cache::Cache;
use crate::permission::Permission;
use crate::types::{PrincipalId, RoleId, TenantId};

const SMALL_CACHE_SHARD_THRESHOLD: usize = 128;
const MAX_DEFAULT_SHARDS: usize = 16;

/// In-memory cache for effective permissions.
///
/// This cache uses sharded locking to reduce contention in concurrent workloads.
/// Each shard maintains its own LRU queue and TTL checks.
#[derive(Debug, Clone)]
pub struct MemoryCache {
    shards: Arc<Vec<Mutex<CacheState>>>,
    shard_capacities: Arc<Vec<usize>>,
    shard_count: usize,
    capacity: usize,
    ttl: Option<Duration>,
}

#[derive(Debug, Default)]
struct CacheState {
    entries: HashMap<CacheKey, CacheEntry>,
    order: VecDeque<CacheKey>,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct CacheKey {
    tenant: TenantId,
    principal: PrincipalId,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    perms: Vec<Permission>,
    updated_at: Instant,
}

impl MemoryCache {
    /// Creates a new cache with the given capacity.
    ///
    /// A capacity of zero disables caching.
    pub fn new(capacity: usize) -> Self {
        let shard_count = Self::default_shard_count(capacity);
        Self::build(capacity, shard_count)
    }

    /// Overrides shard count for lock sharding.
    ///
    /// This is useful for benchmarks and tuning. For small capacities we suggest
    /// keeping one shard to preserve strict global LRU behavior.
    pub fn with_shards(mut self, shards: usize) -> Self {
        let shard_count = Self::normalize_shard_count(self.capacity, shards);
        self.shards = Arc::new(Self::new_shards(shard_count));
        self.shard_capacities = Arc::new(Self::distribute_capacity(self.capacity, shard_count));
        self.shard_count = shard_count;
        self
    }

    /// Configures a time-to-live for cache entries.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    fn build(capacity: usize, shard_count: usize) -> Self {
        Self {
            shards: Arc::new(Self::new_shards(shard_count)),
            shard_capacities: Arc::new(Self::distribute_capacity(capacity, shard_count)),
            shard_count,
            capacity,
            ttl: None,
        }
    }

    fn default_shard_count(capacity: usize) -> usize {
        if capacity < SMALL_CACHE_SHARD_THRESHOLD {
            return 1;
        }
        let cpu_shards = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(MAX_DEFAULT_SHARDS);
        Self::normalize_shard_count(capacity, cpu_shards)
    }

    fn normalize_shard_count(capacity: usize, requested: usize) -> usize {
        if capacity == 0 {
            return 1;
        }
        requested.max(1).min(capacity)
    }

    fn new_shards(shard_count: usize) -> Vec<Mutex<CacheState>> {
        (0..shard_count)
            .map(|_| Mutex::new(CacheState::default()))
            .collect()
    }

    fn distribute_capacity(capacity: usize, shard_count: usize) -> Vec<usize> {
        if shard_count == 0 {
            return Vec::new();
        }

        let base = capacity / shard_count;
        let remainder = capacity % shard_count;

        (0..shard_count)
            .map(|idx| base + usize::from(idx < remainder))
            .collect()
    }

    fn key(tenant: &TenantId, principal: &PrincipalId) -> CacheKey {
        CacheKey {
            tenant: tenant.clone(),
            principal: principal.clone(),
        }
    }

    fn shard_index(&self, key: &CacheKey) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.shard_count
    }

    fn lock_shard(&self, shard_index: usize) -> MutexGuard<'_, CacheState> {
        match self.shards[shard_index].lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn remove_key(state: &mut CacheState, key: &CacheKey) {
        if state.entries.remove(key).is_some() {
            state.order.retain(|existing| existing != key);
        }
    }

    fn touch(state: &mut CacheState, key: &CacheKey) {
        state.order.retain(|existing| existing != key);
        state.order.push_back(key.clone());
    }

    fn is_expired(entry: &CacheEntry, ttl: Duration, now: Instant) -> bool {
        now.saturating_duration_since(entry.updated_at) > ttl
    }

    fn prune_expired(state: &mut CacheState, ttl: Duration, now: Instant) {
        state
            .entries
            .retain(|_, entry| !Self::is_expired(entry, ttl, now));
        state.order.retain(|key| state.entries.contains_key(key));
    }

    fn evict_if_needed(state: &mut CacheState, shard_capacity: usize) {
        if shard_capacity == 0 {
            state.entries.clear();
            state.order.clear();
            return;
        }

        while state.entries.len() > shard_capacity {
            if let Some(key) = state.order.pop_front() {
                state.entries.remove(&key);
            } else {
                break;
            }
        }
    }

    fn invalidate_tenant_inner(state: &mut CacheState, tenant: &TenantId) {
        let keys: Vec<CacheKey> = state
            .entries
            .keys()
            .filter(|key| &key.tenant == tenant)
            .cloned()
            .collect();
        for key in keys {
            Self::remove_key(state, &key);
        }
    }
}

#[async_trait]
impl Cache for MemoryCache {
    async fn get_permissions(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> Option<Vec<Permission>> {
        if self.capacity == 0 {
            return None;
        }

        let key = Self::key(tenant, principal);
        let now = Instant::now();
        let shard_index = self.shard_index(&key);
        let mut guard = self.lock_shard(shard_index);

        if let Some(ttl) = self.ttl
            && let Some(entry) = guard.entries.get(&key)
            && Self::is_expired(entry, ttl, now)
        {
            Self::remove_key(&mut guard, &key);
            return None;
        }

        let perms = guard.entries.get(&key).map(|entry| entry.perms.clone());
        if perms.is_some() {
            Self::touch(&mut guard, &key);
        }
        perms
    }

    async fn set_permissions(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
        perms: Vec<Permission>,
    ) {
        if self.capacity == 0 {
            return;
        }

        let key = Self::key(tenant, principal);
        let now = Instant::now();
        let shard_index = self.shard_index(&key);
        let mut guard = self.lock_shard(shard_index);

        if let Some(ttl) = self.ttl {
            Self::prune_expired(&mut guard, ttl, now);
        }

        guard.entries.insert(
            key.clone(),
            CacheEntry {
                perms,
                updated_at: now,
            },
        );
        Self::touch(&mut guard, &key);
        let shard_capacity = self.shard_capacities[shard_index];
        Self::evict_if_needed(&mut guard, shard_capacity);
    }

    async fn invalidate_principal(&self, tenant: &TenantId, principal: &PrincipalId) {
        let key = Self::key(tenant, principal);
        let shard_index = self.shard_index(&key);
        let mut guard = self.lock_shard(shard_index);
        Self::remove_key(&mut guard, &key);
    }

    async fn invalidate_role(&self, tenant: &TenantId, _role: &RoleId) {
        // Role-to-principal relationships are unknown here, so we invalidate the tenant scope.
        for shard_index in 0..self.shard_count {
            let mut guard = self.lock_shard(shard_index);
            Self::invalidate_tenant_inner(&mut guard, tenant);
        }
    }

    async fn invalidate_tenant(&self, tenant: &TenantId) {
        for shard_index in 0..self.shard_count {
            let mut guard = self.lock_shard(shard_index);
            Self::invalidate_tenant_inner(&mut guard, tenant);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    fn tenant() -> TenantId {
        TenantId::try_from("tenant_1").unwrap()
    }

    fn principal(value: &str) -> PrincipalId {
        PrincipalId::try_from(value).unwrap()
    }

    fn perm(value: &str) -> Permission {
        Permission::try_from(value).unwrap()
    }

    #[test]
    fn lru_should_evict_least_recently_used() {
        let cache = MemoryCache::new(2);
        let tenant = tenant();
        let principal_a = principal("user_a");
        let principal_b = principal("user_b");
        let principal_c = principal("user_c");

        block_on(cache.set_permissions(&tenant, &principal_a, vec![perm("invoice:read")]));
        block_on(cache.set_permissions(&tenant, &principal_b, vec![perm("invoice:write")]));
        let _ = block_on(cache.get_permissions(&tenant, &principal_a));
        block_on(cache.set_permissions(&tenant, &principal_c, vec![perm("invoice:delete")]));

        assert!(block_on(cache.get_permissions(&tenant, &principal_b)).is_none());
        assert!(block_on(cache.get_permissions(&tenant, &principal_a)).is_some());
        assert!(block_on(cache.get_permissions(&tenant, &principal_c)).is_some());
    }

    #[test]
    fn ttl_should_expire_entries() {
        let cache = MemoryCache::new(1).with_ttl(Duration::from_millis(10));
        let tenant = tenant();
        let principal = principal("user_a");

        block_on(cache.set_permissions(&tenant, &principal, vec![perm("invoice:read")]));
        std::thread::sleep(Duration::from_millis(20));

        assert!(block_on(cache.get_permissions(&tenant, &principal)).is_none());
    }

    #[test]
    fn invalidate_tenant_should_clear_entries() {
        let cache = MemoryCache::new(2);
        let tenant = tenant();
        let principal_a = principal("user_a");
        let principal_b = principal("user_b");

        block_on(cache.set_permissions(&tenant, &principal_a, vec![perm("invoice:read")]));
        block_on(cache.set_permissions(&tenant, &principal_b, vec![perm("invoice:write")]));
        block_on(cache.invalidate_tenant(&tenant));

        assert!(block_on(cache.get_permissions(&tenant, &principal_a)).is_none());
        assert!(block_on(cache.get_permissions(&tenant, &principal_b)).is_none());
    }

    #[test]
    fn cache_should_recover_from_poisoned_lock() {
        let cache = MemoryCache::new(1);
        let shards = Arc::clone(&cache.shards);
        let _ = std::thread::spawn(move || {
            let _guard = shards[0].lock().unwrap();
            panic!("poison cache lock");
        })
        .join();

        let tenant = tenant();
        let principal = principal("user_a");
        block_on(cache.set_permissions(&tenant, &principal, vec![perm("invoice:read")]));

        assert!(block_on(cache.get_permissions(&tenant, &principal)).is_some());
    }

    #[test]
    fn with_shards_should_enable_sharding_for_large_cache() {
        let cache = MemoryCache::new(1024).with_shards(8);
        assert_eq!(cache.shard_count, 8);
        assert_eq!(cache.shard_capacities.iter().sum::<usize>(), 1024);
    }
}

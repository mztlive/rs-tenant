use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::cache::{Cache, EffectiveGrant};
use crate::ids::{PrincipalId, RoleId, TenantId};

const SMALL_CACHE_SHARD_THRESHOLD: usize = 128;
const MAX_DEFAULT_SHARDS: usize = 16;

/// In-memory cache for effective grants.
#[derive(Debug, Clone)]
pub struct MemoryCache {
    shards: Arc<Vec<RwLock<CacheState>>>,
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
    config_signature: String,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    grants: Vec<EffectiveGrant>,
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
    pub fn with_shards(mut self, shards: usize) -> Self {
        let shard_count = Self::normalize_shards(self.capacity, shards);
        self.shards = Arc::new(Self::new_shards(shard_count));
        self.shard_capacities = Arc::new(Self::shard_capacities(self.capacity, shard_count));
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
            shard_capacities: Arc::new(Self::shard_capacities(capacity, shard_count)),
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
        Self::normalize_shards(capacity, cpu_shards)
    }

    fn normalize_shards(capacity: usize, requested: usize) -> usize {
        if capacity == 0 {
            return 1;
        }
        requested.max(1).min(capacity)
    }

    fn new_shards(shard_count: usize) -> Vec<RwLock<CacheState>> {
        (0..shard_count)
            .map(|_| RwLock::new(CacheState::default()))
            .collect()
    }

    fn shard_capacities(capacity: usize, shard_count: usize) -> Vec<usize> {
        let base = capacity / shard_count;
        let remainder = capacity % shard_count;
        (0..shard_count)
            .map(|idx| base + usize::from(idx < remainder))
            .collect()
    }

    fn key(tenant: &TenantId, principal: &PrincipalId, config_signature: &str) -> CacheKey {
        CacheKey {
            tenant: tenant.clone(),
            principal: principal.clone(),
            config_signature: config_signature.to_string(),
        }
    }

    fn shard_index(&self, key: &CacheKey) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.shard_count
    }

    fn read_shard(&self, shard_index: usize) -> RwLockReadGuard<'_, CacheState> {
        match self.shards[shard_index].read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn write_shard(&self, shard_index: usize) -> RwLockWriteGuard<'_, CacheState> {
        match self.shards[shard_index].write() {
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
        if state.order.back().is_some_and(|last| last == key) {
            return;
        }
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

    fn remove_tenant_entries(state: &mut CacheState, tenant: &TenantId) {
        state.entries.retain(|key, _| &key.tenant != tenant);
        state.order.retain(|key| state.entries.contains_key(key));
    }
}

#[async_trait]
impl Cache for MemoryCache {
    async fn get_effective_grants(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
        config_signature: &str,
    ) -> Option<Vec<EffectiveGrant>> {
        if self.capacity == 0 {
            return None;
        }

        let key = Self::key(tenant, principal, config_signature);
        let now = Instant::now();
        let shard_index = self.shard_index(&key);
        {
            let guard = self.read_shard(shard_index);
            if let Some(entry) = guard.entries.get(&key) {
                if let Some(ttl) = self.ttl
                    && Self::is_expired(entry, ttl, now)
                {
                    // A write lock below removes the expired entry.
                } else if guard.order.back().is_some_and(|last| last == &key) {
                    return Some(entry.grants.clone());
                }
            } else {
                return None;
            }
        }

        let mut guard = self.write_shard(shard_index);
        if let Some(ttl) = self.ttl
            && let Some(entry) = guard.entries.get(&key)
            && Self::is_expired(entry, ttl, now)
        {
            Self::remove_key(&mut guard, &key);
            return None;
        }
        let grants = guard.entries.get(&key).map(|entry| entry.grants.clone());
        if grants.is_some() {
            Self::touch(&mut guard, &key);
        }
        grants
    }

    async fn set_effective_grants(
        &self,
        tenant: &TenantId,
        principal: &PrincipalId,
        config_signature: &str,
        grants: Vec<EffectiveGrant>,
    ) {
        if self.capacity == 0 {
            return;
        }

        let key = Self::key(tenant, principal, config_signature);
        let now = Instant::now();
        let shard_index = self.shard_index(&key);
        let mut guard = self.write_shard(shard_index);

        if let Some(ttl) = self.ttl {
            Self::prune_expired(&mut guard, ttl, now);
        }

        guard.entries.insert(
            key.clone(),
            CacheEntry {
                grants,
                updated_at: now,
            },
        );
        Self::touch(&mut guard, &key);
        Self::evict_if_needed(&mut guard, self.shard_capacities[shard_index]);
    }

    async fn invalidate_principal(&self, tenant: &TenantId, principal: &PrincipalId) {
        for shard_index in 0..self.shard_count {
            let mut guard = self.write_shard(shard_index);
            guard
                .entries
                .retain(|key, _| &key.tenant != tenant || &key.principal != principal);
            let retained_order = guard
                .order
                .iter()
                .filter(|key| guard.entries.contains_key(*key))
                .cloned()
                .collect();
            guard.order = retained_order;
        }
    }

    async fn invalidate_role(&self, tenant: &TenantId, _role: &RoleId) {
        self.invalidate_tenant(tenant).await;
    }

    async fn invalidate_tenant(&self, tenant: &TenantId) {
        for shard_index in 0..self.shard_count {
            let mut guard = self.write_shard(shard_index);
            Self::remove_tenant_entries(&mut guard, tenant);
        }
    }

    async fn invalidate_all(&self) {
        for shard_index in 0..self.shard_count {
            let mut guard = self.write_shard(shard_index);
            guard.entries.clear();
            guard.order.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryCache;
    use crate::cache::{Cache, EffectiveGrant};
    use crate::{GrantScope, Permission, PrincipalId, RoleId, TenantId};
    use futures::executor::block_on;
    use std::time::Duration;

    fn ids(suffix: &str) -> (TenantId, PrincipalId, RoleId) {
        (
            TenantId::parse(format!("tenant_{suffix}")).expect("tenant"),
            PrincipalId::parse(format!("principal_{suffix}")).expect("principal"),
            RoleId::parse(format!("role_{suffix}")).expect("role"),
        )
    }

    fn grant(role: RoleId, permission: &str) -> EffectiveGrant {
        EffectiveGrant::new(
            role,
            Permission::parse(permission).expect("permission"),
            GrantScope::tenant(),
        )
    }

    #[test]
    fn memory_cache_should_return_hot_entry() {
        let (tenant, principal, role) = ids("hot");
        let cache = MemoryCache::new(8);
        let grants = vec![grant(role, "invoice:read")];

        block_on(cache.set_effective_grants(&tenant, &principal, "a", grants.clone()));
        let cached = block_on(cache.get_effective_grants(&tenant, &principal, "a"));

        assert_eq!(cached, Some(grants));
    }

    #[test]
    fn memory_cache_should_isolate_config_signatures() {
        let (tenant, principal, role) = ids("signature");
        let cache = MemoryCache::new(8);
        block_on(cache.set_effective_grants(
            &tenant,
            &principal,
            "wildcard-off",
            vec![grant(role, "invoice:*")],
        ));

        let cached = block_on(cache.get_effective_grants(&tenant, &principal, "wildcard-on"));

        assert!(cached.is_none());
    }

    #[test]
    fn memory_cache_should_invalidate_principal() {
        let (tenant, principal, role) = ids("invalidate_principal");
        let other_principal = PrincipalId::parse("principal_other").expect("principal");
        let cache = MemoryCache::new(8);
        block_on(cache.set_effective_grants(
            &tenant,
            &principal,
            "a",
            vec![grant(role.clone(), "invoice:read")],
        ));
        block_on(cache.set_effective_grants(
            &tenant,
            &other_principal,
            "a",
            vec![grant(role, "invoice:read")],
        ));

        block_on(cache.invalidate_principal(&tenant, &principal));

        assert!(block_on(cache.get_effective_grants(&tenant, &principal, "a")).is_none());
        assert!(block_on(cache.get_effective_grants(&tenant, &other_principal, "a")).is_some());
    }

    #[test]
    fn memory_cache_invalidate_role_should_fallback_to_tenant() {
        let (tenant, principal, role) = ids("invalidate_role");
        let other_principal = PrincipalId::parse("principal_other").expect("principal");
        let cache = MemoryCache::new(8);
        block_on(cache.set_effective_grants(
            &tenant,
            &principal,
            "a",
            vec![grant(role.clone(), "invoice:read")],
        ));
        block_on(cache.set_effective_grants(
            &tenant,
            &other_principal,
            "a",
            vec![grant(role.clone(), "order:read")],
        ));

        block_on(cache.invalidate_role(&tenant, &role));

        assert!(block_on(cache.get_effective_grants(&tenant, &principal, "a")).is_none());
        assert!(block_on(cache.get_effective_grants(&tenant, &other_principal, "a")).is_none());
    }

    #[test]
    fn memory_cache_should_expire_by_ttl() {
        let (tenant, principal, role) = ids("ttl");
        let cache = MemoryCache::new(8).with_ttl(Duration::from_nanos(1));
        block_on(cache.set_effective_grants(
            &tenant,
            &principal,
            "a",
            vec![grant(role, "invoice:read")],
        ));

        std::thread::sleep(Duration::from_millis(1));

        assert!(block_on(cache.get_effective_grants(&tenant, &principal, "a")).is_none());
    }

    #[test]
    fn memory_cache_should_evict_lru_entry() {
        let tenant = TenantId::parse("tenant_lru").expect("tenant");
        let role = RoleId::parse("role_lru").expect("role");
        let a = PrincipalId::parse("principal_a").expect("principal");
        let b = PrincipalId::parse("principal_b").expect("principal");
        let c = PrincipalId::parse("principal_c").expect("principal");
        let cache = MemoryCache::new(2);
        block_on(cache.set_effective_grants(&tenant, &a, "a", vec![grant(role.clone(), "a:read")]));
        block_on(cache.set_effective_grants(&tenant, &b, "a", vec![grant(role.clone(), "b:read")]));
        assert!(block_on(cache.get_effective_grants(&tenant, &a, "a")).is_some());
        block_on(cache.set_effective_grants(&tenant, &c, "a", vec![grant(role, "c:read")]));

        assert!(block_on(cache.get_effective_grants(&tenant, &a, "a")).is_some());
        assert!(block_on(cache.get_effective_grants(&tenant, &b, "a")).is_none());
        assert!(block_on(cache.get_effective_grants(&tenant, &c, "a")).is_some());
    }
}

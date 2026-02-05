use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::cache::Cache;
use crate::permission::Permission;
use crate::types::{PrincipalId, RoleId, TenantId};

/// In-memory cache for effective permissions.
///
/// This is a simple LRU cache with optional TTL. It is intended for tests
/// and small deployments where a process-local cache is sufficient.
#[derive(Debug, Clone)]
pub struct MemoryCache {
    inner: Arc<Mutex<CacheState>>,
    capacity: usize,
    ttl: Option<Duration>,
}

#[derive(Debug)]
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
        Self {
            inner: Arc::new(Mutex::new(CacheState {
                entries: HashMap::new(),
                order: VecDeque::new(),
            })),
            capacity,
            ttl: None,
        }
    }

    /// Configures a time-to-live for cache entries.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    fn key(tenant: &TenantId, principal: &PrincipalId) -> CacheKey {
        CacheKey {
            tenant: tenant.clone(),
            principal: principal.clone(),
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

    fn evict_if_needed(state: &mut CacheState, capacity: usize) {
        if capacity == 0 {
            state.entries.clear();
            state.order.clear();
            return;
        }

        while state.entries.len() > capacity {
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
        let mut guard = self.inner.lock().expect("poisoned lock");

        if let Some(ttl) = self.ttl {
            if let Some(entry) = guard.entries.get(&key) {
                if Self::is_expired(entry, ttl, now) {
                    Self::remove_key(&mut guard, &key);
                    return None;
                }
            }
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
        let mut guard = self.inner.lock().expect("poisoned lock");

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
        Self::evict_if_needed(&mut guard, self.capacity);
    }

    async fn invalidate_principal(&self, tenant: &TenantId, principal: &PrincipalId) {
        let key = Self::key(tenant, principal);
        let mut guard = self.inner.lock().expect("poisoned lock");
        Self::remove_key(&mut guard, &key);
    }

    async fn invalidate_role(&self, tenant: &TenantId, _role: &RoleId) {
        // Role-to-principal relationships are unknown here, so we invalidate the tenant scope.
        let mut guard = self.inner.lock().expect("poisoned lock");
        Self::invalidate_tenant_inner(&mut guard, tenant);
    }

    async fn invalidate_tenant(&self, tenant: &TenantId) {
        let mut guard = self.inner.lock().expect("poisoned lock");
        Self::invalidate_tenant_inner(&mut guard, tenant);
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
}

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::cache::{Cache, EffectiveGrant};
use crate::ids::{PrincipalId, RoleId, TenantId};

const SMALL_CACHE_SHARD_THRESHOLD: usize = 128;
const MAX_DEFAULT_SHARDS: usize = 16;

/// 有效授权的内存缓存。
#[derive(Debug, Clone)]
pub struct MemoryCache {
    shards: Arc<Vec<RwLock<CacheState>>>,
    shard_capacities: Arc<Vec<usize>>,
    shard_count: usize,
    capacity: usize,
    ttl: Option<Duration>,
}

/// 单个分片内的缓存状态。
#[derive(Debug, Default)]
struct CacheState {
    entries: HashMap<CacheKey, CacheEntry>,
    order: VecDeque<CacheKey>,
}

/// 缓存条目的唯一键。
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct CacheKey {
    tenant: TenantId,
    principal: PrincipalId,
    config_signature: String,
}

/// 缓存条目及其更新时间。
#[derive(Debug, Clone)]
struct CacheEntry {
    grants: Vec<EffectiveGrant>,
    updated_at: Instant,
}

impl MemoryCache {
    /// 使用给定容量创建缓存。
    ///
    /// 容量为零时禁用缓存。
    pub fn new(capacity: usize) -> Self {
        let shard_count = Self::default_shard_count(capacity);
        Self::build(capacity, shard_count)
    }

    /// 覆盖用于锁分片的分片数量。
    pub fn with_shards(mut self, shards: usize) -> Self {
        let shard_count = Self::normalize_shards(self.capacity, shards);
        self.shards = Arc::new(Self::new_shards(shard_count));
        self.shard_capacities = Arc::new(Self::shard_capacities(self.capacity, shard_count));
        self.shard_count = shard_count;
        self
    }

    /// 配置缓存条目的存活时间。
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// 使用容量和分片数量构建缓存实例。
    fn build(capacity: usize, shard_count: usize) -> Self {
        Self {
            shards: Arc::new(Self::new_shards(shard_count)),
            shard_capacities: Arc::new(Self::shard_capacities(capacity, shard_count)),
            shard_count,
            capacity,
            ttl: None,
        }
    }

    /// 根据容量和系统并行度选择默认分片数量。
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

    /// 将请求的分片数量规整到有效范围。
    fn normalize_shards(capacity: usize, requested: usize) -> usize {
        if capacity == 0 {
            return 1;
        }
        requested.max(1).min(capacity)
    }

    /// 创建指定数量的缓存分片。
    fn new_shards(shard_count: usize) -> Vec<RwLock<CacheState>> {
        (0..shard_count)
            .map(|_| RwLock::new(CacheState::default()))
            .collect()
    }

    /// 按总容量计算每个分片的容量。
    fn shard_capacities(capacity: usize, shard_count: usize) -> Vec<usize> {
        let base = capacity / shard_count;
        let remainder = capacity % shard_count;
        (0..shard_count)
            .map(|idx| base + usize::from(idx < remainder))
            .collect()
    }

    /// 构造缓存键。
    fn key(tenant: &TenantId, principal: &PrincipalId, config_signature: &str) -> CacheKey {
        CacheKey {
            tenant: tenant.clone(),
            principal: principal.clone(),
            config_signature: config_signature.to_string(),
        }
    }

    /// 根据缓存键定位分片下标。
    fn shard_index(&self, key: &CacheKey) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.shard_count
    }

    /// 读取指定分片，并在锁中毒时恢复内部值。
    fn read_shard(&self, shard_index: usize) -> RwLockReadGuard<'_, CacheState> {
        match self.shards[shard_index].read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// 写入指定分片，并在锁中毒时恢复内部值。
    fn write_shard(&self, shard_index: usize) -> RwLockWriteGuard<'_, CacheState> {
        match self.shards[shard_index].write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// 从状态中删除指定缓存键及其 LRU 顺序记录。
    fn remove_key(state: &mut CacheState, key: &CacheKey) {
        if state.entries.remove(key).is_some() {
            state.order.retain(|existing| existing != key);
        }
    }

    /// 将缓存键移动到 LRU 队列尾部。
    fn touch(state: &mut CacheState, key: &CacheKey) {
        if state.order.back().is_some_and(|last| last == key) {
            return;
        }
        state.order.retain(|existing| existing != key);
        state.order.push_back(key.clone());
    }

    /// 判断缓存条目是否超过存活时间。
    fn is_expired(entry: &CacheEntry, ttl: Duration, now: Instant) -> bool {
        now.saturating_duration_since(entry.updated_at) > ttl
    }

    /// 清理已经过期的缓存条目。
    fn prune_expired(state: &mut CacheState, ttl: Duration, now: Instant) {
        state
            .entries
            .retain(|_, entry| !Self::is_expired(entry, ttl, now));
        state.order.retain(|key| state.entries.contains_key(key));
    }

    /// 在分片容量超限时淘汰最久未使用条目。
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

    /// 删除某个租户下的所有缓存条目。
    fn remove_tenant_entries(state: &mut CacheState, tenant: &TenantId) {
        state.entries.retain(|key, _| &key.tenant != tenant);
        state.order.retain(|key| state.entries.contains_key(key));
    }
}

#[async_trait]
impl Cache for MemoryCache {
    /// 从缓存读取有效授权，并在必要时刷新 LRU 顺序。
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
                    // 下方的写锁会删除过期条目。
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

    /// 写入有效授权并按分片容量执行淘汰。
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

    /// 失效某个租户主体的所有缓存条目。
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

    /// 角色级缓存失效退化为租户级缓存失效。
    async fn invalidate_role(&self, tenant: &TenantId, _role: &RoleId) {
        self.invalidate_tenant(tenant).await;
    }

    /// 失效某个租户的所有缓存条目。
    async fn invalidate_tenant(&self, tenant: &TenantId) {
        for shard_index in 0..self.shard_count {
            let mut guard = self.write_shard(shard_index);
            Self::remove_tenant_entries(&mut guard, tenant);
        }
    }

    /// 清空所有分片中的缓存条目。
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

    /// 按后缀构造测试标识符。
    fn ids(suffix: &str) -> (TenantId, PrincipalId, RoleId) {
        (
            TenantId::parse(format!("tenant_{suffix}")).expect("tenant"),
            PrincipalId::parse(format!("principal_{suffix}")).expect("principal"),
            RoleId::parse(format!("role_{suffix}")).expect("role"),
        )
    }

    /// 构造租户级测试授权。
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

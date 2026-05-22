use crate::error::{Error, Result};
use crate::{ScopePath, ScopeRoots, TenantId};
use std::collections::{BTreeMap, BTreeSet};

/// Scope granted by a platform role assignment.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum PlatformGrantScope {
    /// Grant for platform-owned resources only.
    Platform,
    /// Grant for every tenant and every tenant path.
    AllTenants,
    /// Grant for an explicit set of tenants.
    Tenants(TenantSet),
    /// Grant for explicit roots within explicit tenants.
    TenantPaths(TenantScopeRoots),
}

impl PlatformGrantScope {
    /// Creates a platform-only grant scope.
    pub fn platform() -> Self {
        Self::Platform
    }

    /// Creates an all-tenants grant scope.
    pub fn all_tenants() -> Self {
        Self::AllTenants
    }

    /// Creates a tenant-set grant scope.
    pub fn tenants(tenants: Vec<TenantId>) -> Result<Self> {
        TenantSet::new(tenants).map(Self::Tenants)
    }

    /// Creates a tenant-path grant scope.
    pub fn tenant_paths(entries: Vec<TenantScopedRoots>) -> Result<Self> {
        TenantScopeRoots::new(entries).map(Self::TenantPaths)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for PlatformGrantScope {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum PlatformGrantScopeWire<'a> {
            Platform,
            AllTenants,
            Tenants { tenants: &'a TenantSet },
            TenantPaths { entries: &'a TenantScopeRoots },
        }

        match self {
            Self::Platform => PlatformGrantScopeWire::Platform.serialize(serializer),
            Self::AllTenants => PlatformGrantScopeWire::AllTenants.serialize(serializer),
            Self::Tenants(tenants) => {
                PlatformGrantScopeWire::Tenants { tenants }.serialize(serializer)
            }
            Self::TenantPaths(entries) => {
                PlatformGrantScopeWire::TenantPaths { entries }.serialize(serializer)
            }
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PlatformGrantScope {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum PlatformGrantScopeWire {
            Platform,
            AllTenants,
            Tenants { tenants: TenantSet },
            TenantPaths { entries: TenantScopeRoots },
        }

        Ok(match PlatformGrantScopeWire::deserialize(deserializer)? {
            PlatformGrantScopeWire::Platform => Self::Platform,
            PlatformGrantScopeWire::AllTenants => Self::AllTenants,
            PlatformGrantScopeWire::Tenants { tenants } => Self::Tenants(tenants),
            PlatformGrantScopeWire::TenantPaths { entries } => Self::TenantPaths(entries),
        })
    }
}

/// Non-empty, de-duplicated set of tenant identifiers.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct TenantSet {
    tenants: Vec<TenantId>,
}

impl TenantSet {
    /// Creates a non-empty tenant set.
    pub fn new(tenants: Vec<TenantId>) -> Result<Self> {
        if tenants.is_empty() {
            return Err(Error::InvalidScope(
                "tenant set must not be empty".to_string(),
            ));
        }
        Ok(Self {
            tenants: tenants
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        })
    }

    /// Returns tenants as a slice.
    pub fn as_slice(&self) -> &[TenantId] {
        &self.tenants
    }

    /// Consumes this set and returns the de-duplicated tenants.
    pub fn into_vec(self) -> Vec<TenantId> {
        self.tenants
    }

    /// Returns whether this set contains `tenant`.
    pub fn contains(&self, tenant: &TenantId) -> bool {
        self.tenants.iter().any(|entry| entry == tenant)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for TenantSet {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.tenants.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for TenantSet {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let tenants = Vec::<TenantId>::deserialize(deserializer)?;
        Self::new(tenants).map_err(serde::de::Error::custom)
    }
}

/// Scope roots for a single tenant.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TenantScopedRoots {
    /// Tenant covered by these roots.
    pub tenant: TenantId,
    /// Compacted roots covered within the tenant.
    pub roots: ScopeRoots,
}

impl TenantScopedRoots {
    /// Creates tenant-scoped roots.
    pub fn new(tenant: TenantId, roots: ScopeRoots) -> Self {
        Self { tenant, roots }
    }
}

/// Non-empty, compacted tenant-scoped root entries.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct TenantScopeRoots {
    entries: Vec<TenantScopedRoots>,
}

impl TenantScopeRoots {
    /// Creates tenant-scoped roots, merging entries for the same tenant.
    pub fn new(entries: Vec<TenantScopedRoots>) -> Result<Self> {
        if entries.is_empty() {
            return Err(Error::InvalidScope(
                "tenant scope roots must not be empty".to_string(),
            ));
        }

        let mut by_tenant: BTreeMap<TenantId, Vec<ScopePath>> = BTreeMap::new();
        for entry in entries {
            by_tenant
                .entry(entry.tenant)
                .or_default()
                .extend(entry.roots.into_vec());
        }

        let mut compacted = Vec::with_capacity(by_tenant.len());
        for (tenant, roots) in by_tenant {
            compacted.push(TenantScopedRoots {
                tenant,
                roots: ScopeRoots::new(roots)?,
            });
        }
        Ok(Self { entries: compacted })
    }

    /// Returns the compacted entries.
    pub fn as_slice(&self) -> &[TenantScopedRoots] {
        &self.entries
    }

    /// Consumes this wrapper and returns the compacted entries.
    pub fn into_vec(self) -> Vec<TenantScopedRoots> {
        self.entries
    }

    /// Returns whether these roots cover `target` within `tenant`.
    pub fn allows_path(&self, tenant: &TenantId, target: &ScopePath) -> bool {
        self.entries
            .iter()
            .find(|entry| &entry.tenant == tenant)
            .is_some_and(|entry| {
                entry
                    .roots
                    .as_slice()
                    .iter()
                    .any(|root| root.allows(target))
            })
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for TenantScopeRoots {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.entries.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for TenantScopeRoots {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let entries = Vec::<TenantScopedRoots>::deserialize(deserializer)?;
        Self::new(entries).map_err(serde::de::Error::custom)
    }
}

/// Merged tenant data access scope for a platform permission query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TenantDataAccessScope {
    /// No matching tenant data access.
    None,
    /// Every tenant and every tenant path is accessible.
    AllTenants,
    /// Explicit tenants are accessible at tenant level.
    Tenants {
        /// De-duplicated tenants.
        tenants: Vec<TenantId>,
    },
    /// Explicit tenant path roots are accessible.
    TenantPaths {
        /// Compacted tenant path entries.
        entries: Vec<TenantScopedRoots>,
    },
}

#[cfg(feature = "serde")]
impl serde::Serialize for TenantDataAccessScope {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum TenantDataAccessScopeWire<'a> {
            None,
            AllTenants,
            Tenants { tenants: &'a [TenantId] },
            TenantPaths { entries: &'a [TenantScopedRoots] },
        }

        match self {
            Self::None => TenantDataAccessScopeWire::None.serialize(serializer),
            Self::AllTenants => TenantDataAccessScopeWire::AllTenants.serialize(serializer),
            Self::Tenants { tenants } => {
                TenantDataAccessScopeWire::Tenants { tenants }.serialize(serializer)
            }
            Self::TenantPaths { entries } => {
                TenantDataAccessScopeWire::TenantPaths { entries }.serialize(serializer)
            }
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for TenantDataAccessScope {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum TenantDataAccessScopeWire {
            None,
            AllTenants,
            Tenants { tenants: Vec<TenantId> },
            TenantPaths { entries: Vec<TenantScopedRoots> },
        }

        match TenantDataAccessScopeWire::deserialize(deserializer)? {
            TenantDataAccessScopeWire::None => Ok(Self::None),
            TenantDataAccessScopeWire::AllTenants => Ok(Self::AllTenants),
            TenantDataAccessScopeWire::Tenants { tenants } => TenantSet::new(tenants)
                .map(|set| Self::Tenants {
                    tenants: set.into_vec(),
                })
                .map_err(serde::de::Error::custom),
            TenantDataAccessScopeWire::TenantPaths { entries } => TenantScopeRoots::new(entries)
                .map(|entries| Self::TenantPaths {
                    entries: entries.into_vec(),
                })
                .map_err(serde::de::Error::custom),
        }
    }
}

impl TenantDataAccessScope {
    pub(crate) fn merge(scopes: impl IntoIterator<Item = PlatformGrantScope>) -> Result<Self> {
        let mut tenants = Vec::new();
        let mut path_entries = Vec::new();

        for scope in scopes {
            match scope {
                PlatformGrantScope::Platform => {}
                PlatformGrantScope::AllTenants => return Ok(Self::AllTenants),
                PlatformGrantScope::Tenants(set) => tenants.extend(set.into_vec()),
                PlatformGrantScope::TenantPaths(entries) => {
                    path_entries.extend(entries.into_vec());
                }
            }
        }

        if !tenants.is_empty() && !path_entries.is_empty() {
            return Err(Error::InvalidScope(
                "tenant data scope must not mix tenant-level and path-level grants".to_string(),
            ));
        }

        if !tenants.is_empty() {
            return TenantSet::new(tenants).map(|set| Self::Tenants {
                tenants: set.into_vec(),
            });
        }

        if path_entries.is_empty() {
            Ok(Self::None)
        } else {
            TenantScopeRoots::new(path_entries).map(|entries| Self::TenantPaths {
                entries: entries.into_vec(),
            })
        }
    }

    /// Returns whether tenant-level access is allowed.
    pub fn allows_tenant(&self, tenant: &TenantId) -> bool {
        match self {
            Self::AllTenants => true,
            Self::Tenants { tenants } => tenants.iter().any(|entry| entry == tenant),
            Self::None | Self::TenantPaths { .. } => false,
        }
    }

    /// Returns whether path-level access is allowed.
    pub fn allows_path(&self, tenant: &TenantId, target: &ScopePath) -> bool {
        match self {
            Self::AllTenants => true,
            Self::Tenants { tenants } => tenants.iter().any(|entry| entry == tenant),
            Self::TenantPaths { entries } => entries
                .iter()
                .find(|entry| &entry.tenant == tenant)
                .is_some_and(|entry| {
                    entry
                        .roots
                        .as_slice()
                        .iter()
                        .any(|root| root.allows(target))
                }),
            Self::None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PlatformGrantScope, TenantDataAccessScope, TenantScopeRoots, TenantScopedRoots, TenantSet,
    };
    use crate::{ScopePath, ScopeRoots, TenantId};

    fn tenant(value: &str) -> TenantId {
        TenantId::parse(value).expect("tenant")
    }

    fn roots(values: &[&str]) -> ScopeRoots {
        ScopeRoots::new(
            values
                .iter()
                .map(|value| ScopePath::parse(value).expect("path"))
                .collect(),
        )
        .expect("roots")
    }

    #[test]
    fn tenant_set_should_reject_empty_values() {
        let err = TenantSet::new(vec![]).expect_err("must reject");
        assert!(err.to_string().contains("tenant set"));
    }

    #[test]
    fn tenant_set_should_deduplicate_and_order_tenants() {
        let set = TenantSet::new(vec![
            tenant("tenant_b"),
            tenant("tenant_a"),
            tenant("tenant_a"),
        ])
        .expect("set");
        assert_eq!(set.as_slice(), &[tenant("tenant_a"), tenant("tenant_b")]);
    }

    #[test]
    fn tenant_scope_roots_should_merge_same_tenant_roots() {
        let entries = TenantScopeRoots::new(vec![
            TenantScopedRoots::new(tenant("tenant_a"), roots(&["agent/1/store/1"])),
            TenantScopedRoots::new(tenant("tenant_a"), roots(&["agent/1"])),
        ])
        .expect("entries");

        assert_eq!(
            entries.as_slice()[0].roots.as_slice(),
            &[ScopePath::parse("agent/1").expect("path")]
        );
    }

    #[test]
    fn tenant_data_access_scope_should_merge_path_entries() {
        let scope = TenantDataAccessScope::merge(vec![
            PlatformGrantScope::tenant_paths(vec![
                TenantScopedRoots::new(tenant("tenant_a"), roots(&["agent/1/store/1"])),
                TenantScopedRoots::new(tenant("tenant_a"), roots(&["agent/1"])),
            ])
            .expect("scope"),
        ])
        .expect("scope");

        assert_eq!(
            scope,
            TenantDataAccessScope::TenantPaths {
                entries: vec![TenantScopedRoots::new(
                    tenant("tenant_a"),
                    roots(&["agent/1"])
                )],
            }
        );
    }

    #[test]
    fn tenant_data_access_scope_should_reject_mixed_tenant_and_path_grants() {
        let err = TenantDataAccessScope::merge(vec![
            PlatformGrantScope::tenants(vec![tenant("tenant_a")]).expect("scope"),
            PlatformGrantScope::tenant_paths(vec![TenantScopedRoots::new(
                tenant("tenant_b"),
                roots(&["agent/1"]),
            )])
            .expect("scope"),
        ])
        .expect_err("must reject mixed grants");

        assert!(err.to_string().contains("must not mix"));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_should_reject_empty_tenant_data_tenants() {
        let err =
            serde_json::from_str::<TenantDataAccessScope>(r#"{"type":"tenants","tenants":[]}"#)
                .expect_err("must reject");

        assert!(err.to_string().contains("tenant set"));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_should_reject_empty_tenant_data_path_entries() {
        let err = serde_json::from_str::<TenantDataAccessScope>(
            r#"{"type":"tenant_paths","entries":[]}"#,
        )
        .expect_err("must reject");

        assert!(err.to_string().contains("tenant scope roots"));
    }
}

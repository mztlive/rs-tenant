use crate::error::{Error, Result};
use crate::ids::TenantId;
use std::borrow::Borrow;
use std::collections::BTreeSet;
use std::fmt;

const MAX_SCOPE_PATH_LEN: usize = 256;

/// Hierarchical scope path, for example `agent/123/store/456`.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ScopePath(String);

impl ScopePath {
    /// Parses and validates a scope path.
    pub fn parse(value: impl AsRef<str>) -> Result<Self> {
        let trimmed = value.as_ref().trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidScope(
                "scope path must not be empty".to_string(),
            ));
        }
        if trimmed.len() > MAX_SCOPE_PATH_LEN {
            return Err(Error::InvalidScope(format!(
                "scope path length must be <= {MAX_SCOPE_PATH_LEN}"
            )));
        }
        for segment in trimmed.split('/') {
            if segment.is_empty() {
                return Err(Error::InvalidScope(
                    "scope path contains empty segment".to_string(),
                ));
            }
            if !segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
            {
                return Err(Error::InvalidScope(
                    "scope path contains invalid characters".to_string(),
                ));
            }
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Returns the path string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns whether this path is a strict ancestor of `other`.
    pub fn is_ancestor_of(&self, other: &ScopePath) -> bool {
        self != other && other.0.starts_with(&format!("{}/", self.0))
    }

    /// Returns whether this path allows access to `target`.
    pub fn allows(&self, target: &ScopePath) -> bool {
        self == target || self.is_ancestor_of(target)
    }
}

impl fmt::Display for ScopePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for ScopePath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for ScopePath {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<&str> for ScopePath {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ScopePath {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ScopePath {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

/// Scope granted by a single role assignment.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum GrantScope {
    /// Tenant-wide grant.
    Tenant,
    /// Grant rooted at one or more scope paths.
    Paths(ScopeRoots),
}

/// Non-empty, compacted scope roots.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ScopeRoots {
    roots: Vec<ScopePath>,
}

impl ScopeRoots {
    /// Creates validated scope roots.
    pub fn new(roots: Vec<ScopePath>) -> Result<Self> {
        if roots.is_empty() {
            return Err(Error::InvalidScope(
                "grant scope paths must not be empty".to_string(),
            ));
        }
        Ok(Self {
            roots: compact_paths(roots),
        })
    }

    /// Returns the compacted roots.
    pub fn as_slice(&self) -> &[ScopePath] {
        &self.roots
    }

    /// Consumes the wrapper and returns the roots.
    pub fn into_vec(self) -> Vec<ScopePath> {
        self.roots
    }
}

impl GrantScope {
    /// Creates a tenant-wide grant scope.
    pub fn tenant() -> Self {
        Self::Tenant
    }

    /// Creates a path grant scope.
    pub fn paths(roots: Vec<ScopePath>) -> Result<Self> {
        ScopeRoots::new(roots).map(Self::Paths)
    }

    /// Returns whether this grant covers the whole tenant.
    pub fn is_tenant(&self) -> bool {
        matches!(self, Self::Tenant)
    }

    /// Returns path roots for path grants.
    pub fn roots(&self) -> &[ScopePath] {
        match self {
            Self::Tenant => &[],
            Self::Paths(roots) => roots.as_slice(),
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ScopeRoots {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.roots.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ScopeRoots {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let roots = Vec::<ScopePath>::deserialize(deserializer)?;
        Self::new(roots).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for GrantScope {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum GrantScopeWire<'a> {
            Tenant,
            Paths { roots: &'a [ScopePath] },
        }
        match self {
            Self::Tenant => GrantScopeWire::Tenant.serialize(serializer),
            Self::Paths(roots) => GrantScopeWire::Paths {
                roots: roots.as_slice(),
            }
            .serialize(serializer),
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for GrantScope {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum GrantScopeWire {
            Tenant,
            Paths { roots: Vec<ScopePath> },
        }
        match GrantScopeWire::deserialize(deserializer)? {
            GrantScopeWire::Tenant => Ok(Self::Tenant),
            GrantScopeWire::Paths { roots } => Self::paths(roots).map_err(serde::de::Error::custom),
        }
    }
}

/// Merged access scope for a concrete permission query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccessScope {
    /// No matching access.
    None,
    /// Tenant-wide access.
    Tenant { tenant: TenantId },
    /// Path-rooted access.
    Paths {
        /// Tenant for the query.
        tenant: TenantId,
        /// Compacted roots that cover all allowed descendants.
        roots: Vec<ScopePath>,
    },
}

impl AccessScope {
    /// Merges grant scopes into a final access scope.
    pub fn merge(tenant: TenantId, grants: impl IntoIterator<Item = GrantScope>) -> Self {
        let mut roots = Vec::new();
        for grant in grants {
            match grant {
                GrantScope::Tenant => return Self::Tenant { tenant },
                GrantScope::Paths(grant_roots) => roots.extend(grant_roots.into_vec()),
            }
        }
        if roots.is_empty() {
            Self::None
        } else {
            Self::Paths {
                tenant,
                roots: compact_paths(roots),
            }
        }
    }

    /// Returns whether this scope allows a target path.
    pub fn allows_path(&self, target: &ScopePath) -> bool {
        match self {
            Self::None => false,
            Self::Tenant { .. } => true,
            Self::Paths { roots, .. } => roots.iter().any(|root| root.allows(target)),
        }
    }
}

fn compact_paths(roots: Vec<ScopePath>) -> Vec<ScopePath> {
    let ordered: BTreeSet<_> = roots.into_iter().collect();
    let mut compacted: Vec<ScopePath> = Vec::new();
    for path in ordered {
        if compacted.iter().any(|root| root.allows(&path)) {
            continue;
        }
        compacted.push(path);
    }
    compacted
}

#[cfg(test)]
mod tests {
    use super::{AccessScope, GrantScope, ScopePath};
    use crate::TenantId;

    #[test]
    fn scope_path_should_allow_descendant() {
        let root = ScopePath::parse("agent/123").expect("scope path");
        let target = ScopePath::parse("agent/123/store/456").expect("scope path");

        assert!(root.allows(&target));
        assert!(!target.allows(&root));
    }

    #[test]
    fn grant_scope_paths_should_reject_empty_roots() {
        let err = GrantScope::paths(Vec::new()).expect_err("must reject");
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn access_scope_should_compact_child_paths() {
        let tenant = TenantId::parse("tenant_1").expect("tenant");
        let parent = ScopePath::parse("agent/123").expect("scope path");
        let child = ScopePath::parse("agent/123/store/456").expect("scope path");

        let scope = AccessScope::merge(
            tenant,
            [GrantScope::paths(vec![child, parent]).expect("grant scope")],
        );

        let AccessScope::Paths { roots, .. } = scope else {
            panic!("expected paths");
        };
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].as_str(), "agent/123");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_should_reject_empty_grant_paths() {
        let err = serde_json::from_str::<GrantScope>(r#"{"type":"paths","roots":[]}"#)
            .expect_err("must reject");
        assert!(err.to_string().contains("must not be empty"));
    }
}

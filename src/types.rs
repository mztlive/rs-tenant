use crate::error::{Error, Result};
use std::borrow::Borrow;
use std::fmt;

const MAX_NAME_LEN: usize = 128;

fn validate_simple_name(value: &str, kind: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidId(format!("{kind} must not be empty")));
    }
    if trimmed.len() > MAX_NAME_LEN {
        return Err(Error::InvalidId(format!(
            "{kind} length must be <= {MAX_NAME_LEN}"
        )));
    }
    if !trimmed.chars().all(is_allowed_name_char) {
        return Err(Error::InvalidId(format!(
            "{kind} contains invalid characters"
        )));
    }
    Ok(trimmed.to_string())
}

fn is_allowed_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, ':' | '_' | '-')
}

macro_rules! define_id_type {
    ($(#[$doc:meta])* $name:ident, $kind:expr) => {
        $(#[$doc])*
        #[derive(Clone, Debug, Eq, PartialEq, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(transparent))]
        pub struct $name(String);

        impl $name {
            /// Creates a validated identifier.
            pub fn new(value: impl AsRef<str>) -> Result<Self> {
                validate_simple_name(value.as_ref(), $kind).map(Self)
            }

            /// Creates an identifier from a trusted string without validation.
            pub fn from_string(value: String) -> Self {
                Self(value)
            }

            /// Returns the underlying string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<&str> for $name {
            type Error = Error;

            fn try_from(value: &str) -> Result<Self> {
                Self::new(value)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::from_string(value)
            }
        }
    };
}

define_id_type!(
    /// Tenant identifier.
    TenantId,
    "tenant id"
);
define_id_type!(
    /// Principal identifier.
    PrincipalId,
    "principal id"
);
define_id_type!(
    /// Role identifier.
    RoleId,
    "role id"
);
define_id_type!(
    /// Global role identifier.
    GlobalRoleId,
    "global role id"
);

impl PrincipalId {
    /// Creates a principal id from `kind` and `account_id` segments.
    ///
    /// Both segments are validated by [`PrincipalId::new`]. Callers should pass
    /// semantic pieces such as `("admin", "user_1")` instead of formatting
    /// the raw id string at call sites.
    pub fn try_from_parts(kind: impl AsRef<str>, account_id: impl AsRef<str>) -> Result<Self> {
        let kind = validate_simple_name(kind.as_ref(), "principal kind")?;
        let account_id = validate_simple_name(account_id.as_ref(), "principal account id")?;
        Self::new(format!("{kind}:{account_id}"))
    }
}

/// Resource name used for scope checks.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct ResourceName(String);

impl ResourceName {
    /// Creates a validated resource name.
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        validate_simple_name(value.as_ref(), "resource name").map(Self)
    }

    /// Creates a resource name from a trusted string without validation.
    pub fn from_string(value: String) -> Self {
        Self(value)
    }

    /// Returns the underlying string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ResourceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ResourceName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for ResourceName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for ResourceName {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl From<String> for ResourceName {
    fn from(value: String) -> Self {
        Self::from_string(value)
    }
}

#[cfg(test)]
mod tests {
    use super::PrincipalId;

    #[test]
    fn principal_id_try_from_parts_success() {
        let principal = PrincipalId::try_from_parts("admin", "user_1").expect("principal id");
        assert_eq!(principal.as_str(), "admin:user_1");
    }

    #[test]
    fn principal_id_try_from_parts_rejects_empty_segment() {
        let err = PrincipalId::try_from_parts("admin", "   ").expect_err("must reject");
        assert!(err.to_string().contains("principal account id"));
    }

    #[test]
    fn principal_id_try_from_parts_rejects_invalid_chars() {
        let err = PrincipalId::try_from_parts("ad min", "user_1").expect_err("must reject");
        assert!(err.to_string().contains("principal kind"));
    }
}

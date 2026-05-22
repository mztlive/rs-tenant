use crate::error::{Error, Result};
use std::borrow::Borrow;
use std::fmt;

const MAX_ID_LEN: usize = 128;

/// 修剪并校验通用标识符。
fn validate_id(value: &str, kind: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidId(format!("{kind} must not be empty")));
    }
    if trimmed.len() > MAX_ID_LEN {
        return Err(Error::InvalidId(format!(
            "{kind} length must be <= {MAX_ID_LEN}"
        )));
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(Error::InvalidId(format!(
            "{kind} contains invalid characters"
        )));
    }
    Ok(trimmed.to_string())
}

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident, $kind:expr) => {
        $(#[$doc])*
        #[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// 解析并校验标识符。
            pub fn parse(value: impl AsRef<str>) -> Result<Self> {
                validate_id(value.as_ref(), $kind).map(Self)
            }

            /// 以字符串切片形式返回标识符。
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
                self.as_str()
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl TryFrom<&str> for $name {
            type Error = Error;

            fn try_from(value: &str) -> Result<Self> {
                Self::parse(value)
            }
        }

        #[cfg(feature = "serde")]
        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

define_id!(
    /// 租户标识符。
    TenantId,
    "tenant id"
);
define_id!(
    /// 租户授权上下文中的主体标识符。
    PrincipalId,
    "principal id"
);
define_id!(
    /// 租户级角色标识符。
    RoleId,
    "role id"
);

#[cfg(test)]
mod tests {
    use super::{PrincipalId, RoleId, TenantId};

    #[test]
    fn ids_should_trim_and_validate() {
        let tenant = TenantId::parse(" tenant_1 ").expect("tenant id");
        assert_eq!(tenant.as_str(), "tenant_1");
    }

    #[test]
    fn ids_should_reject_empty_values() {
        let err = PrincipalId::parse(" ").expect_err("must reject");
        assert!(err.to_string().contains("principal id"));
    }

    #[test]
    fn ids_should_reject_invalid_characters() {
        let err = RoleId::parse("role/admin").expect_err("must reject");
        assert!(err.to_string().contains("role id"));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_should_validate_ids() {
        let err = serde_json::from_str::<TenantId>("\"bad/id\"").expect_err("must reject");
        assert!(err.to_string().contains("tenant id"));
    }
}

use crate::error::{Error, Result};

const MAX_ID_LEN: usize = 128;

/// 修剪并校验通用标识符。
pub(crate) fn validate_id(value: &str, kind: &str) -> Result<String> {
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

macro_rules! define_id_type {
    ($(#[$doc:meta])* $name:ident, $kind:expr) => {
        $(#[$doc])*
        #[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// 解析并校验标识符。
            pub fn parse(value: impl AsRef<str>) -> crate::Result<Self> {
                crate::id::validate_id(value.as_ref(), $kind).map(Self)
            }

            /// 以字符串切片形式返回标识符。
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl TryFrom<&str> for $name {
            type Error = crate::Error;

            fn try_from(value: &str) -> crate::Result<Self> {
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

pub(crate) use define_id_type;

use crate::error::{Error, Result};
use std::borrow::Borrow;
use std::fmt;

const MAX_PERMISSION_PART_LEN: usize = 128;

/// 修剪权限片段并转换为小写。
fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

/// 校验权限资源或动作片段。
fn validate_segment(value: &str, kind: &str, allow_slash: bool) -> Result<()> {
    if value.is_empty() {
        return Err(Error::InvalidPermission(format!(
            "{kind} must not be empty"
        )));
    }
    if value.len() > MAX_PERMISSION_PART_LEN {
        return Err(Error::InvalidPermission(format!(
            "{kind} length must be <= {MAX_PERMISSION_PART_LEN}"
        )));
    }
    if value == "*" {
        return Ok(());
    }
    for segment in value.split('/') {
        if segment.is_empty() {
            return Err(Error::InvalidPermission(format!(
                "{kind} contains empty segment"
            )));
        }
        if !allow_slash && segment != value {
            return Err(Error::InvalidPermission(format!(
                "{kind} must not contain '/'"
            )));
        }
        if !segment
            .chars()
            .all(|ch| matches!(ch, 'a'..='z' | '0'..='9' | '_' | '-'))
        {
            return Err(Error::InvalidPermission(format!(
                "{kind} contains invalid characters"
            )));
        }
    }
    Ok(())
}

macro_rules! define_permission_part {
    ($(#[$doc:meta])* $name:ident, $kind:expr, $allow_slash:expr) => {
        $(#[$doc])*
        #[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// 解析、规范化并校验取值。
            pub fn parse(value: impl AsRef<str>) -> Result<Self> {
                let normalized = normalize(value.as_ref());
                validate_segment(&normalized, $kind, $allow_slash)?;
                Ok(Self(normalized))
            }

            /// 返回规范化后的取值。
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// 返回该片段是否为完整通配符。
            pub fn is_wildcard(&self) -> bool {
                self.0 == "*"
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
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

define_permission_part!(
    /// 权限资源，例如 `billing/invoice`。
    Resource,
    "resource",
    true
);
define_permission_part!(
    /// 权限动作，例如 `read`。
    Action,
    "action",
    false
);

/// 规范化后的 `resource:action` 权限。
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Permission {
    resource: Resource,
    action: Action,
}

impl Permission {
    /// 从已校验片段创建权限。
    pub fn new(resource: Resource, action: Action) -> Self {
        Self { resource, action }
    }

    /// 解析 `resource:action` 字符串。
    pub fn parse(value: impl AsRef<str>) -> Result<Self> {
        let normalized = normalize(value.as_ref());
        let mut parts = normalized.split(':');
        let resource = parts.next().unwrap_or_default();
        let action = parts.next().ok_or_else(|| {
            Error::InvalidPermission("permission must be in resource:action format".to_string())
        })?;
        if parts.next().is_some() {
            return Err(Error::InvalidPermission(
                "permission must contain exactly one ':' separator".to_string(),
            ));
        }
        Ok(Self::new(
            Resource::parse(resource)?,
            Action::parse(action)?,
        ))
    }

    /// 返回资源片段。
    pub fn resource(&self) -> &Resource {
        &self.resource
    }

    /// 返回动作片段。
    pub fn action(&self) -> &Action {
        &self.action
    }

    /// 返回该权限是否包含任意通配符片段。
    pub fn has_wildcard(&self) -> bool {
        self.resource.is_wildcard() || self.action.is_wildcard()
    }

    /// 返回当前授予的权限是否覆盖 `required`。
    pub fn matches(&self, required: &Permission, enable_wildcard: bool) -> bool {
        if !enable_wildcard && self.has_wildcard() {
            return false;
        }
        if !enable_wildcard {
            return self == required;
        }
        let resource_matches = self.resource.is_wildcard() || self.resource == required.resource;
        let action_matches = self.action.is_wildcard() || self.action == required.action;
        resource_matches && action_matches
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.resource, self.action)
    }
}

impl TryFrom<&str> for Permission {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Permission {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Permission {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, MAX_PERMISSION_PART_LEN, Permission, Resource};
    use crate::Error;

    #[test]
    fn permission_should_trim_and_lowercase() {
        let permission = Permission::parse(" Billing/Invoice:Read ").expect("permission");
        assert_eq!(permission.to_string(), "billing/invoice:read");
    }

    #[test]
    fn permission_should_reject_resource_colon() {
        let err = Permission::parse("billing:invoice:read").expect_err("must reject");
        assert!(matches!(err, Error::InvalidPermission(_)));
    }

    #[test]
    fn permission_should_reject_missing_separator_and_empty_segments() {
        for value in ["invoice", "invoice:", ":read", "billing//invoice:read"] {
            let err = Permission::parse(value).expect_err("must reject");
            assert!(matches!(err, Error::InvalidPermission(_)));
        }
    }

    #[test]
    fn action_should_reject_slash_and_overlong_values() {
        let slash_err = Action::parse("read/write").expect_err("must reject");
        assert!(slash_err.to_string().contains("must not contain '/'"));

        let oversized = "a".repeat(MAX_PERMISSION_PART_LEN + 1);
        let length_err = Action::parse(oversized).expect_err("must reject");
        assert!(length_err.to_string().contains("length must be"));
    }

    #[test]
    fn wildcard_should_match_only_when_enabled() {
        let granted = Permission::parse("invoice:*").expect("permission");
        let required = Permission::parse("invoice:read").expect("permission");

        assert!(!granted.matches(&required, false));
        assert!(granted.matches(&required, true));
    }

    #[test]
    fn resource_should_not_support_partial_wildcard() {
        let err = Resource::parse("billing/*").expect_err("must reject");
        assert!(matches!(err, Error::InvalidPermission(_)));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_should_validate_permission() {
        let err = serde_json::from_str::<Permission>("\"billing:invoice:read\"")
            .expect_err("must reject");
        assert!(err.to_string().contains("exactly one"));
    }
}

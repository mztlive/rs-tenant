use crate::error::{Error, Result};
use crate::types::ResourceName;
use std::borrow::{Borrow, Cow};
use std::fmt;

/// Permission string wrapper (`resource:action`).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Permission(String);

impl Permission {
    /// Parses and validates a permission using the default validator.
    ///
    /// This trims whitespace and normalizes to lowercase.
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        Self::new_with(value, &DefaultPermissionValidator, true)
    }

    /// Parses and validates a permission with a custom validator.
    ///
    /// When `normalize` is true, the value is trimmed and lowercased
    /// before validation.
    pub fn new_with(
        value: impl AsRef<str>,
        validator: &dyn PermissionValidator,
        normalize: bool,
    ) -> Result<Self> {
        let trimmed = value.as_ref().trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidPermission(
                "permission must not be empty".to_string(),
            ));
        }
        let normalized = if normalize {
            trimmed.to_ascii_lowercase()
        } else {
            trimmed.to_string()
        };
        validator.validate(&normalized)?;
        Ok(Self(normalized))
    }

    /// Creates a permission from a trusted string without validation.
    pub fn from_string(value: String) -> Self {
        Self(value)
    }

    /// Returns the underlying string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Permission {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for Permission {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for Permission {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl From<String> for Permission {
    fn from(value: String) -> Self {
        Self::from_string(value)
    }
}

/// Permission validator interface for custom rules.
pub trait PermissionValidator: Send + Sync {
    /// Validates a normalized permission string.
    fn validate(&self, value: &str) -> Result<()>;
}

/// Default strict permission validator.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultPermissionValidator;

impl PermissionValidator for DefaultPermissionValidator {
    fn validate(&self, value: &str) -> Result<()> {
        let (resource, action) = split_permission(value).ok_or_else(|| {
            Error::InvalidPermission("permission must be in resource:action format".to_string())
        })?;
        if resource.is_empty() || action.is_empty() {
            return Err(Error::InvalidPermission(
                "permission must not have empty segments".to_string(),
            ));
        }
        for segment in resource.split(':') {
            if !is_valid_segment(segment) {
                return Err(Error::InvalidPermission(
                    "resource segment contains invalid characters".to_string(),
                ));
            }
        }
        if !is_valid_segment(action) {
            return Err(Error::InvalidPermission(
                "action segment contains invalid characters".to_string(),
            ));
        }
        Ok(())
    }
}

fn is_valid_segment(segment: &str) -> bool {
    if segment == "*" {
        return true;
    }
    if segment.is_empty() {
        return false;
    }
    segment
        .chars()
        .all(|ch| matches!(ch, 'a'..='z' | '0'..='9' | '_' | '-'))
}

pub(crate) fn split_permission(value: &str) -> Option<(&str, &str)> {
    value.rsplit_once(':')
}

fn normalize_for_match<'a>(value: &'a str, normalize: bool) -> Cow<'a, str> {
    if normalize {
        Cow::Owned(value.to_ascii_lowercase())
    } else {
        Cow::Borrowed(value)
    }
}

fn has_wildcard_segment(resource: &str, action: &str) -> bool {
    action == "*" || resource.split(':').any(|segment| segment == "*")
}

pub(crate) fn permission_matches(
    granted: &Permission,
    required: &Permission,
    enable_wildcard: bool,
    normalize: bool,
) -> bool {
    let Some((g_res_raw, g_act_raw)) = split_permission(granted.as_str()) else {
        return false;
    };
    let Some((r_res_raw, r_act_raw)) = split_permission(required.as_str()) else {
        return false;
    };
    if !enable_wildcard && has_wildcard_segment(g_res_raw, g_act_raw) {
        return false;
    }
    let g_res = normalize_for_match(g_res_raw, normalize);
    let g_act = normalize_for_match(g_act_raw, normalize);
    let r_res = normalize_for_match(r_res_raw, normalize);
    let r_act = normalize_for_match(r_act_raw, normalize);

    if !enable_wildcard {
        return g_res == r_res && g_act == r_act;
    }

    if g_res == "*" && g_act == "*" {
        return true;
    }
    if g_res == "*" && g_act == r_act {
        return true;
    }
    if g_act == "*" && g_res == r_res {
        return true;
    }
    g_res == r_res && g_act == r_act
}

pub(crate) fn resource_matches(
    granted: &Permission,
    resource: &ResourceName,
    enable_wildcard: bool,
    normalize: bool,
) -> bool {
    let Some((g_res_raw, g_act_raw)) = split_permission(granted.as_str()) else {
        return false;
    };
    if !enable_wildcard && has_wildcard_segment(g_res_raw, g_act_raw) {
        return false;
    }
    let g_res = normalize_for_match(g_res_raw, normalize);
    let resource = normalize_for_match(resource.as_str(), normalize);

    if !enable_wildcard {
        return g_res == resource;
    }
    if g_res == "*" {
        return true;
    }
    g_res == resource
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_from_should_trim_and_lowercase() {
        let permission = Permission::try_from(" Invoice:Read ").unwrap();
        assert_eq!(permission.as_str(), "invoice:read");
    }

    #[test]
    fn try_from_should_reject_empty_segments() {
        let result = Permission::try_from(":read");
        assert!(matches!(result, Err(Error::InvalidPermission(_))));
    }

    #[test]
    fn resource_match_should_ignore_wildcard_permission_when_disabled() {
        let granted = Permission::try_from("invoice:*").unwrap();
        let resource = ResourceName::try_from("invoice").unwrap();

        assert!(!resource_matches(&granted, &resource, false, true));
    }

    #[test]
    fn permission_match_should_ignore_wildcard_permission_when_disabled() {
        let granted = Permission::try_from("invoice:*").unwrap();
        let required = Permission::try_from("invoice:*").unwrap();

        assert!(!permission_matches(&granted, &required, false, true));
    }
}

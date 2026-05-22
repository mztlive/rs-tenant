use crate::id::define_id_type;

define_id_type!(
    /// 平台主体标识符。
    PlatformPrincipalId,
    "platform principal id"
);
define_id_type!(
    /// 平台级角色标识符。
    PlatformRoleId,
    "platform role id"
);

#[cfg(test)]
mod tests {
    use super::{PlatformPrincipalId, PlatformRoleId};

    #[test]
    fn platform_ids_should_trim_and_validate() {
        let principal = PlatformPrincipalId::parse(" platform_admin ").expect("principal");
        assert_eq!(principal.as_str(), "platform_admin");
    }

    #[test]
    fn platform_ids_should_reject_invalid_characters() {
        let err = PlatformRoleId::parse("role/admin").expect_err("must reject");
        assert!(err.to_string().contains("platform role id"));
    }
}

use crate::id::define_id_type;

define_id_type!(
    /// 租户标识符。
    TenantId,
    "tenant id"
);
define_id_type!(
    /// 租户授权上下文中的主体标识符。
    PrincipalId,
    "principal id"
);
define_id_type!(
    /// 租户级角色标识符。
    RoleId,
    "role id"
);

#[cfg(test)]
mod tests {
    use super::{PrincipalId, RoleId, TenantId};

    const MAX_ID_LEN: usize = 128;

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

    #[test]
    fn ids_should_reject_values_over_max_length() {
        let oversized = "a".repeat(MAX_ID_LEN + 1);
        let err = TenantId::parse(oversized).expect_err("must reject");

        assert!(err.to_string().contains("length must be"));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_should_validate_ids() {
        let err = serde_json::from_str::<TenantId>("\"bad/id\"").expect_err("must reject");
        assert!(err.to_string().contains("tenant id"));
    }
}

use crate::scope::AccessScope;

/// 最终的允许或拒绝决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDecision {
    /// 允许访问。
    Allow,
    /// 拒绝访问。
    Deny,
}

/// 用于解释和测试的高层拒绝原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    /// 租户不存在或未激活。
    TenantInactive,
    /// 主体成员关系不存在或未激活。
    PrincipalInactive,
    /// 没有匹配的权限授权。
    PermissionMissing,
    /// 只有路径级授权时调用了租户级 API。
    TargetScopeRequired,
    /// 目标路径不在可访问根路径内。
    ScopeDenied,
}

/// 授权决策的轻量解释信息。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessExplanation {
    /// 最终决策。
    pub decision: AccessDecision,
    /// 当决策为 [`AccessDecision::Deny`] 时的拒绝原因。
    pub reason: Option<DenyReason>,
    /// 检查过程中计算出的有效访问范围。
    pub scope: AccessScope,
}

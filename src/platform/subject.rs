use super::PlatformPrincipalId;

/// 平台主体状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PlatformPrincipalStatus {
    /// 主体可以接收平台授权。
    Active,
    /// 主体不存在或已禁用。
    Inactive,
}

/// 用于授权的平台级主体。
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlatformSubject {
    /// 平台主体标识符。
    pub principal: PlatformPrincipalId,
}

impl PlatformSubject {
    /// 创建平台主体。
    pub fn new(principal: PlatformPrincipalId) -> Self {
        Self { principal }
    }
}

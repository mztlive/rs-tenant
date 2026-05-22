use crate::Permission;

/// 角色分配、角色权限和分配范围合成后的有效授权。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopedGrant<R, S> {
    /// 读取到权限的来源角色。
    pub role: R,
    /// 角色授予的权限。
    pub permission: Permission,
    /// 原始角色分配附带的范围。
    pub scope: S,
}

impl<R, S> ScopedGrant<R, S> {
    /// 创建有效授权。
    pub fn new(role: R, permission: Permission, scope: S) -> Self {
        Self {
            role,
            permission,
            scope,
        }
    }

    /// 返回该授权是否匹配所需权限。
    pub(crate) fn matches_permission(&self, required: &Permission, wildcard: bool) -> bool {
        self.permission.matches(required, wildcard)
    }
}

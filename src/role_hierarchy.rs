use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashSet;
use std::hash::Hash;

/// 角色继承图读取和错误映射。
#[async_trait]
pub(crate) trait RoleHierarchy {
    /// 角色标识符类型。
    type Role: Clone + Eq + Hash + Send + Sync + 'static;

    /// 返回直接父角色。
    async fn parent_roles(&self, role: &Self::Role) -> Result<Vec<Self::Role>>;

    /// 返回最大继承深度。
    fn max_depth(&self) -> usize;

    /// 构造角色环错误。
    fn cycle_error(&self, role: Self::Role) -> Error;

    /// 构造深度超限错误。
    fn depth_error(&self, role: Self::Role) -> Error;
}

/// 展开角色及其继承链上的父角色。
pub(crate) async fn expand_roles<H>(hierarchy: &H, root: H::Role) -> Result<Vec<H::Role>>
where
    H: RoleHierarchy + Sync,
{
    let mut visited = HashSet::new();
    let mut visiting = HashSet::new();
    let mut output = Vec::new();
    expand_from(hierarchy, root, &mut visited, &mut visiting, &mut output).await?;
    Ok(output)
}

async fn expand_from<H>(
    hierarchy: &H,
    root: H::Role,
    visited: &mut HashSet<H::Role>,
    visiting: &mut HashSet<H::Role>,
    output: &mut Vec<H::Role>,
) -> Result<()>
where
    H: RoleHierarchy + Sync,
{
    visiting.insert(root.clone());
    output.push(root.clone());
    let parents = hierarchy.parent_roles(&root).await?;
    let mut stack: Vec<(H::Role, usize, std::vec::IntoIter<H::Role>)> =
        vec![(root, 0, parents.into_iter())];

    while let Some((current, depth, mut iter)) = stack.pop() {
        if let Some(parent) = iter.next() {
            stack.push((current.clone(), depth, iter));
            let next_depth = depth + 1;
            if next_depth > hierarchy.max_depth() {
                return Err(hierarchy.depth_error(parent));
            }
            if visiting.contains(&parent) {
                return Err(hierarchy.cycle_error(parent));
            }
            if visited.contains(&parent) {
                continue;
            }

            let parents = hierarchy.parent_roles(&parent).await?;
            visiting.insert(parent.clone());
            output.push(parent.clone());
            stack.push((parent, next_depth, parents.into_iter()));
            continue;
        }

        visiting.remove(&current);
        visited.insert(current);
    }

    Ok(())
}

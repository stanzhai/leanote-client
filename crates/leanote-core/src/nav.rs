//! 笔记本导航树（issue 15）：roots 按 `Seq`、子挂 `ParentNotebookId` 下按
//! `Seq`、深度优先走树。桌面左栏与移动端笔记本列表**同源共用**——此前
//! 移动端自排（按 parent 字符串分组排序）导致顺序/嵌套与桌面不一致。
//! 纯逻辑无 GPUI 依赖，每个分支可头less单测（自桌面 browse.rs 迁入）。

use std::collections::HashMap;

/// 一个可选中的导航节点（笔记本或标签行）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavNode {
    /// Stable element id / selection key.
    pub id: String,
    pub title: String,
    /// How many notes this node filters to.
    pub count: i64,
    pub depth: u8,
}

/// Build the Notebooks group: roots in `Seq` order, children nested under
/// their `ParentNotebookId` (also `Seq` order, one level like the Electron
/// tree). `counts` comes from [`crate::Store::notebook_note_counts`].
/// Orphans (parent id 无对应笔记本) 不进列表。
pub fn notebook_nodes(
    notebooks: &[(String, String, String, i64)], // (id, title, parent, seq)
    counts: &HashMap<String, i64>,
) -> Vec<NavNode> {
    // Children keyed by parent id, sibling order by Seq.
    let mut kids: HashMap<String, Vec<(String, String, i64)>> = HashMap::new();
    let mut roots: Vec<(String, String, i64)> = Vec::new();
    for (id, title, parent, seq) in notebooks {
        if parent.is_empty() {
            roots.push((id.clone(), title.clone(), *seq));
        } else {
            kids.entry(parent.clone())
                .or_default()
                .push((id.clone(), title.clone(), *seq));
        }
    }
    let by_seq = |v: &mut Vec<(String, String, i64)>| v.sort_by_key(|(_, _, s)| *s);
    by_seq(&mut roots);
    for v in kids.values_mut() {
        by_seq(v);
    }

    // Depth-first walk: root, then its children.
    fn walk(
        out: &mut Vec<NavNode>,
        (id, title, _): &(String, String, i64),
        kids: &HashMap<String, Vec<(String, String, i64)>>,
        counts: &HashMap<String, i64>,
        depth: u8,
    ) {
        out.push(NavNode {
            id: id.clone(),
            title: title.clone(),
            count: counts.get(id).copied().unwrap_or(0),
            depth,
        });
        if let Some(children) = kids.get(id) {
            for c in children {
                walk(out, c, kids, counts, depth + 1);
            }
        }
    }
    let mut out = Vec::new();
    for r in &roots {
        walk(&mut out, r, &kids, counts, 0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(pairs: &[(&str, i64)]) -> HashMap<String, i64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn notebook_nodes_nest_and_seq_order() {
        let nbs: Vec<(String, String, String, i64)> = vec![
            ("nb2", "Work", "", 2),
            ("nb3", "Reading", "", 3),
            ("nb1", "Inbox", "", 1),
            ("nb2b", "Deep", "nb2", 2),
            ("nb2a", "Meetings", "nb2", 1),
        ]
        .into_iter()
        .map(|(a, b, c, d)| (a.into(), b.into(), c.into(), d))
        .collect();
        let nodes = notebook_nodes(&nbs, &counts(&[("nb1", 1), ("nb2", 2), ("nb2b", 5)]));
        let flat: Vec<(&str, u8, i64)> = nodes
            .iter()
            .map(|n| (n.title.as_str(), n.depth, n.count))
            .collect();
        // Roots in Seq order 1,2,3; nb2's children nested after it by Seq.
        assert_eq!(
            flat,
            vec![
                ("Inbox", 0, 1),
                ("Work", 0, 2),
                ("Meetings", 1, 0), // no count row → 0
                ("Deep", 1, 5),
                ("Reading", 0, 0), // nb3 has no counts row → 0
            ]
        );
    }

    #[test]
    fn notebook_nodes_drop_orphans() {
        // Server never sends a notebook whose parent is missing; if it ever
        // did, the walk drops it rather than showing a stray root.
        let nbs: Vec<(String, String, String, i64)> = vec![("orphan", "Ghost", "gone", 1)]
            .into_iter()
            .map(|(a, b, c, d)| (a.into(), b.into(), c.into(), d))
            .collect();
        assert!(notebook_nodes(&nbs, &HashMap::new()).is_empty());
    }
}

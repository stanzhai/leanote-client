//! Ticket 09: pure browse logic — nav tree build + list row meta. No GPUI
//! imports so every branch has a headless unit test.

use std::collections::HashMap;

/// A selectable left-nav node.
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
/// tree). `counts` comes from [`leanote_core::Store::notebook_note_counts`].
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

/// Middle-list row meta: notebook name + updated date. Tags are dropped —
/// long tag lists pushed the date off the truncated line.
pub fn note_meta(notebook: &str, updated_ms: i64) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !notebook.is_empty() {
        parts.push(notebook.to_string());
    }
    parts.push(crate::app_shell::fmt_time(updated_ms));
    parts.join(" · ")
}

/// Markdown source handed to `TextView::markdown`. Markdown content passes
/// through unmodified; non-Markdown (rich-text) notes are converted for
/// display only (ticket 12) — the store keeps the original content until
/// the user runs Convert to Markdown. Plain-text content has no HTML tags,
/// so it converts to itself unchanged.
pub fn reader_markdown_src(title: &str, content: &str, is_markdown: bool) -> String {
    if is_markdown {
        content.to_string()
    } else {
        let body = leanote_core::html2md::html_to_markdown(content);
        if body.trim().is_empty() {
            format!("# {title}\n")
        } else {
            body
        }
    }
}

/// Ticket 11 follow-up: derive a drag-drop target from the flat nav list,
/// porting zTree's drop bands (`jquery.ztree.all-3.5.js` 2899–2914): top
/// quarter of the hovered row = insert before, bottom quarter = after,
/// middle half = drop inside (append as last child). The index is the
/// position among SIBLINGS of the hovered row's parent — the flat list
/// index is not it once earlier siblings have subtrees.
///
/// `dragged` is the notebook being moved: zTree splices it out of its old
/// parent BEFORE computing the target index (`moveNode`, ztree.all-3.5.js
/// 3340–3357), and `Store::move_notebook` inserts into the target group
/// with the dragged id excluded from the query — so the sibling position
/// MUST be counted on the group WITHOUT the dragged notebook.
#[derive(Clone, Debug, PartialEq)]
pub struct DropTarget {
    /// Parent to move under ("" = root).
    pub parent: String,
    /// Insert position among that parent's siblings (usize::MAX = append
    /// as last child, the `inner` band).
    pub index: usize,
    /// Y of the 2px drop indicator in content px (top edge for
    /// before/inner, bottom edge for after).
    pub indicator_y: f32,
    /// `inner` band — caller highlights the row instead of drawing the line.
    pub inner: bool,
}

pub fn drop_target(
    nodes: &[NavNode],
    ix: usize,
    into: f32,
    row_h: f32,
    dragged: &str,
) -> Option<DropTarget> {
    let node = nodes.get(ix)?;
    // Parent id + sibling position from the flat walk: scan backwards past
    // earlier siblings' subtrees.
    let (parent, sib) = parent_and_sibling_pos(nodes, ix, dragged);
    let before = into < 0.25 * row_h;
    let after = into > 0.75 * row_h;
    if !before && !after {
        Some(DropTarget {
            parent: node.id.clone(),
            index: usize::MAX,
            indicator_y: row_h * ix as f32,
            inner: true,
        })
    } else {
        Some(DropTarget {
            parent,
            index: if before { sib } else { sib + 1 },
            indicator_y: row_h * if before { ix } else { ix + 1 } as f32,
            inner: false,
        })
    }
}

/// Walk backwards from the hovered row: the parent is the nearest row above
/// with `depth == d - 1`; the sibling position is how many same-depth rows
/// (the hovered one included, the dragged one excluded) precede it since
/// that parent. Deeper rows belong to earlier siblings' subtrees and don't
/// count; the dragged row doesn't either — it is no longer in the group
/// when the insertion index is used.
fn parent_and_sibling_pos(nodes: &[NavNode], ix: usize, dragged: &str) -> (String, usize) {
    let d = nodes[ix].depth as usize;
    if d == 0 {
        // Root sibling position: flat index minus the non-root rows above.
        return (
            String::new(),
            nodes[..ix]
                .iter()
                .filter(|n| n.depth == 0 && n.id != dragged)
                .count(),
        );
    }
    // Count the hovered row itself plus every same-depth row above it back
    // to the parent — deeper rows belong to earlier siblings' subtrees.
    let mut sib = 1;
    let mut parent = String::new();
    for n in nodes[..ix].iter().rev() {
        let nd = n.depth as usize;
        if nd == d && n.id != dragged {
            sib += 1;
        } else if nd == d - 1 {
            parent = n.id.clone();
            break;
        }
    }
    (parent, sib - 1)
}

/// Issue 21 需求9: swap every `leanote://file/getImage?fileId=X` link for a
/// `data:` URL from the local images table so TextView renders it; unknown
/// ids keep the link (renders as alt text until the bytes arrive).
///
/// Remote `<host>/api/file/getImage?fileId=X` links get the account token
/// appended: the server 500s private images for anonymous GETs, and the
/// preview fetches them with its plain HTTP client (no session cookies).
/// Tokens are hex, so no URL escaping is needed.
pub fn resolve_note_images(content: &str, store: &leanote_core::Store) -> String {
    let mut out = content.to_string();
    for id in leanote_core::Store::image_ids_in_content(content) {
        if let Some(img) = store.get_image(&id).ok().flatten() {
            out = out.replace(
                &format!("leanote://file/getImage?fileId={id}"),
                &format!(
                    "data:{};base64,{}",
                    img.mime,
                    crate::export::base64_encode(&img.bytes)
                ),
            );
        }
    }
    if let Some(acct) = store.active_account().ok().flatten() {
        if !acct.token.is_empty() && !acct.host.is_empty() {
            let host = acct.host.trim_end_matches('/');
            out = out.replace(
                &format!("{host}/api/file/getImage?fileId="),
                &format!("{host}/api/file/getImage?token={}&fileId=", acct.token),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(pairs: &[(&str, i64)]) -> HashMap<String, i64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn nav(nodes: &[(&str, u8)]) -> Vec<NavNode> {
        nodes
            .iter()
            .map(|(id, depth)| NavNode {
                id: id.to_string(),
                title: id.to_string(),
                count: 0,
                depth: *depth,
            })
            .collect()
    }

    #[test]
    fn drop_target_bands_on_root_row() {
        // roots A, B, C — hovering B's top quarter inserts before it
        // (root group, index 1). Dragged D is not in the list at all.
        let nodes = nav(&[("A", 0), ("B", 0), ("C", 0)]);
        let t = drop_target(&nodes, 1, 4.0, 32.0, "D").unwrap();
        assert_eq!((t.parent.as_str(), t.index, t.indicator_y), ("", 1, 32.0));
        // bottom quarter → after B, indicator at the row's bottom edge
        let t = drop_target(&nodes, 1, 28.0, 32.0, "D").unwrap();
        assert_eq!((t.parent.as_str(), t.index, t.indicator_y), ("", 2, 64.0));
        // middle half → drop inside B as its last child
        let t = drop_target(&nodes, 1, 16.0, 32.0, "D").unwrap();
        assert!(t.inner);
        assert_eq!((t.parent.as_str(), t.index), ("B", usize::MAX));
    }

    #[test]
    fn drop_target_sibling_index_skips_earlier_subtrees() {
        // The flat-index bug this fixes: A has two children ahead of B, so
        // B's flat index is 3 but its sibling position is 1.
        let nodes = nav(&[("A", 0), ("A1", 1), ("A2", 1), ("B", 0)]);
        let t = drop_target(&nodes, 3, 2.0, 32.0, "X").unwrap();
        assert_eq!((t.parent.as_str(), t.index), ("", 1));
        let t = drop_target(&nodes, 3, 30.0, 32.0, "X").unwrap();
        assert_eq!((t.parent.as_str(), t.index), ("", 2));
        // inside A's child group: before/after A2 → index 1/2 under A
        let t = drop_target(&nodes, 2, 2.0, 32.0, "X").unwrap();
        assert_eq!((t.parent.as_str(), t.index), ("A", 1));
        let t = drop_target(&nodes, 2, 30.0, 32.0, "X").unwrap();
        assert_eq!((t.parent.as_str(), t.index), ("A", 2));
    }

    #[test]
    fn drop_target_index_excludes_dragged_sibling() {
        // Regression: the sibling position must be counted on the group
        // WITHOUT the dragged notebook (Store::move_notebook inserts into
        // the target group with the dragged id excluded). Dragging A in
        // front of C used to compute sib(C)=2 (counting A) and insert at
        // index 2 of [B,C] → landed AFTER C.
        let nodes = nav(&[("A", 0), ("B", 0), ("C", 0)]);
        let t = drop_target(&nodes, 2, 4.0, 32.0, "A").unwrap(); // before C
        assert_eq!((t.parent.as_str(), t.index), ("", 1));
        let t = drop_target(&nodes, 2, 28.0, 32.0, "A").unwrap(); // after C
        assert_eq!((t.parent.as_str(), t.index), ("", 2));
        // Same-group move, dragged row sits between two targets.
        let nodes = nav(&[("A", 0), ("B", 0), ("C", 0)]);
        let t = drop_target(&nodes, 0, 28.0, 32.0, "B").unwrap(); // after A
        assert_eq!((t.parent.as_str(), t.index), ("", 1));
        let t = drop_target(&nodes, 2, 4.0, 32.0, "B").unwrap(); // before C
                                                                 // Move to the very END: drag A past the last row (bottom band of
                                                                 // C, or the trailing empty space the shell clamps there).
        let t = drop_target(&nodes, 2, 31.0, 32.0, "A").unwrap(); // after C
        assert_eq!((t.parent.as_str(), t.index), ("", 2));
        // Dragging C onto its own bottom band: sib=2 among [A,B] → 3,
        // clamped by move_notebook's `new_index.min(len)` to 2 = no-op
        // (zTree likewise ignores a drop onto itself).
        let t = drop_target(&nodes, 2, 31.0, 32.0, "C").unwrap();
        assert_eq!((t.parent.as_str(), t.index), ("", 3));
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

    #[test]
    fn note_meta_joins_present_parts() {
        let all = note_meta("Work", 1_765_235_200_000);
        assert_eq!(all, "Work · 2025-12-08");
        assert_eq!(note_meta("", 1_765_235_200_000), "2025-12-08");
    }

    #[test]
    fn reader_markdown_passthrough_and_conversion() {
        let src = "# T\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n~~gone~~";
        assert_eq!(reader_markdown_src("T", src, true), src);
        // Rich text converts to Markdown for display, never fenced.
        let rich = reader_markdown_src("T", "<h2>H</h2><p><strong>b</strong></p>", false);
        assert!(rich.contains("## H"), "{rich}");
        assert!(rich.contains("**b**"), "{rich}");
        assert!(!rich.contains("```"), "{rich}");
        // Plain text (no tags) survives as a paragraph: htmd treats the
        // blank line as a soft break, both lines stay legible.
        let plain = reader_markdown_src("T", "line1\n\nline2", false);
        assert!(
            plain.contains("line1") && plain.contains("line2"),
            "{plain}"
        );
        // Empty rich body falls back to the title so the pane isn't blank.
        assert_eq!(reader_markdown_src("T", "   ", false), "# T\n");
    }

    #[test]
    fn resolve_note_images_swaps_known_ids_only() {
        let dir = std::env::temp_dir().join(format!("lt-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = leanote_core::Store::open(dir.clone(), "u1".into()).unwrap();
        store
            .insert_image(&leanote_core::ImageAttachment {
                local_id: leanote_core::Id("imgaaaaaaaaaaaaaaaaaaaa1".into()),
                server_id: String::new(),
                file_name: "a.png".into(),
                mime: "image/png".into(),
                bytes: vec![1, 2, 3],
                size: 3,
                created_time: 0,
            })
            .unwrap();
        let md = "![a](leanote://file/getImage?fileId=imgaaaaaaaaaaaaaaaaaaaa1)\n![miss](leanote://file/getImage?fileId=zzz)";
        let out = resolve_note_images(md, &store);
        assert!(out.starts_with("![a](data:image/png;base64,"), "{out}");
        assert!(out.contains("leanote://file/getImage?fileId=zzz"), "{out}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_note_images_appends_token_to_remote_links() {
        let dir = std::env::temp_dir().join(format!("lt-imgtok-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = leanote_core::Store::open(dir.clone(), "u1".into()).unwrap();
        store
            .upsert_account(&leanote_core::Account {
                user_id: "u1".into(),
                email: String::new(),
                username: String::new(),
                host: "https://h.example".into(),
                token: "abc123".into(),
                is_active: true,
            })
            .unwrap();
        let md = "![a](https://h.example/api/file/getImage?fileId=abc)\n\
                  ![b](https://other.example/api/file/getImage?fileId=abc)";
        let out = resolve_note_images(md, &store);
        assert_eq!(
            out,
            "![a](https://h.example/api/file/getImage?token=abc123&fileId=abc)\n\
             ![b](https://other.example/api/file/getImage?fileId=abc)"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

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

/// Issue 21 需求9: swap every `leanote://file/getImage?fileId=X` link for a
/// `data:` URL from the local images table so TextView renders it; unknown
/// ids keep the link (renders as alt text until the bytes arrive).
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
}

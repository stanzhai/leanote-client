use leanote_core::*;
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("leanote-core-test-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn tmpdir_live(name: &str) -> PathBuf {
    // keep-on-drop variant for tests that reopen the db file directly
    let d = std::env::temp_dir().join(format!(
        "leanote-core-testlive-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn store(name: &str) -> Store {
    Store::open(tmpdir(name), "user1".to_string()).expect("open store")
}

fn add(store: &Store, title: &str, content: &str, tags: &[&str], notebook: &str) -> Note {
    store
        .add_note(NewNote {
            title: title.to_string(),
            content: content.to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            notebook_title: notebook.to_string(),
            is_markdown: true,
        })
        .expect("add note")
}

#[test]
fn add_via_stdin_style_content_sets_dirty_flags_and_markdown() {
    let s = store("flags");
    let n = add(&s, "T", "# Hello\nWorld", &["ai"], "AI札");
    assert_eq!(n.title, "T");
    assert_eq!(n.content, "# Hello\nWorld");
    assert!(n.is_dirty, "IsDirty must be set");
    assert!(n.is_new, "LocalIsNew must be set");
    assert!(n.is_markdown, "IsMarkdown must be true for CLI writes");
    assert!(!n.is_trash);
    assert!(!n.is_deleted);
    assert!(!n.star);
    // Desc mirrors JS: newlines->space, '#' stripped, 50 cap, trimmed
    assert_eq!(n.desc, "Hello World");
}

#[test]
fn add_auto_creates_aizha_notebook_once_with_dirty_flags() {
    let s = store("aizha");
    add(&s, "A", "c1", &[], "AI札");
    let nbs = s.notebooks().unwrap();
    assert_eq!(nbs.len(), 1);
    assert_eq!(nbs[0].title, "AI札");
    assert_eq!(nbs[0].seq, -1, "auto notebook Seq=-1 like JS");
    assert_eq!(nbs[0].parent_notebook_id, "");
    assert!(nbs[0].is_new && nbs[0].is_dirty);
    // second add reuses it — still exactly one notebook
    add(&s, "B", "c2", &[], "AI札");
    assert_eq!(s.notebooks().unwrap().len(), 1);
    assert_eq!(s.note_count().unwrap(), 2);
}

#[test]
fn add_upserts_tags() {
    let s = store("tags");
    add(&s, "A", "c", &["x", "y"], "AI札");
    add(&s, "B", "c", &["y", "z"], "AI札");
    let tags: Vec<String> = s.tags().unwrap().into_iter().map(|t| t.tag).collect();
    assert_eq!(tags, vec!["x", "y", "z"]);
}

#[test]
fn prefix_unique_ambiguous_none() {
    let s = store("prefix");
    let a = add(&s, "A", "ca", &[], "AI札");
    let b = add(&s, "B", "cb", &[], "AI札");
    // unique prefix
    let uniq = a.note_id.chars().take(6).collect::<String>();
    match s.resolve_prefix(&uniq).unwrap() {
        Ok(n) => assert_eq!(n.note_id, a.note_id),
        Err(e) => panic!("expected unique match, got {:?}", e),
    }
    // shared prefix "" is ambiguous (both match); use a prefix both share if
    // collision is engineered: skip unless ids collide. Use full id of b:
    match s.resolve_prefix(&b.note_id.to_string()).unwrap() {
        Ok(n) => assert_eq!(n.note_id, b.note_id),
        Err(e) => panic!("exact id must resolve, got {:?}", e),
    }
    // none
    assert!(matches!(
        s.resolve_prefix("ffffffffffffffffffffffff").unwrap(),
        Err(PrefixError::NotFound)
    ));
    let _ = b;
    // engineered ambiguity is covered by prefix_ambiguous_reports_ambiguous
}

#[test]
fn prefix_ambiguous_reports_ambiguous() {
    let dir = tmpdir_live("prefix-amb");
    let s = Store::open(dir.clone(), "user1".to_string()).unwrap();
    add(&s, "one", "c", &[], "AI札");
    add(&s, "two", "c", &[], "AI札");
    let c = rusqlite::Connection::open(dir.join("leanote.db")).unwrap();
    c.execute(
        "UPDATE notes SET NoteId='dddd11111111111111111111' WHERE rowid=1",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE notes SET NoteId='dddd22111111111111111111' WHERE rowid=2",
        [],
    )
    .unwrap();
    drop(c);
    match s.resolve_prefix("dddd").unwrap() {
        Ok(_) => panic!("expected ambiguity"),
        Err(PrefixError::Ambiguous(n)) => assert_eq!(n, 2),
        Err(e) => panic!("wrong error: {:?}", e),
    }
}

#[test]
fn store_path_is_leanote_and_never_touches_nedb() {
    // ADR-0008: data dir is `leanote` on every platform (same dir as the
    // legacy Electron app on macOS); the NeDB `nedb55/` subtree is never
    // read or written.
    let d = app_data_dir();
    let s = d.to_string_lossy();
    assert!(s.ends_with("leanote"), "store dir {} must be leanote", s);
    assert!(!s.contains("leanote-rust"), "no legacy leanote-rust dir");
    assert!(!s.contains("nedb55"), "must never touch NeDB files");
    // and the actual file we write is SQLite, not NeDB JSONL
    let f = d.join(DB_FILE);
    assert_eq!(f.file_name().unwrap(), "leanote.db");
}

#[test]
fn journal_mode_is_wal() {
    let dir = tmpdir_live("wal");
    let s = Store::open(dir.clone(), "user1".to_string()).unwrap();
    assert_eq!(s.journal_mode().unwrap(), "wal");
    add(&s, "w", "c", &[], "AI札");
    // WAL sidecar evidence: a write appends frames before checkpoint
    assert!(
        dir.join("leanote.db-wal").exists(),
        "WAL sidecar must exist after write"
    );
}

#[test]
fn interrupted_write_leaves_previous_state() {
    let dir = tmpdir_live("crash");
    {
        let s = Store::open(dir.clone(), "user1".to_string()).unwrap();
        add(&s, "keep", "stable", &[], "AI札");
    }
    // Truncate the WAL mid-frame = process killed during a write.
    let wal = dir.join("leanote.db-wal");
    if wal.exists() {
        let raw = std::fs::read(&wal).unwrap();
        let mut f = std::fs::OpenOptions::new().write(true).open(&wal).unwrap();
        use std::io::Write;
        let _ = f.write_all(&raw[..raw.len() / 3]);
        let _ = f.set_len(raw.len() as u64 / 3 + 7);
    }
    let s2 = Store::open(dir.clone(), "user1".to_string()).unwrap();
    let notes = s2.list_notes().unwrap();
    assert_eq!(notes.len(), 1, "committed note survives torn WAL");
    assert_eq!(notes[0].title, "keep");
}

#[test]
fn trash_and_deleted_notes_are_hidden() {
    let dir = tmpdir_live("hidden");
    let s = Store::open(dir.clone(), "user1".to_string()).unwrap();
    add(&s, "visible", "c", &[], "AI札");
    let c = rusqlite::Connection::open(dir.join("leanote.db")).unwrap();
    c.execute("INSERT INTO notes (NoteId, UserId, NotebookId, Title, IsTrash) VALUES ('t1','user1','x','trashed',1)", []).unwrap();
    c.execute("INSERT INTO notes (NoteId, UserId, NotebookId, Title, LocalIsDelete) VALUES ('d1','user1','x','deleted',1)", []).unwrap();
    drop(c);
    let titles: Vec<String> = s
        .list_notes()
        .unwrap()
        .into_iter()
        .map(|n| n.title)
        .collect();
    assert_eq!(titles, vec!["visible".to_string()]);
}

#[test]
fn convert_to_markdown_rewrites_content_flags_dirty() {
    let s = store("convert");
    let n = s
        .add_note(NewNote {
            title: "rich".into(),
            content: r#"<h1>Title</h1><ul><li>a</li></ul><p><s>x</s></p>"#.into(),
            tags: vec![],
            notebook_title: "nb".into(),
            is_markdown: false,
        })
        .unwrap();
    // simulate a synced clean row, as it would arrive after full_sync
    s.note_set_not_dirty_not_delete(&n.note_id).unwrap();

    let out = s
        .convert_to_markdown(&n.note_id)
        .unwrap()
        .expect("converted");
    assert!(out.is_markdown);
    assert!(out.is_dirty);
    assert_eq!(out.title, "rich");
    assert!(out.content.contains("# Title"), "{}", out.content);
    assert!(out.content.contains("*   a"), "{}", out.content);
    assert!(out.content.contains("~~x~~"), "{}", out.content);

    // idempotent second call: already Markdown, untouched
    let again = s.convert_to_markdown(&n.note_id).unwrap().unwrap();
    assert_eq!(again.updated_time, out.updated_time);

    // unknown id → None
    assert!(s.convert_to_markdown(&Id("nope".into())).unwrap().is_none());
}

// --- ticket 09: browse queries, star, trash lifecycle ---

#[test]
fn filtered_queries_return_right_sets() {
    let dir = tmpdir_live("browse");
    let s = Store::open(dir.clone(), "user1".to_string()).unwrap();
    let a = add(&s, "A", "c", &["rust"], "Inbox");
    let b = add(&s, "B", "c", &["rust", "gpui"], "Work");
    add(&s, "C", "c", &["design"], "Work");
    // list order is UpdatedTime DESC, rowid ASC. Three inserts inside one
    // wall-clock millisecond tie and then fall to rowid, so the order below
    // would be machine-dependent. Pin distinct times directly (same direct-
    // SQL pattern as trash_and_deleted_notes_are_hidden).
    let db = rusqlite::Connection::open(dir.join("leanote.db")).unwrap();
    db.execute("UPDATE notes SET UpdatedTime=1000 WHERE Title='A'", [])
        .unwrap();
    db.execute("UPDATE notes SET UpdatedTime=2000 WHERE Title='B'", [])
        .unwrap();
    db.execute("UPDATE notes SET UpdatedTime=3000 WHERE Title='C'", [])
        .unwrap();
    drop(db);
    let all: Vec<String> = s
        .list_notes()
        .unwrap()
        .into_iter()
        .map(|n| n.title)
        .collect();
    assert_eq!(all, vec!["C", "B", "A"]);
    let work: Vec<String> = s
        .list_notes_filtered(Some(&b.notebook_id), false)
        .unwrap()
        .into_iter()
        .map(|n| n.title)
        .collect();
    assert_eq!(work.len(), 2);
    assert!(work.contains(&"B".to_string()) && work.contains(&"C".to_string()));
    let inbox: Vec<String> = s
        .list_notes_filtered(Some(&a.notebook_id), false)
        .unwrap()
        .into_iter()
        .map(|n| n.title)
        .collect();
    assert_eq!(inbox, vec!["A"]);
    // tag matches whole tags only
    let rust: Vec<String> = s
        .list_notes_by_tag("rust")
        .unwrap()
        .into_iter()
        .map(|n| n.title)
        .collect();
    assert_eq!(rust, vec!["B", "A"]);
    assert!(s.list_notes_by_tag("ru").unwrap().is_empty());
    let gpui: Vec<String> = s
        .list_notes_by_tag("gpui")
        .unwrap()
        .into_iter()
        .map(|n| n.title)
        .collect();
    assert_eq!(gpui, vec!["B"]);
}

#[test]
fn toggle_star_flips_star_without_setting_isdirty() {
    let s = store("star");
    let n = add(&s, "A", "c", &[], "Inbox");
    assert!(!n.star && n.is_dirty);
    // Simulate a synced (clean) note the way full_sync does: apply_sync_notes
    // writes IsDirty=0. Re-apply the row for the same NoteId.
    let clean_row = serde_json::json!({
        "NoteId": n.note_id.to_string(),
        "NotebookId": n.notebook_id.to_string(),
        "Title": "A",
        "Tags": [],
        "IsMarkdown": true,
        "IsTrash": false,
        "IsDeleted": false,
        "Usn": 1,
    });
    s.apply_sync_notes(&[clean_row], 0).unwrap();
    assert_eq!(s.raw_query_row("SELECT IsDirty FROM notes").unwrap(), 0);
    let to = s.toggle_star(&n.note_id).unwrap().unwrap();
    assert!(to, "first toggle stars");
    let after = s.get_note_any(&n.note_id).unwrap().unwrap();
    assert!(after.star);
    assert!(
        !after.is_dirty,
        "JS Note.star sets Star only, never IsDirty"
    );
    assert_eq!(
        after.updated_time, n.updated_time,
        "starring must not bump UpdatedTime (Recent order is content-driven)"
    );
    let to2 = s.toggle_star(&n.note_id).unwrap().unwrap();
    assert!(!to2, "second toggle unstars");
    let after2 = s.get_note_any(&n.note_id).unwrap().unwrap();
    assert!(!after2.star && !after2.is_dirty);
    // starred filter only sees starred notes
    assert!(s.list_notes_filtered(None, true).unwrap().is_empty());
    s.toggle_star(&n.note_id).unwrap();
    let starred: Vec<String> = s
        .list_notes_filtered(None, true)
        .unwrap()
        .into_iter()
        .map(|n| n.title)
        .collect();
    assert_eq!(starred, vec!["A"]);
    assert!(s.toggle_star(&Id("missing".into())).unwrap().is_none());
}

#[test]
fn set_note2blog_publishes_and_marks_dirty() {
    let s = store("blog");
    let n = add(&s, "A", "c", &[], "Inbox");
    // Clean it the way a sync pull would.
    let clean_row = serde_json::json!({
        "NoteId": n.note_id.to_string(),
        "NotebookId": n.notebook_id.to_string(),
        "Title": "A",
        "Tags": [],
        "IsMarkdown": true,
        "IsTrash": false,
        "IsDeleted": false,
        "Usn": 1,
    });
    s.apply_sync_notes(&[clean_row], 0).unwrap();
    // Publish: IsBlog=1 AND IsDirty=1 (JS setNote2Blog pushes the change).
    s.set_note2blog(&n.note_id, true).unwrap();
    let meta = s
        .list_notes_filtered(None, false)
        .unwrap()
        .into_iter()
        .find(|m| m.note_id == n.note_id)
        .unwrap();
    assert!(meta.is_blog);
    assert_eq!(s.raw_query_row("SELECT IsDirty FROM notes").unwrap(), 1);
    // Publishing is metadata, not content: UpdatedTime must NOT move
    // (else the note jumps to the top of Recent).
    let before: i64 = s.raw_query_row("SELECT UpdatedTime FROM notes").unwrap();
    s.set_note2blog(&n.note_id, true).unwrap(); // no-op republish
    let after: i64 = s.raw_query_row("SELECT UpdatedTime FROM notes").unwrap();
    assert_eq!(before, after);
    // Unpublish round-trips.
    s.set_note2blog(&n.note_id, false).unwrap();
    let meta = s
        .list_notes_filtered(None, false)
        .unwrap()
        .into_iter()
        .find(|m| m.note_id == n.note_id)
        .unwrap();
    assert!(!meta.is_blog);
}

#[test]
fn trash_restore_hard_delete_and_empty() {
    let s = store("trashops");
    let keep = add(&s, "keep", "c", &[], "Inbox");
    let t1 = add(&s, "t1", "c", &[], "Inbox");
    let t2 = add(&s, "t2", "c", &[], "Work");
    let work_nb = s
        .notebooks()
        .unwrap()
        .into_iter()
        .find(|nb| nb.title == "Work")
        .unwrap();
    // trash two
    s.trash_note(&t1.note_id).unwrap();
    s.trash_note(&t2.note_id).unwrap();
    let trash: Vec<String> = s
        .list_trash()
        .unwrap()
        .into_iter()
        .map(|n| n.title)
        .collect();
    assert_eq!(trash.len(), 2);
    assert!(s.list_notes().unwrap().iter().all(|n| n.title == "keep"));
    // restore t2 back into its original notebook
    assert_eq!(
        s.restore_note(&t2.note_id, &work_nb.notebook_id).unwrap(),
        1
    );
    let t2b = s.get_note_any(&t2.note_id).unwrap().unwrap();
    assert!(!t2b.is_trash && !t2b.is_deleted && t2b.notebook_id == work_nb.notebook_id);
    assert!(
        t2b.is_dirty,
        "restore is a server-visible change (JS moveNote sets IsDirty)"
    );
    // hard delete t1: row gone entirely
    assert_eq!(s.hard_delete_note(&t1.note_id).unwrap(), 1);
    assert!(s.get_note_any(&t1.note_id).unwrap().is_none());
    assert_eq!(s.note_count().unwrap(), 2);
    // empty trash clears the rest
    s.trash_note(&keep.note_id).unwrap();
    assert_eq!(s.empty_trash().unwrap(), 1);
    assert!(s.list_trash().is_ok());
    let hidden = s.get_note_any(&keep.note_id).unwrap().unwrap();
    assert!(hidden.is_deleted && hidden.is_dirty && hidden.is_trash);
    // notebook counts only live notes: Inbox's only note is trashed, so the
    // GROUP BY emits no Inbox row at all.
    let counts = s.notebook_note_counts().unwrap();
    let inbox = s
        .notebooks()
        .unwrap()
        .into_iter()
        .find(|nb| nb.title == "Inbox")
        .unwrap();
    assert!(!counts.iter().any(|(id, _)| *id == inbox.notebook_id));
    let work_count = counts
        .iter()
        .find(|(id, _)| *id == work_nb.notebook_id)
        .map(|(_, c)| *c);
    assert_eq!(work_count, Some(1));
}

// ---- Ticket 11: notebook & tag management ----

fn nb_id(s: &Store, title: &str) -> leanote_core::Id {
    s.notebooks()
        .unwrap()
        .into_iter()
        .find(|nb| nb.title == title)
        .unwrap_or_else(|| panic!("notebook {title} missing"))
        .notebook_id
}

/// Notebook ids of one sibling group in Seq order.
fn group_seq(s: &Store, parent: &str) -> Vec<(String, i64)> {
    let mut v: Vec<(String, i64)> = s
        .notebooks()
        .unwrap()
        .into_iter()
        .filter(|nb| nb.parent_notebook_id == parent)
        .map(|nb| (nb.title, nb.seq))
        .collect();
    v.sort_by_key(|(_, seq)| *seq);
    v
}

#[test]
fn notebook_crud_flags_and_seq() {
    let s = store("t11_crud");
    let a = s.create_notebook("A", "").unwrap();
    let b = s.create_notebook("B", "").unwrap();
    assert!(
        a.is_new && a.is_dirty,
        "create sets LocalIsNew+IsDirty (JS addNotebook)"
    );
    assert_eq!((a.seq, b.seq), (0, -1), "Seq = min sibling - 1 (prepend)");
    // nested: seq independent per parent group
    let a1 = s.create_notebook("A1", &a.notebook_id).unwrap();
    assert_eq!(a1.seq, 0);
    assert_eq!(a1.parent_notebook_id, a.notebook_id.to_string());
    // rename: dirty, new title
    assert_eq!(s.rename_notebook(&b.notebook_id, "B2").unwrap(), 1);
    let b2 = s
        .notebooks()
        .unwrap()
        .into_iter()
        .find(|nb| nb.notebook_id == b.notebook_id)
        .unwrap();
    assert_eq!((b2.title.as_str(), b2.is_dirty), ("B2", true));
}
#[test]

fn delete_notebook_guard_semantics() {
    let s = store("t11_delete");
    // A (no notes) with child B containing one note.
    let a = s.create_notebook("A", "").unwrap();
    let child = s.create_notebook("A-child", &a.notebook_id).unwrap();
    add(&s, "n3", "c", &[], "A-child");
    let before = s.note_count().unwrap();

    // A has a live child → guard fires, nothing deleted.
    assert!(s.has_sub_notebooks(&a.notebook_id).unwrap());
    s.delete_notebook(&a.notebook_id).unwrap();
    assert!(s
        .notebooks()
        .unwrap()
        .iter()
        .any(|nb| nb.notebook_id == a.notebook_id));
    assert_eq!(s.note_count().unwrap(), before, "guard deleted nothing");

    // B has live notes → subtree count > 0 (the app's second guard).
    assert!(!s.has_sub_notebooks(&child.notebook_id).unwrap());
    assert_eq!(
        s.notebook_subtree_note_count(&child.notebook_id).unwrap(),
        1
    );

    // Empty notebook deletes cleanly (app reaches it only when both guards pass).
    let e = s.create_notebook("E", "").unwrap();
    assert_eq!(s.notebook_subtree_note_count(&e.notebook_id).unwrap(), 0);
    s.delete_notebook(&e.notebook_id).unwrap();
    assert!(s
        .notebooks()
        .unwrap()
        .iter()
        .all(|nb| nb.notebook_id != e.notebook_id));
    let dirty: i64 = s
        .raw_query_row("SELECT COUNT(*) FROM notebooks WHERE LocalIsDelete=1 AND IsDirty=1")
        .unwrap();
    assert_eq!(dirty, 1, "empty notebook soft-deleted, dirty for the push");
}

#[test]
fn move_notebook_rewrites_both_sibling_groups() {
    let s = store("t11_move");
    let a = s.create_notebook("A", "").unwrap();
    let b = s.create_notebook("B", "").unwrap();
    let c = s.create_notebook("C", "").unwrap();
    let b1 = s.create_notebook("B1", &b.notebook_id).unwrap();
    let _b2 = s.create_notebook("B2", &b.notebook_id).unwrap();

    // 1. reorder within root: C to the front via explicit sibling order
    s.move_notebook(
        &c.notebook_id,
        "",
        0,
        &[
            c.notebook_id.to_string(),
            a.notebook_id.to_string(),
            b.notebook_id.to_string(),
        ],
    )
    .unwrap();
    assert_eq!(
        group_seq(&s, ""),
        vec![("C".into(), 0), ("A".into(), 1), ("B".into(), 2)]
    );
    assert!(
        s.notebooks().unwrap().iter().all(|nb| nb.is_dirty),
        "the whole rewritten group is dirty for the updateNotebook push"
    );

    // 2. reparent by index: A under B at index 1 (between B1 and B2)
    s.move_notebook(&a.notebook_id, &b.notebook_id.to_string(), 1, &[])
        .unwrap();
    assert_eq!(
        group_seq(&s, ""),
        vec![("C".into(), 0), ("B".into(), 1)],
        "old root group re-packed 0..n-1 after A left"
    );
    assert_eq!(
        group_seq(&s, &b.notebook_id.to_string()),
        vec![("B2".into(), 0), ("A".into(), 1), ("B1".into(), 2)],
        "B1 prepended under B, so A@1 lands between B2 and B1"
    );
    let a_row = s
        .notebooks()
        .unwrap()
        .into_iter()
        .find(|nb| nb.title == "A")
        .unwrap();
    assert_eq!(a_row.parent_notebook_id, b.notebook_id.to_string());
    assert!(a_row.is_dirty);
    let _ = b1;
}

#[test]
fn tag_rename_and_delete_are_global() {
    let s = store("t11_tags");
    add(&s, "n1", "c", &["rust", "gpui"], "AI札");
    add(&s, "n2", "c", &["rust"], "AI札");
    add(&s, "n3", "c", &["rustlang", "other"], "AI札");

    // rename: whole-tag match only ("rustlang" untouched), notes dirty
    assert_eq!(s.rename_tag("rust", "rust2").unwrap(), 2);
    let n1 = s
        .get_note_any(&s.list_notes().unwrap()[0].note_id.clone())
        .unwrap();
    let _ = n1;
    let by_title = |t: &str| {
        s.list_notes()
            .unwrap()
            .into_iter()
            .find(|n| n.title == t)
            .unwrap()
    };
    assert_eq!(by_title("n1").tags, vec!["rust2", "gpui"]);
    assert_eq!(by_title("n2").tags, vec!["rust2"]);
    assert_eq!(
        by_title("n3").tags,
        vec!["rustlang", "other"],
        "no substring hit"
    );
    assert!(by_title("n1").is_dirty && by_title("n2").is_dirty);
    // n3 is a fresh add_note (IsDirty=1 from creation); the rename must
    // not touch it — verified by its unchanged Tags.
    let tag_titles: Vec<String> = s.tags().unwrap().into_iter().map(|t| t.tag).collect();
    assert!(tag_titles.contains(&"rust2".to_string()) && !tag_titles.contains(&"rust".to_string()));

    // delete: removed everywhere, tag row gone from live list
    assert_eq!(s.delete_tag("rust2").unwrap(), 2);
    assert_eq!(by_title("n1").tags, vec!["gpui"]);
    assert_eq!(by_title("n2").tags, Vec::<String>::new());
    let live: Vec<String> = s.tags().unwrap().into_iter().map(|t| t.tag).collect();
    assert!(!live.contains(&"rust2".to_string()));
    let deleted_row: i64 = s
        .raw_query_row(
            "SELECT COUNT(*) FROM tags WHERE Tag='rust2' AND LocalIsDelete=1 AND IsDirty=1",
        )
        .unwrap();
    assert_eq!(
        deleted_row, 1,
        "tag row soft-deleted + dirty → deleteTag push"
    );
}

#[test]
fn dirty_flags_ready_for_sync_push() {
    let s = store("t11_push");
    // notebook create/rename/move leave IsDirty=1 rows the push phase reads
    let a = s.create_notebook("A", "").unwrap();
    s.rename_notebook(&a.notebook_id, "A2").unwrap();
    let dirty_nb: i64 = s
        .raw_query_row("SELECT COUNT(*) FROM notebooks WHERE IsDirty=1")
        .unwrap();
    assert_eq!(dirty_nb, 1);
    s.move_notebook(&a.notebook_id, "", 0, &[a.notebook_id.to_string()])
        .unwrap();
    assert_eq!(
        s.raw_query_row("SELECT COUNT(*) FROM notebooks WHERE IsDirty=1")
            .unwrap(),
        1
    );
    // tag ops mark notes dirty
    add(&s, "n", "c", &["t"], "A2");
    s.rename_tag("t", "t2").unwrap();
    s.delete_tag("t2").unwrap();
    assert_eq!(
        s.raw_query_row("SELECT COUNT(*) FROM notes WHERE IsDirty=1")
            .unwrap(),
        1
    );
}

// --- ticket 10: note history ---

fn save(s: &Store, id: &Id, content: &str) {
    s.save_note_with_history(
        id,
        NoteUpdate {
            content: Some(content.to_string()),
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn history_consecutive_identical_saves_dedup() {
    let s = store("hist_dedup");
    let n = add(&s, "T", "v1", &[], "Inbox");
    save(&s, &n.note_id, "v2");
    save(&s, &n.note_id, "v2"); // identical to last snapshot target — no new row
    save(&s, &n.note_id, "v2");
    assert_eq!(
        s.raw_query_row("SELECT COUNT(*) FROM note_history")
            .unwrap(),
        1
    );
    assert_eq!(s.list_history(&n.note_id).unwrap()[0].content, "v1");
    // same content twice in a row with a DIFFERENT other save between → kept
    save(&s, &n.note_id, "v3");
    save(&s, &n.note_id, "v1");
    assert_eq!(
        s.raw_query_row("SELECT COUNT(*) FROM note_history")
            .unwrap(),
        3
    );
}

#[test]
fn history_caps_at_20_rows_dropping_oldest() {
    let s = store("hist_trim");
    let n = add(&s, "T", "v0", &[], "Inbox");
    for i in 1..=25 {
        save(&s, &n.note_id, &format!("v{i}"));
    }
    let h = s.list_history(&n.note_id).unwrap();
    assert_eq!(h.len(), 20);
    assert_eq!(
        h[0].content, "v24",
        "newest snapshot is the last pre-change content"
    );
    assert_eq!(h[19].content, "v5", "oldest five trimmed");
    assert!(!h.iter().any(|x| x.content == "v4"));
}

#[test]
fn restore_makes_old_version_the_latest_and_dirty() {
    let s = store("hist_restore");
    let n = add(&s, "Old title", "old body", &[], "Inbox");
    save(&s, &n.note_id, "new body");
    let h = s.list_history(&n.note_id).unwrap();
    assert_eq!(h.len(), 1);

    let restored = s
        .restore_history(&n.note_id, &h[0].history_id)
        .unwrap()
        .unwrap();
    assert_eq!(restored.content, "old body");
    assert_eq!(restored.title, "Old title");
    assert!(restored.is_dirty, "restore is a server-visible change");

    // restoring records the replaced "new body" as the newest history row
    let h2 = s.list_history(&n.note_id).unwrap();
    assert_eq!(h2[0].content, "new body");
    assert_eq!(h2.len(), 2);
    // and restoring back again flips content again (bounce is lossless)
    let back = s
        .restore_history(&n.note_id, &h2[0].history_id)
        .unwrap()
        .unwrap();
    assert_eq!(back.content, "new body");
    assert!(s
        .restore_history(&n.note_id, &Id("bogus".into()))
        .unwrap()
        .is_none());
}

#[test]
fn hard_delete_purges_history_rows() {
    let s = store("hist_purge");
    let n = add(&s, "T", "v1", &[], "Inbox");
    save(&s, &n.note_id, "v2");
    assert_eq!(
        s.raw_query_row("SELECT COUNT(*) FROM note_history")
            .unwrap(),
        1
    );
    assert_eq!(s.hard_delete_note(&n.note_id).unwrap(), 1);
    assert_eq!(
        s.raw_query_row("SELECT COUNT(*) FROM note_history")
            .unwrap(),
        0
    );
    assert!(s.list_history(&n.note_id).unwrap().is_empty());
    assert!(s.purge_history(&n.note_id).is_ok());
}

#[test]
fn save_with_history_no_change_writes_nothing() {
    let s = store("hist_noop");
    let n = add(&s, "T", "v1", &[], "Inbox");
    let saved = s
        .save_note_with_history(&n.note_id, NoteUpdate::default())
        .unwrap()
        .unwrap();
    assert_eq!(saved.content, "v1");
    assert_eq!(
        s.raw_query_row("SELECT COUNT(*) FROM note_history")
            .unwrap(),
        0
    );
    assert_eq!(s.raw_query_row("SELECT COUNT(*) FROM notes").unwrap(), 1);
}

/// Review item 1: a store opened for a non-"user1" account must see its own
/// dirty notes (sync readiness query is UserId-scoped, not hardcoded).
#[test]
fn store_scopes_dirty_notes_to_opened_user() {
    let s = Store::open(tmpdir_live("scoped-user"), "userX".to_string()).expect("open store");
    let n = add(&s, "Mine", "# x", &[], "NB");
    let dirty: Vec<_> = s.dirty_notes().expect("dirty_notes");
    assert!(
        dirty
            .iter()
            .any(|d| d.note_id.to_string() == n.note_id.to_string()),
        "userX's note must be dirty for userX"
    );

    let other = Store::open(tmpdir_live("scoped-user"), "user1".to_string()).expect("open store");
    assert!(
        other.dirty_notes().expect("dirty_notes").is_empty(),
        "user1 must not see userX's rows"
    );
}

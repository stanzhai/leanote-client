use leanote_core::*;
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("leanote-core-test-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn tmpdir_live(name: &str) -> PathBuf {
    // keep-on-drop variant for tests that reopen the db file directly
    let d = std::env::temp_dir().join(format!("leanote-core-testlive-{}-{}", name, std::process::id()));
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
    c.execute("UPDATE notes SET NoteId='dddd11111111111111111111' WHERE rowid=1", []).unwrap();
    c.execute("UPDATE notes SET NoteId='dddd22111111111111111111' WHERE rowid=2", []).unwrap();
    drop(c);
    match s.resolve_prefix("dddd").unwrap() {
        Ok(_) => panic!("expected ambiguity"),
        Err(PrefixError::Ambiguous(n)) => assert_eq!(n, 2),
        Err(e) => panic!("wrong error: {:?}", e),
    }
}

#[test]
fn store_path_differs_from_electron_nedb_dir() {
    let d = app_data_dir();
    let s = d.to_string_lossy();
    assert!(s.ends_with("leanote-rust"), "store dir {} must be leanote-rust", s);
    assert!(!s.ends_with("leanote/"), "must not be the Electron dir");
    assert!(!s.contains("nedb55"), "must never touch NeDB files");
    // and the actual file we write is SQLite, not .db JSONL of NeDB
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
    let titles: Vec<String> = s.list_notes().unwrap().into_iter().map(|n| n.title).collect();
    assert_eq!(titles, vec!["visible".to_string()]);
}

// --- ticket 09: browse queries, star, trash lifecycle ---

#[test]
fn filtered_queries_return_right_sets() {
    let s = store("browse");
    let a = add(&s, "A", "c", &["rust"], "Inbox");
    let b = add(&s, "B", "c", &["rust", "gpui"], "Work");
    add(&s, "C", "c", &["design"], "Work");
    // newest-first ordering (B then A, by UpdatedTime tie → rowid)
    let all: Vec<String> = s.list_notes().unwrap().into_iter().map(|n| n.title).collect();
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
    let rust: Vec<String> = s.list_notes_by_tag("rust").unwrap().into_iter().map(|n| n.title).collect();
    assert_eq!(rust, vec!["B", "A"]);
    assert!(s.list_notes_by_tag("ru").unwrap().is_empty());
    let gpui: Vec<String> = s.list_notes_by_tag("gpui").unwrap().into_iter().map(|n| n.title).collect();
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
    assert!(!after.is_dirty, "JS Note.star sets Star+UpdatedTime only, never IsDirty");
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
fn trash_restore_hard_delete_and_empty() {
    let s = store("trashops");
    let keep = add(&s, "keep", "c", &[], "Inbox");
    let t1 = add(&s, "t1", "c", &[], "Inbox");
    let t2 = add(&s, "t2", "c", &[], "Work");
    let work_nb = s.notebooks().unwrap().into_iter().find(|nb| nb.title == "Work").unwrap();
    // trash two
    s.trash_note(&t1.note_id).unwrap();
    s.trash_note(&t2.note_id).unwrap();
    let trash: Vec<String> = s.list_trash().unwrap().into_iter().map(|n| n.title).collect();
    assert_eq!(trash.len(), 2);
    assert!(s.list_notes().unwrap().iter().all(|n| n.title == "keep"));
    // restore t2 back into its original notebook
    assert_eq!(s.restore_note(&t2.note_id, &work_nb.notebook_id).unwrap(), 1);
    let t2b = s.get_note_any(&t2.note_id).unwrap().unwrap();
    assert!(!t2b.is_trash && !t2b.is_deleted && t2b.notebook_id == work_nb.notebook_id);
    assert!(t2b.is_dirty, "restore is a server-visible change (JS moveNote sets IsDirty)");
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
    let inbox = s.notebooks().unwrap().into_iter().find(|nb| nb.title == "Inbox").unwrap();
    assert!(!counts.iter().any(|(id, _)| *id == inbox.notebook_id));
    let work_count = counts.iter().find(|(id, _)| *id == work_nb.notebook_id).map(|(_, c)| *c);
    assert_eq!(work_count, Some(1));
}

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

    let out = s.convert_to_markdown(&n.note_id).unwrap().expect("converted");
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

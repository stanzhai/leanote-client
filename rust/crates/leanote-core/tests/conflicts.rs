use leanote_core::*;
use serde_json::json;
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("leanote-core-conf-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn store(name: &str) -> Store {
    Store::open(tmpdir(name), "user1".to_string()).expect("open store")
}

/// Local note + a recorded conflict against server note "note0042".
fn seeded(name: &str) -> (Store, Id) {
    let s = store(name);
    let n = s
        .add_note(NewNote {
            title: "Note 42".into(),
            content: "# Note 42\n\nlocal edit".into(),
            tags: vec![],
            notebook_title: "AI札".into(),
            is_markdown: true,
        })
        .unwrap();
    s.update_note(&n.note_id, NoteUpdate { content: Some("# Note 42\n\nlocal edit v2".into()), ..Default::default() })
        .unwrap();
    s.record_conflict(
        "note0042",
        &n.note_id.0,
        &json!({"NoteId":"note0042","NotebookId":"nb2","Title":"Note 42 server","Usn":1500,
                "UpdatedTime":"2026-01-03T10:00:00Z","IsMarkdown":true,"Tags":[]}),
        "Note 42",
        "# Note 42\n\nlocal edit v2",
        "# Note 42\n\nserver edit",
    )
    .unwrap();
    (s, n.note_id)
}

#[test]
fn list_conflicts_returns_typed_rows() {
    let (s, local_id) = seeded("list");
    let rows = s.list_conflicts().unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.server_note_id, "note0042");
    assert_eq!(r.local_note_id, local_id.0);
    assert_eq!(r.server_title, "Note 42 server");
    assert_eq!(r.server_content, "# Note 42\n\nserver edit");
    assert_eq!(r.local_title, "Note 42");
    assert_eq!(r.local_content, "# Note 42\n\nlocal edit v2");
    assert!(r.server_updated_ms > 0);
}

#[test]
fn record_conflict_twice_does_not_accumulate() {
    let (s, local_id) = seeded("noacc");
    s.record_conflict(
        "note0042",
        &local_id.0,
        &json!({"NoteId":"note0042","Title":"Note 42 server v2","Usn":1501,
                "UpdatedTime":"2026-01-04T10:00:00Z"}),
        "Note 42",
        "# Note 42\n\nlocal edit v3",
        "# Note 42\n\nserver edit v2",
    )
    .unwrap();
    let rows = s.list_conflicts().unwrap();
    assert_eq!(rows.len(), 1, "upsert keyed on ServerNoteId must not duplicate");
    assert_eq!(rows[0].server_title, "Note 42 server v2");
    assert_eq!(rows[0].local_content, "# Note 42\n\nlocal edit v3");
}

#[test]
fn resolve_use_local_keeps_local_and_clears_conflict() {
    let (s, local_id) = seeded("use_local");
    assert!(s.resolve_conflict_use_local("note0042").unwrap(), "existing row");
    assert!(s.list_conflicts().unwrap().is_empty());
    let n = s.get_note_any(&local_id).unwrap().unwrap();
    assert_eq!(n.title, "Note 42");
    assert_eq!(n.content, "# Note 42\n\nlocal edit v2");
    assert!(n.is_dirty, "stays dirty so the next sync force-pushes it");
    assert!(!n.is_deleted);
    assert_eq!(s.raw_query_row("SELECT COUNT(*) FROM sync_conflicts").unwrap(), 0);
    // unknown id: no-op, reported as not-found
    assert!(!s.resolve_conflict_use_local("nope").unwrap());
}

#[test]
fn resolve_use_server_overwrites_local_and_clears_dirty() {
    let (s, local_id) = seeded("use_server");
    assert!(s.resolve_conflict_use_server("note0042").unwrap());
    let n = s.get_note_any(&local_id).unwrap().unwrap();
    assert_eq!(n.title, "Note 42 server", "title from the server version");
    assert_eq!(n.content, "# Note 42\n\nserver edit");
    assert!(!n.is_dirty);
    assert!(!n.is_new);
    assert!(!n.is_deleted);
    assert_eq!(
        s.raw_query_row(&format!("SELECT Usn FROM notes WHERE NoteId='{}'", local_id.0)).unwrap(),
        1500,
        "usn from server row so the next pull is quiet"
    );
    assert!(s.list_conflicts().unwrap().is_empty());
}

#[test]
fn resolve_keep_both_splits_into_server_note_and_new_local() {
    let (s, local_id) = seeded("keep_both");
    let new_id = s.resolve_conflict_keep_both("note0042", "（本地副本）").unwrap().expect("new note id");

    // original row takes the server version, clean
    let orig = s.get_note_any(&local_id).unwrap().unwrap();
    assert_eq!(orig.title, "Note 42 server");
    assert_eq!(orig.content, "# Note 42\n\nserver edit");
    assert!(!orig.is_dirty && !orig.is_new);

    // local version becomes a new note: new id, suffix, dirty, same notebook
    assert_ne!(new_id.0, local_id.0);
    assert_eq!(
        s.raw_query_row(&format!(
            "SELECT COUNT(*) FROM notes WHERE NoteId='{}' AND ServerNoteId IS NULL",
            new_id.0
        ))
        .unwrap(),
        1,
        "new row is local-only until pushed via addNote"
    );
    assert!(s.list_conflicts().unwrap().is_empty());
}

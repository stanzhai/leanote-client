//! Full-sync acceptance against the in-process fake server:
//! login persistence, dataset parity, idempotent re-run, interrupted
//! (SIGKILL) re-run, `leanote sync` smoke.

use serde_json::{json, Map, Value};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use leanote_core::sync::{full_sync, Api, NoProgress, SyncError};
use leanote_core::Store;
use leanote_fakeserver::{start, EMAIL, PWD, USER_ID};

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "leanote-sync-{}-{}-{}",
        name,
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Multi-chunk dataset: >MAX_ENTRY notes forces several note chunks.
fn dataset() -> (Vec<Value>, Vec<Value>, Vec<Value>, Map<String, Value>) {
    let mut notebooks = Vec::new();
    for i in 1..=3 {
        notebooks.push(json!({
            "NotebookId": format!("nb{i}"), "Title": format!("Notebook {i}"),
            "Seq": i, "ParentNotebookId": if i == 2 { "nb1".to_string() } else { String::new() },
            "Usn": i, "IsDeleted": false, "IsDirty": false,
            "CreatedTime": "2026-01-01T00:00:00Z", "UpdatedTime": "2026-01-01T00:00:00Z",
        }));
    }
    // server-deleted notebook
    notebooks.push(json!({
        "NotebookId": "nbgone", "Title": "Gone", "Seq": 4, "ParentNotebookId": "",
        "Usn": 4, "IsDeleted": true, "IsDirty": false,
        "CreatedTime": "2026-01-01T00:00:00Z", "UpdatedTime": "2026-01-01T00:00:00Z",
    }));

    let mut notes = Vec::new();
    let mut contents = Map::new();
    for i in 0..450 {
        let id = format!("note{i:04}");
        notes.push(json!({
            "NoteId": id, "NotebookId": format!("nb{}", i % 3 + 1),
            "Title": format!("Note {i}"),
            "IsTrash": false, "IsMarkdown": true,
            "Tags": if i % 10 == 0 { vec!["shared".to_string()] } else { vec![] },
            "Files": [], "IsDeleted": false, "IsDirty": false,
            "CreatedTime": "2026-01-02T00:00:00Z", "UpdatedTime": "2026-01-02T00:00:00Z",
            "Usn": 10 + i,
        }));
        contents.insert(id, json!(format!("# Note {i}\n\nbody {i}")));
    }
    // server-deleted note
    notes.push(json!({
        "NoteId": "notegone", "NotebookId": "nb1", "Title": "Gone",
        "IsTrash": false, "IsMarkdown": true, "Tags": [], "Files": [],
        "IsDeleted": true, "IsDirty": false,
        "CreatedTime": "2026-01-02T00:00:00Z", "UpdatedTime": "2026-01-02T00:00:00Z",
        "Usn": 1000,
    }));

    let tags = vec![
        json!({"TagId": "t1", "Tag": "shared", "Usn": 600, "IsDeleted": false}),
        json!({"TagId": "t2", "Tag": "gone", "Usn": 601, "IsDeleted": true}),
    ];
    (notebooks, notes, tags, contents)
}

async fn seeded() -> leanote_fakeserver::FakeServer {
    let srv = start().await;
    let (nb, n, t, c) = dataset();
    srv.set_notebooks(nb);
    srv.set_notes(n);
    srv.set_tags(t);
    srv.set_contents(c);
    srv.set_sync_usn(1001);
    srv
}

fn store(dir: &PathBuf) -> Store {
    Store::open(dir.clone(), USER_ID.to_string()).unwrap()
}

fn counts(s: &Store) -> (i64, i64, i64, i64) {
    (
        s.raw_query_row("SELECT COUNT(*) FROM notebooks").unwrap(),
        s.raw_query_row("SELECT COUNT(*) FROM notes").unwrap(),
        s.raw_query_row("SELECT COUNT(*) FROM tags").unwrap(),
        s.raw_query_row("SELECT COUNT(*) FROM notes WHERE Content != ''").unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn login_persists_token_and_wrong_password_errors() {
    let srv = seeded().await;
    let dir = tmpdir("login");
    let s = store(&dir);

    // wrong password → server Msg, like JS surfacing the login failure
    let err = Api::login(&srv.url(), EMAIL, "wrong", &s).unwrap_err();
    assert!(matches!(&err, SyncError::Api(m) if m == "WrongPassword"), "got {err:?}");
    assert!(s.active_account().unwrap().is_none());

    // ok login persists the token + account, single active row
    let acct = Api::login(&srv.url(), EMAIL, PWD, &s).unwrap();
    assert_eq!(acct.token, srv.token());
    assert_eq!(acct.user_id, USER_ID);
    let stored = s.active_account().unwrap().unwrap();
    assert_eq!(stored.token, srv.token());
    assert_eq!(stored.email, EMAIL);
    assert_eq!(stored.host, srv.url());
    // pre-sync LastSyncUsn is the JS default −1
    assert_eq!(s.last_sync_usn().unwrap(), -1);
}

#[tokio::test(flavor = "multi_thread")]
async fn full_sync_dataset_parity_and_last_sync_usn() {
    let srv = seeded().await;
    let dir = tmpdir("parity");
    let s = store(&dir);
    let acct = Api::login(&srv.url(), EMAIL, PWD, &s).unwrap();

    let stats = full_sync(&Api::for_account(&acct), &s, &NoProgress).unwrap();
    assert_eq!(stats.notebooks, 4);
    assert_eq!(stats.notes, 451);
    assert_eq!(stats.tags, 2);
    assert_eq!(stats.contents, 450);

    let (nb, n, t, c) = counts(&s);
    // deleted-on-server rows must not appear as live rows
    assert_eq!(nb, 3, "3 live notebooks (nbgone deleted)");
    assert_eq!(n, 450, "450 live notes (notegone deleted)");
    assert_eq!(t, 2, "both tag rows kept; gone marked LocalIsDelete");
    assert_eq!(c, 450, "every live note has content");

    assert_eq!(s.last_sync_usn().unwrap(), 1001);

    // content actually landed
    let note = s.get_note(&leanote_core::Id("note0010".into())).unwrap().unwrap();
    assert_eq!(note.content, "# Note 10\n\nbody 10");
    assert_eq!(note.tags, vec!["shared".to_string()]);
    // no note content lost: every live note's content matches the dataset
    for i in 0..450 {
        let id = leanote_core::Id(format!("note{i:04}"));
        assert_eq!(
            s.get_note(&id).unwrap().unwrap().content,
            format!("# Note {i}\n\nbody {i}"),
            "note {i} content"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn rerun_is_idempotent_no_dup_rows_no_lost_content() {
    let srv = seeded().await;
    let dir = tmpdir("rerun");
    let s = store(&dir);
    let acct = Api::login(&srv.url(), EMAIL, PWD, &s).unwrap();

    full_sync(&Api::for_account(&acct), &s, &NoProgress).unwrap();
    let first = counts(&s);
    let api = Api::for_account(&acct);
    let stats = full_sync(&api, &s, &NoProgress).unwrap();

    // re-run re-pulls everything but upserts in place
    assert_eq!(stats.notebooks, 4);
    assert_eq!(stats.notes, 451);
    assert_eq!(stats.contents, 0, "content already present, not re-fetched");
    let second = counts(&s);
    assert_eq!(first, second);
    let n = s.get_note(&leanote_core::Id("note0042".into())).unwrap().unwrap();
    assert_eq!(n.content, "# Note 42\n\nbody 42");
    assert_eq!(s.last_sync_usn().unwrap(), 1001);
}

#[tokio::test(flavor = "multi_thread")]
async fn killed_mid_sync_rerun_recovers_consistent() {
    // SIGKILL a real `leanote sync` subprocess right after the first note
    // chunk commits (progress lines are unbuffered), then re-run to
    // convergence.
    let srv = seeded().await;
    let dir = tmpdir("killed");
    let bin = env!("CARGO_BIN_EXE_leanote");

    let mut child = Command::new(bin)
        .args(["sync", "--host", &srv.url(), "--email", EMAIL, "--password", PWD])
        .env("LEANOTE_RUST_DATA", &dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut out = child.stdout.take().unwrap();
    let mut seen = String::new();
    let mut buf = [0u8; 256];
    // read until the first notes-chunk commit is reported
    loop {
        let n = out.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        seen.push_str(&String::from_utf8_lossy(&buf[..n]));
        if seen.contains("notes: +") {
            break;
        }
    }
    child.kill().unwrap();
    let _ = child.wait();

    // Partial state must be internally consistent (no torn chunk).
    let s = store(&dir);
    let (nb, _, _, c) = counts(&s);
    assert!(nb > 0, "notebooks already committed before the kill");
    assert!(c == 0, "content phase never reached");

    // Re-run to convergence.
    let status = Command::new(bin)
        .args(["sync", "--host", &srv.url(), "--email", EMAIL, "--password", PWD])
        .env("LEANOTE_RUST_DATA", &dir)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stdout)
    );

    let (nb, n, t, c) = counts(&s);
    assert_eq!((nb, n, t, c), (3, 450, 2, 450));
    // each login past the first bumps the server's sync_usn by one
    assert_eq!(s.last_sync_usn().unwrap(), 1002);
    for i in 0..450 {
        let id = leanote_core::Id(format!("note{i:04}"));
        assert_eq!(
            s.get_note(&id).unwrap().unwrap().content,
            format!("# Note {i}\n\nbody {i}"),
            "note {i} content after crash+rerun"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sync_cli_smoke_prints_progress() {
    let srv = seeded().await;
    let dir = tmpdir("smoke");
    let bin = env!("CARGO_BIN_EXE_leanote");

    let out = Command::new(bin)
        .args([
            "sync",
            "--host",
            &srv.url(),
            "--email",
            EMAIL,
            "--password",
            PWD,
        ])
        .env("LEANOTE_RUST_DATA", &dir)
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("notebooks: +4"), "{text}");
    assert!(text.contains("LastSyncUsn=1001"), "{text}");

    // and a wrong-password run exits non-zero with the server Msg
    let bad = Command::new(bin)
        .args(["sync", "--host", &srv.url(), "--email", EMAIL, "--password", "nope"])
        .env("LEANOTE_RUST_DATA", &dir)
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("WrongPassword"));
}

// ---- incr sync (ticket 04) ----

use leanote_core::sync::incr_sync;
use leanote_core::{Id, NewNote, NoteUpdate};
use rusqlite::Connection;

/// seeded server + full_sync'd store, ready for incr tests.
async fn synced(name: &str) -> (leanote_fakeserver::FakeServer, PathBuf, Store, Api) {
    let srv = seeded().await;
    let dir = tmpdir(name);
    let s = store(&dir);
    let acct = Api::login(&srv.url(), EMAIL, PWD, &s).unwrap();
    let api = Api::for_account(&acct);
    full_sync(&api, &s, &NoProgress).unwrap();
    assert_eq!(s.last_sync_usn().unwrap(), 1001);
    (srv, dir, s, api)
}

fn find_push<'a>(
    pushes: &'a [leanote_fakeserver::RecordedPush],
    endpoint: &str,
) -> &'a leanote_fakeserver::RecordedPush {
    pushes
        .iter()
        .find(|p| p.endpoint == endpoint)
        .unwrap_or_else(|| panic!("no push {endpoint} in {pushes:?}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn incr_push_offline_new_and_edit() {
    let (srv, _dir, s, api) = synced("incr_new_edit").await;

    s.add_note(NewNote {
        title: "Local new".into(),
        content: "# new".into(),
        tags: vec![],
        notebook_title: "Notebook 1".into(),
        is_markdown: true,
    })
    .unwrap();
    s.update_note(
        &Id("note0042".into()),
        NoteUpdate { content: Some("# Note 42\n\nlocal edit".into()), ..Default::default() },
    )
    .unwrap();

    incr_sync(&api, &s, &NoProgress).unwrap();

    let pushes = srv.pushes();
    let up = find_push(&pushes, "note/updateNote");
    assert_eq!(up.fields["NoteId"].as_str().unwrap(), "note0042");
    assert_eq!(up.fields["Title"].as_str().unwrap(), "Note 42");
    let add = find_push(&pushes, "note/addNote");
    assert_eq!(add.fields["Title"].as_str().unwrap(), "Local new");
    assert_eq!(add.fields["Content"].as_str().unwrap(), "# new");

    // new note: clean + mapped to a server id
    assert_eq!(
        s.raw_query_row(
            "SELECT COUNT(*) FROM notes WHERE Title='Local new' AND IsDirty=0 AND LocalIsNew=0
             AND ServerNoteId LIKE 'srvnote%'"
        )
        .unwrap(),
        1
    );
    // edited note: clean + usn advanced
    assert_eq!(
        s.raw_query_row(
            "SELECT COUNT(*) FROM notes WHERE NoteId='note0042' AND IsDirty=0 AND Usn=1002
             AND Content='# Note 42\n\nlocal edit'"
        )
        .unwrap(),
        1
    );
    assert_eq!(counts(&s), (3, 451, 2, 451));
    assert_eq!(s.last_sync_usn().unwrap(), 1003);
}

#[tokio::test(flavor = "multi_thread")]
async fn incr_push_hard_delete_sends_delete_trash() {
    let (srv, dir, s, api) = synced("incr_hard_delete").await;

    let conn = Connection::open(dir.join("leanote.db")).unwrap();
    conn.execute(
        "UPDATE notes SET IsTrash=1, LocalIsDelete=1, IsDirty=1 WHERE NoteId='note0042'",
        [],
    )
    .unwrap();

    incr_sync(&api, &s, &NoProgress).unwrap();

    let pushes = srv.pushes();
    let del = find_push(&pushes, "note/deleteTrash");
    assert_eq!(del.fields["noteId"].as_str().unwrap(), "note0042");
    assert_eq!(s.raw_query_row("SELECT COUNT(*) FROM notes WHERE NoteId='note0042'").unwrap(), 0);
    assert_eq!(counts(&s).1, 449);
}

#[tokio::test(flavor = "multi_thread")]
async fn incr_usn_advance_re_rounds_to_convergence() {
    let (srv, _dir, s, api) = synced("incr_re_round").await;

    for i in [42, 43] {
        s.update_note(
            &Id(format!("note{i:04}")),
            NoteUpdate { content: Some(format!("# Note {i}\n\nlocal edit")), ..Default::default() },
        )
        .unwrap();
    }
    // first push answers with a far-future usn → needAgain → re-round pulls
    // (nothing new) and skips the second push round
    srv.inject_response(json!({"Ok": true, "NoteId": "note0042", "Usn": 5000}));

    incr_sync(&api, &s, &NoProgress).unwrap();

    assert_eq!(srv.pushes().len(), 2);
    assert_eq!(
        s.raw_query_row(
            "SELECT COUNT(*) FROM notes WHERE NoteId IN ('note0042','note0043') AND IsDirty=0"
        )
        .unwrap(),
        2
    );
    // converged: local cursor == server LastSyncUsn (only the real push bumped it)
    assert_eq!(s.last_sync_usn().unwrap(), api.get_sync_state().unwrap());
    assert_eq!(s.last_sync_usn().unwrap(), 1002);
}

#[tokio::test(flavor = "multi_thread")]
async fn incr_delete_conflict_sets_not_dirty_keeps_row() {
    let (srv, dir, s, api) = synced("incr_del_conflict").await;

    let conn = Connection::open(dir.join("leanote.db")).unwrap();
    conn.execute(
        "UPDATE notes SET IsTrash=1, LocalIsDelete=1, IsDirty=1 WHERE NoteId='note0042'",
        [],
    )
    .unwrap();
    srv.inject_response(json!({"Ok": false, "Msg": "conflict"}));

    incr_sync(&api, &s, &NoProgress).unwrap();

    let pushes = srv.pushes();
    let del = find_push(&pushes, "note/deleteTrash");
    assert_eq!(del.fields["noteId"].as_str().unwrap(), "note0042");
    // 置非脏不删: row survives, not dirty, not deleted (still trash)
    let n = s.get_note_any(&Id("note0042".into())).unwrap().unwrap();
    assert!(!n.is_dirty);
    assert!(!n.is_deleted);
    assert!(n.is_trash);
    assert_eq!(counts(&s).1, 450);
}

#[tokio::test(flavor = "multi_thread")]
async fn incr_content_conflict_recorded_not_overwritten() {
    let (srv, _dir, s, api) = synced("incr_content_conflict").await;

    // local edit (dirty)…
    s.update_note(
        &Id("note0042".into()),
        NoteUpdate { content: Some("# Note 42\n\nlocal edit".into()), ..Default::default() },
    )
    .unwrap();
    // …racing a server edit at a higher usn with different content
    let (mut nb, mut notes, tags, mut contents) = dataset();
    nb.clear();
    for n in notes.iter_mut() {
        if n["NoteId"] == json!("note0042") {
            n["Usn"] = json!(1500);
        }
    }
    contents.insert("note0042".into(), json!("# Note 42\n\nserver edit"));
    srv.set_notebooks(nb);
    srv.set_notes(notes);
    srv.set_tags(tags);
    srv.set_contents(contents);
    srv.set_sync_usn(1002);
    // the updateNote push also answers conflict → local stays dirty
    srv.inject_response(json!({"Ok": false, "Msg": "conflict"}));

    incr_sync(&api, &s, &NoProgress).unwrap();

    let conflicts = s.conflicts().unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0]["server"]["NoteId"].as_str().unwrap(), "note0042");
    assert_eq!(conflicts[0]["server"]["Usn"].as_i64().unwrap(), 1500);
    assert_eq!(
        conflicts[0]["local"]["Content"].as_str().unwrap(),
        "# Note 42\n\nlocal edit"
    );
    // local row untouched by the server version
    let n = s.get_note_any(&Id("note0042".into())).unwrap().unwrap();
    assert_eq!(n.content, "# Note 42\n\nlocal edit");
    assert!(n.is_dirty, "conflict keeps the local row dirty");
}

#[tokio::test(flavor = "multi_thread")]
async fn incr_pull_delta_only_no_duplicates_on_rerun() {
    let (srv, _dir, s, api) = synced("incr_delta").await;

    let (nb, mut notes, tags, mut contents) = dataset();
    notes.push(json!({
        "NoteId": "note0500", "NotebookId": "nb2", "Title": "Note 500",
        "IsTrash": false, "IsMarkdown": true, "Tags": [], "Files": [],
        "IsDeleted": false, "IsDirty": false,
        "CreatedTime": "2026-01-02T00:00:00Z", "UpdatedTime": "2026-01-02T00:00:00Z",
        "Usn": 1500,
    }));
    contents.insert("note0500".into(), json!("# Note 500\n\nbody 500"));
    srv.set_notebooks(nb);
    srv.set_notes(notes);
    srv.set_tags(tags);
    srv.set_contents(contents);
    srv.set_sync_usn(1002);

    incr_sync(&api, &s, &NoProgress).unwrap();

    assert_eq!(counts(&s), (3, 451, 2, 451));
    let n = s.get_note_any(&Id("note0500".into())).unwrap().unwrap();
    assert_eq!(n.content, "# Note 500\n\nbody 500");
    let last = s.last_sync_usn().unwrap();
    assert_eq!(last, 1002);
    assert!(srv.pushes().is_empty(), "pull-only round pushes nothing");

    // rerun: nothing new server-side, nothing dirty → no traffic, no dupes
    srv.clear_pushes();
    incr_sync(&api, &s, &NoProgress).unwrap();
    assert_eq!(counts(&s), (3, 451, 2, 451));
    assert!(srv.pushes().is_empty());
    assert_eq!(s.last_sync_usn().unwrap(), last);
}

/// Ticket 12 end-to-end: legacy rich-text note → read-side convert →
/// explicit 转存 → incr sync pushes updateNote with the converted Markdown.
#[tokio::test(flavor = "multi_thread")]
async fn convert_to_markdown_pushes_converted_content() {
    let (srv, _dir, s, api) = synced("convert_e2e").await;

    // a rich-text note straight off the server: IsMarkdown=false, HTML body
    let html = concat!(
        "<h2>归档</h2>",
        "<ul><li>要点<ol><li>子项</li></ol></li></ul>",
        "<table><thead><tr><th>列A</th><th>列B</th></tr></thead>",
        "<tbody><tr><td>1</td><td>2</td></tr></tbody></table>",
        "<p><strong>粗</strong> <em>斜</em> <code>x</code> <s>删</s></p>",
        "<blockquote><p>引</p></blockquote>",
    );
    let rich_id = "note0500";
    srv.set_notes({
        let mut rows = dataset().0;
        rows.push(json!({
            "NoteId": rich_id, "NotebookId": "nb1", "Title": "Rich",
            "IsTrash": false, "IsMarkdown": false, "Tags": [], "Files": [],
            "IsDeleted": false, "IsDirty": false,
            "CreatedTime": "2026-01-02T00:00:00Z", "UpdatedTime": "2026-01-02T00:00:00Z",
            "Usn": 1000,
        }));
        rows
    });
    let mut contents = Map::new();
    contents.insert(rich_id.to_string(), json!(html));
    srv.set_contents(contents);
    srv.set_sync_usn(1002);

    // pull the rich note down (IsMarkdown=false lands as-is)
    full_sync(&api, &s, &NoProgress).unwrap();
    let note = s.get_note(&Id(rich_id.into())).unwrap().expect("synced");
    assert!(!note.is_markdown);
    assert_eq!(note.content, html);

    // explicit convert (the GUI's 转存为 Markdown action)
    let converted = s.convert_to_markdown(&Id(rich_id.into())).unwrap().unwrap();
    assert!(converted.is_markdown && converted.is_dirty);
    let expected = leanote_core::html2md::html_to_markdown(html);
    assert!(expected.contains("## 归档") && expected.contains("~~删~~"));

    incr_sync(&api, &s, &NoProgress).unwrap();

    let pushes = srv.pushes();
    let up = find_push(&pushes, "note/updateNote");
    assert_eq!(up.fields["NoteId"].as_str().unwrap(), rich_id);
    assert_eq!(up.fields["Content"].as_str().unwrap(), expected);
    // note is clean again; IsMarkdown stays local (server keeps it false, JS parity)
    assert_eq!(
        s.raw_query_row(
            "SELECT COUNT(*) FROM notes WHERE NoteId='note0500' AND IsDirty=0 AND IsMarkdown=1"
        )
        .unwrap(),
        1
    );
}

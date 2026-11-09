//! Smoke tests: start server, login ok/bad, replay a full dataset, record pushes.

use serde_json::{json, Map, Value};

use leanote_fakeserver::{start, EMAIL, PWD};

async fn get(url: &str) -> Value {
    let body = reqwest::get(url).await.unwrap().text().await.unwrap();
    serde_json::from_str(&body).unwrap_or(Value::Null)
}

async fn post(url: &str, form: &str) -> Value {
    let client = reqwest::Client::new();
    let body = client
        .post(url)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form.to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    serde_json::from_str(&body).unwrap_or(Value::Null)
}

/// Dataset mirroring what `src/sync.js` requests/parses:
/// notebooks/notebooks/tags rows with USN-ascending chunks, notes whose
/// content lives server-side, one deleted row of each kind.
fn dataset() -> (Vec<Value>, Vec<Value>, Vec<Value>, Map<String, Value>) {
    let notebooks = vec![
        json!({
            "NotebookId": "nb1", "Title": "AI札", "Seq": 1, "ParentNotebookId": "",
            "Usn": 1, "IsDeleted": false, "IsDirty": false,
            "CreatedTime": "2026-01-01T00:00:00Z", "UpdatedTime": "2026-01-01T00:00:00Z",
        }),
        json!({
            "NotebookId": "nb2", "Title": "Work", "Seq": 2, "ParentNotebookId": "nb1",
            "Usn": 2, "IsDeleted": false, "IsDirty": false,
            "CreatedTime": "2026-01-02T00:00:00Z", "UpdatedTime": "2026-01-02T00:00:00Z",
        }),
        json!({
            "NotebookId": "nb3", "Title": "Gone", "Seq": 3, "ParentNotebookId": "",
            "Usn": 5, "IsDeleted": true, "IsDirty": false,
            "CreatedTime": "2026-01-03T00:00:00Z", "UpdatedTime": "2026-01-03T00:00:00Z",
        }),
    ];
    let notes = vec![
        json!({
            "NoteId": "n1", "NotebookId": "nb1", "Title": "First",
            "IsTrash": false, "IsMarkdown": true, "Tags": ["ai"], "Files": [],
            "IsDeleted": false, "IsDirty": false,
            "CreatedTime": "2026-01-04T00:00:00Z", "UpdatedTime": "2026-01-04T00:00:00Z",
            "Usn": 3,
        }),
        json!({
            "NoteId": "n2", "NotebookId": "nb2", "Title": "Second",
            "IsTrash": false, "IsMarkdown": true, "Tags": [], "Files": [],
            "IsDeleted": false, "IsDirty": false,
            "CreatedTime": "2026-01-05T00:00:00Z", "UpdatedTime": "2026-01-05T00:00:00Z",
            "Usn": 4,
        }),
        json!({
            "NoteId": "n3", "NotebookId": "nb1", "Title": "Deleted on server",
            "IsTrash": false, "IsMarkdown": true, "Tags": [], "Files": [],
            "IsDeleted": true, "IsDirty": false,
            "CreatedTime": "2026-01-06T00:00:00Z", "UpdatedTime": "2026-01-06T00:00:00Z",
            "Usn": 7,
        }),
    ];
    let tags = vec![
        json!({"TagId": "t1", "Tag": "ai", "Usn": 6, "IsDeleted": false}),
        json!({"TagId": "t2", "Tag": "gone", "Usn": 8, "IsDeleted": true}),
    ];
    let mut contents = Map::new();
    contents.insert("n1".into(), json!("# First\n\nbody"));
    contents.insert("n2".into(), json!("# Second\n\nbody"));
    (notebooks, notes, tags, contents)
}

#[tokio::test]
async fn login_ok_and_bad_password() {
    let srv = start().await;
    // bad password
    let bad = post(
        &format!("{}/api/auth/login?token=&macAddr=m&v=1", srv.url()),
        &format!("email={EMAIL}&pwd=wrong"),
    )
    .await;
    assert_eq!(bad["Ok"], json!(false));
    assert_eq!(bad["Msg"], json!("WrongPassword"));
    // good password issues a token
    let ok = post(
        &format!("{}/api/auth/login?token=&macAddr=m&v=1", srv.url()),
        &format!("email={EMAIL}&pwd={PWD}"),
    )
    .await;
    assert_eq!(ok["Ok"], json!(true));
    assert_eq!(ok["UserId"], json!("user1"));
    assert_eq!(ok["Token"], json!("faketoken42"));
    // sync endpoints refuse without the token
    let notlogin = get(&format!("{}/api/user/getSyncState?token=bad", srv.url())).await;
    assert_eq!(notlogin["Msg"], json!("NOTLOGIN"));
    // and accept with it
    let state = get(&format!(
        "{}/api/user/getSyncState?token={}",
        srv.url(),
        srv.token()
    ))
    .await;
    assert_eq!(state["Ok"], json!(true));
}

#[tokio::test]
async fn full_dataset_replay_with_paging() {
    let srv = start().await;
    post(
        &format!("{}/api/auth/login?token=", srv.url()),
        &format!("email={EMAIL}&pwd={PWD}"),
    )
    .await;
    let (notebooks, notes, tags, contents) = dataset();
    srv.set_notebooks(notebooks);
    srv.set_notes(notes);
    srv.set_tags(tags);
    srv.set_contents(contents);
    srv.set_sync_usn(8);

    let t = srv.token();
    let base = srv.url();

    // getSyncState
    let state = get(&format!("{base}/api/user/getSyncState?token={t}")).await;
    assert_eq!(state["LastSyncUsn"], json!(8));

    // notebooks, paged 2 at a time: 2 + 1 (last chunk len < maxEntry)
    let p1: Vec<Value> = serde_json::from_value(
        get(&format!(
            "{base}/api/notebook/getSyncNotebooks?token={t}&afterUsn=0&maxEntry=2"
        ))
        .await,
    )
    .unwrap();
    assert_eq!(p1.len(), 2);
    assert_eq!(p1[0]["NotebookId"], json!("nb1"));
    assert_eq!(p1[1]["Usn"], json!(2));
    let p2: Vec<Value> = serde_json::from_value(
        get(&format!(
            "{base}/api/notebook/getSyncNotebooks?token={t}&afterUsn=2&maxEntry=2"
        ))
        .await,
    )
    .unwrap();
    assert_eq!(p2.len(), 1);
    assert_eq!(p2[0]["NotebookId"], json!("nb3"));
    assert_eq!(p2[0]["IsDeleted"], json!(true));

    // notes + contents (content fetched separately, like sync.js)
    let sync_notes: Vec<Value> = serde_json::from_value(
        get(&format!(
            "{base}/api/note/getSyncNotes?token={t}&afterUsn=0&maxEntry=30"
        ))
        .await,
    )
    .unwrap();
    assert_eq!(sync_notes.len(), 3);
    let c1 = get(&format!(
        "{base}/api/note/getNoteContent?token={t}&noteId=n1"
    ))
    .await;
    assert_eq!(c1["Ok"], json!(true));
    assert_eq!(c1["NoteId"], json!("n1"));
    assert_eq!(c1["Content"], json!("# First\n\nbody"));
    let missing = get(&format!(
        "{base}/api/note/getNoteContent?token={t}&noteId=nope"
    ))
    .await;
    assert_eq!(missing["Ok"], json!(false));

    // tags
    let sync_tags: Vec<Value> = serde_json::from_value(
        get(&format!(
            "{base}/api/tag/getSyncTags?token={t}&afterUsn=0&maxEntry=30"
        ))
        .await,
    )
    .unwrap();
    assert_eq!(sync_tags.len(), 2);
    assert_eq!(sync_tags[0]["Tag"], json!("ai"));
    assert_eq!(sync_tags[1]["IsDeleted"], json!(true));
}

#[tokio::test]
async fn pushes_are_recorded() {
    let srv = start().await;
    post(
        &format!("{}/api/auth/login?token=", srv.url()),
        &format!("email={EMAIL}&pwd={PWD}"),
    )
    .await;
    let t = srv.token();
    let base = srv.url();
    assert!(srv.pushes().is_empty());

    // addNotebook: needle posts form fields (camelCase from api.js)
    let add = post(
        &format!("{base}/api/notebook/addNotebook?token={t}"),
        "title=AI%E6%9C%AD&seq=1&parentNotebookId=",
    )
    .await;
    assert_eq!(add["Ok"], json!(true));
    assert_eq!(add["NotebookId"], json!("srvnb1"));
    assert_eq!(add["Usn"], json!(1));

    // addNote: PascalCase body fields
    let note = post(
        &format!("{base}/api/note/addNote?token={t}"),
        "Title=Hello&NotebookId=srvnb9&Content=%23+Hello&IsMarkdown=true&Tags=ai&Tags=cli",
    )
    .await;
    assert_eq!(note["Ok"], json!(true));
    assert_eq!(note["NoteId"], json!("srvnote2"));

    // updateNote / deleteTrash / tags
    post(
        &format!("{base}/api/note/updateNote?token={t}"),
        "NoteId=srvnote2&Usn=2&Title=Hello2&Tags=x",
    )
    .await;
    post(
        &format!("{base}/api/note/deleteTrash?token={t}"),
        "noteId=srvnote2&usn=3",
    )
    .await;
    post(&format!("{base}/api/tag/addTag?token={t}"), "tag=rust").await;
    post(
        &format!("{base}/api/tag/deleteTag?token={t}"),
        "tag=rust&usn=4",
    )
    .await;
    post(
        &format!("{base}/api/notebook/updateNotebook?token={t}"),
        "notebookId=srvnb1&title=Renamed&usn=1&seq=2&parentNotebookId=",
    )
    .await;

    let pushes = srv.pushes();
    let eps: Vec<&str> = pushes.iter().map(|p| p.endpoint.as_str()).collect();
    assert_eq!(
        eps,
        vec![
            "notebook/addNotebook",
            "note/addNote",
            "note/updateNote",
            "note/deleteTrash",
            "tag/addTag",
            "tag/deleteTag",
            "notebook/updateNotebook",
        ]
    );
    // field-level assertions on the recorded addNote
    let add_note = &pushes[1];
    assert_eq!(add_note.fields["Title"], json!("Hello"));
    assert_eq!(add_note.fields["Content"], json!("# Hello"));
    // repeated Tags keys collect into an array
    assert_eq!(add_note.fields["Tags"], json!(["ai", "cli"]));
    // deleteTrash body uses lowercase noteId/usn (api.js line 639)
    assert_eq!(pushes[3].fields["noteId"], json!("srvnote2"));
    assert_eq!(pushes[3].fields["usn"], json!("3"));
}

#[tokio::test]
async fn injected_conflict_response() {
    let srv = start().await;
    post(
        &format!("{}/api/auth/login?token=", srv.url()),
        &format!("email={EMAIL}&pwd={PWD}"),
    )
    .await;
    let t = srv.token();
    srv.inject_response(json!({"Ok": false, "Msg": "conflict"}));

    // next updateNote gets the injected body, request still recorded
    let ret = post(
        &format!("{}/api/note/updateNote?token={}", srv.url(), t),
        "NoteId=srvnote10&Usn=9&Title=X",
    )
    .await;
    assert_eq!(ret["Ok"], json!(false));
    assert_eq!(ret["Msg"], json!("conflict"));
    assert_eq!(srv.pushes().len(), 1);

    // subsequent push succeeds again
    let ok = post(
        &format!("{}/api/note/updateNote?token={}", srv.url(), t),
        "NoteId=srvnote10&Usn=9&Title=Y",
    )
    .await;
    assert_eq!(ok["Ok"], json!(true));
    assert_eq!(srv.pushes().len(), 2);
}

//! One-shot end-to-end smoke against the in-process fake server:
//! login → full sync → create nested notebooks offline → incr sync →
//! verify the server received the mapped parent id, parent-first.
//! Run: cargo test -p leanote-cli --test e2e_smoke -- --nocapture

use leanote_core::sync::{full_sync, incr_sync, Api, NoProgress};
use leanote_core::Store;
use leanote_fakeserver::start;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test(flavor = "multi_thread")]
async fn e2e_nested_notebooks_survive_push() {
    let srv = start().await;
    srv.set_notebooks(vec![json!({
        "NotebookId": "nb0", "Title": "Root", "Seq": 1, "ParentNotebookId": "",
        "Usn": 1, "IsDeleted": false, "IsDirty": false,
        "CreatedTime": "2026-01-01T00:00:00Z", "UpdatedTime": "2026-01-01T00:00:00Z",
    })]);
    srv.set_notes(vec![]);
    srv.set_tags(vec![]);
    srv.set_contents(Default::default());
    srv.set_sync_usn(1);

    let dir = std::env::temp_dir().join(format!(
        "leanote-e2e-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let s = Store::open(dir.clone(), leanote_fakeserver::USER_ID.to_string()).unwrap();
    let acct = Api::login(
        &srv.url(),
        leanote_fakeserver::EMAIL,
        leanote_fakeserver::PWD,
        &s,
    )
    .unwrap();
    let api = Api::for_account(&acct);
    full_sync(&api, &s, &NoProgress).unwrap();
    println!("full sync ok, last usn = {:?}", s.last_sync_usn());

    // Offline: drag-drop equivalent — create parent then child under it.
    let parent = s.create_notebook("Parent", "").unwrap();
    let child = s.create_notebook("Child", &parent.notebook_id).unwrap();
    println!(
        "local parent id = {}, child parent link = {}",
        parent.notebook_id, child.parent_notebook_id
    );

    srv.clear_pushes();
    incr_sync(&api, &s, &NoProgress).unwrap();

    for p in srv.pushes() {
        println!("push {} {:?}", p.endpoint, p.fields);
    }
    let pushes = srv.pushes();
    let parent_push = pushes
        .iter()
        .position(|p| p.endpoint == "notebook/addNotebook" && p.fields["title"] == json!("Parent"))
        .expect("parent addNotebook");
    let child_push = pushes
        .iter()
        .position(|p| p.endpoint == "notebook/addNotebook" && p.fields["title"] == json!("Child"))
        .expect("child addNotebook");
    assert!(parent_push < child_push, "parent must be pushed first");
    let child_parent = pushes[child_push].fields["parentNotebookId"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert!(
        !child_parent.is_empty(),
        "child's parentNotebookId must be the mapped server id, got {child_parent:?}"
    );
    println!("OK: child -> server parent {child_parent}, parent-first order held");
    let _ = std::fs::remove_dir_all(&dir);
}

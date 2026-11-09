// Ticket 05 acceptance: /api/ping, addNote/updateNote success+error
// responses byte-compared against fixtures derived from src/local_server.js
// (JS sendJSON = compact JSON.stringify, Content-Type application/json).
// Also: CORS `*`, 5MB body rejection, 127.0.0.1-only bind.

use axum::http::StatusCode;
use leanote_core::Store;
use leanote_http::serve;
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("leanote-http-test-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

async fn spawn(name: &str) -> (u16, Store) {
    let dir = tmpdir(name); // single dir — tmpdir() wipes on every call
    let store = Store::open(dir, "user1".to_string()).expect("open store");
    let server_store = Store::open(
        std::env::temp_dir().join(format!("leanote-http-test-{}-{}", name, std::process::id())),
        "user1".to_string(),
    )
    .expect("open server store");
    let port = serve(server_store, 0).await.expect("serve");
    (port, store)
}

fn url(port: u16, path: &str) -> String {
    format!("http://127.0.0.1:{}{}", port, path)
}

#[tokio::test]
async fn ping_status_body_and_headers() {
    let (port, _store) = spawn("ping").await;
    let resp = reqwest::Client::new().get(url(port, "/api/ping")).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["access-control-allow-origin"], "*");
    assert_eq!(resp.headers()["content-type"], "application/json");
    assert_eq!(resp.text().await.unwrap(), "{\"success\":true}");
}

#[tokio::test]
async fn unknown_endpoint_404() {
    let (port, _store) = spawn("404").await;
    let resp = reqwest::Client::new().get(url(port, "/api/nope")).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(resp.text().await.unwrap(), "{\"success\":false,\"error\":\"Unknown endpoint\"}");

    // wrong method on a known path → 404 too
    let resp = reqwest::Client::new()
        .get(url(port, "/api/note/addNote"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn options_preflight_200_empty() {
    let (port, _store) = spawn("options").await;
    let resp = reqwest::Client::new()
        .request(reqwest::Method::OPTIONS, url(port, "/anything"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["access-control-allow-origin"], "*");
    assert_eq!(
        resp.headers()["access-control-allow-methods"],
        "GET, POST, DELETE, OPTIONS"
    );
    assert_eq!(resp.text().await.unwrap(), "");
}

#[tokio::test]
async fn add_note_success_creates_aizha_and_returns_id() {
    let (port, store) = spawn("addok").await;
    let resp = reqwest::Client::new()
        .post(url(port, "/api/note/addNote"))
        .json(&serde_json::json!({
            "title": "T1",
            "content": "# Hello\nWorld",
            "tags": ["ai", "rust"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["success"], serde_json::json!(true));
    let id = body["noteId"].as_str().unwrap();
    assert_eq!(id.len(), 24);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));

    let note = store.get_note(&leanote_core::Id(id.to_string())).unwrap().unwrap();
    assert_eq!(note.title, "T1");
    assert_eq!(note.content, "# Hello\nWorld");
    assert!(note.is_markdown, "IsMarkdown must be written");
    assert!(note.is_new && note.is_dirty);
    assert_eq!(note.tags, vec!["ai".to_string(), "rust".to_string()]);
    assert_eq!(note.desc, "Hello World");
    // AI札 auto-created exactly once; tags upserted
    assert_eq!(store.notebooks().unwrap()[0].title, "AI札");
    let tags: Vec<String> = store.tags().unwrap().iter().map(|t| t.tag.clone()).collect();
    assert!(tags.contains(&"ai".to_string()) && tags.contains(&"rust".to_string()));
}

#[tokio::test]
async fn add_note_validation_errors() {
    let (port, _store) = spawn("adderr").await;
    let c = reqwest::Client::new();

    let r = c.post(url(port, "/api/note/addNote")).json(&serde_json::json!({"content": "c"})).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert_eq!(r.text().await.unwrap(), "{\"success\":false,\"error\":\"title is required\"}");

    let r = c.post(url(port, "/api/note/addNote")).json(&serde_json::json!({"title": "t"})).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert_eq!(r.text().await.unwrap(), "{\"success\":false,\"error\":\"content is required\"}");

    let r = c.post(url(port, "/api/note/addNote")).json(&serde_json::json!({"title": "", "content": "c"})).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert_eq!(r.text().await.unwrap(), "{\"success\":false,\"error\":\"title is required\"}");

    // invalid JSON → 400 with JSON-parse message, like the JS try/catch
    let r = c
        .post(url(port, "/api/note/addNote"))
        .header("content-type", "application/json")
        .body("{not json")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let b: serde_json::Value = r.json().await.unwrap();
    assert_eq!(b["success"], serde_json::json!(false));
    assert!(!b["error"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn update_note_success_and_prefix_match() {
    let (port, store) = spawn("updok").await;
    let c = reqwest::Client::new();
    let created: serde_json::Value = c
        .post(url(port, "/api/note/addNote"))
        .json(&serde_json::json!({"title": "Old", "content": "old body", "tags": []}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let full_id = created["noteId"].as_str().unwrap().to_string();

    // full id
    let r = c.post(url(port, "/api/note/updateNote"))
        .json(&serde_json::json!({"noteId": full_id, "title": "New", "content": "new body"}))
        .send().await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let b: serde_json::Value = r.json().await.unwrap();
    assert_eq!(b["success"], serde_json::json!(true));
    assert_eq!(b["noteId"], serde_json::json!(full_id));

    let n = store.get_note(&leanote_core::Id(full_id.clone())).unwrap().unwrap();
    assert_eq!(n.title, "New");
    assert_eq!(n.content, "new body");

    // unique prefix
    let prefix: String = full_id.chars().take(6).collect();
    let r = c.post(url(port, "/api/note/updateNote"))
        .json(&serde_json::json!({"noteId": prefix, "title": "ViaPrefix"}))
        .send().await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let n = store.get_note(&leanote_core::Id(full_id.clone())).unwrap().unwrap();
    assert_eq!(n.title, "ViaPrefix");
    // content untouched by title-only update
    assert_eq!(n.content, "new body");

    // tags update + upsert
    let r = c.post(url(port, "/api/note/updateNote"))
        .json(&serde_json::json!({"noteId": full_id.clone(), "tags": ["新标签"]}))
        .send().await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let tags: Vec<String> = store.tags().unwrap().iter().map(|t| t.tag.clone()).collect();
    assert!(tags.contains(&"新标签".to_string()));
}

#[tokio::test]
async fn update_note_errors() {
    let (port, _store) = spawn("upderr").await;
    let c = reqwest::Client::new();

    let r = c.post(url(port, "/api/note/updateNote")).json(&serde_json::json!({"title": "x"})).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert_eq!(r.text().await.unwrap(), "{\"success\":false,\"error\":\"noteId is required\"}");

    let r = c.post(url(port, "/api/note/updateNote")).json(&serde_json::json!({"noteId": "deadbeef"})).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
    assert_eq!(r.text().await.unwrap(), "{\"success\":false,\"error\":\"Note not found\"}");
}

#[tokio::test]
async fn body_over_5mb_rejected_like_js() {
    let (port, _store) = spawn("big").await;
    let big = vec![b'a'; 5 * 1024 * 1024 + 1];
    let resp = reqwest::Client::new()
        .post(url(port, "/api/note/addNote"))
        .header("content-type", "application/json")
        .body(big)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(resp.headers()["content-type"], "application/json");
    assert_eq!(resp.text().await.unwrap(), "{\"error\":\"Request body too large\"}");
}

#[tokio::test]
async fn binds_loopback_only() {
    let (port, _store) = spawn("loopback").await;
    // sanity: loopback reachable (already used above), and the listener
    // must not accept on the machine's non-loopback address: connecting to
    // 127.0.0.1 works; asserting the bind addr directly is the stronger
    // check — leanote_http::HOST is compile-time "127.0.0.1".
    assert_eq!(leanote_http::HOST, "127.0.0.1");
    let resp = reqwest::Client::new().get(url(port, "/api/ping")).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn add_note_custom_notebook_and_defaults() {
    let (port, store) = spawn("nb").await;
    let c = reqwest::Client::new();
    // explicit notebook string → that notebook (created, since it's new)
    let r = c.post(url(port, "/api/note/addNote"))
        .json(&serde_json::json!({"title": "N", "content": "c", "notebook": "工作", "tags": []}))
        .send().await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let titles: Vec<String> = store.notebooks().unwrap().iter().map(|n| n.title.clone()).collect();
    assert!(titles.contains(&"工作".to_string()));
    assert!(!titles.contains(&"AI札".to_string()), "AI札 only when unspecified");

    // no tags field at all → no tag rows
    let r = c.post(url(port, "/api/note/addNote"))
        .json(&serde_json::json!({"title": "M", "content": "c"}))
        .send().await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert!(store.tags().unwrap().is_empty());
}

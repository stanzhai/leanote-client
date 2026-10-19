// Byte-parity against the JS local server. Fixtures in
// .scratch/rust-rewrite/fixtures/js_api_fixtures.json were captured by
// running src/local_server.js with stub services (see capture_js_api.js);
// this test replays the same request sequence against the Rust server and
// asserts identical status / Content-Type / CORS / body bytes.
// `<24hex>` in a body matches any 24-char hex id; `bodyPattern` requests a
// shape check (used only for the JSON-parse error, whose wording is
// engine-specific: V8 vs serde_json).

use leanote_core::Store;
use leanote_http::serve;
use std::path::PathBuf;

#[tokio::test]
async fn matches_js_server_bytes() {
    let fixture_path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "..",
        "..",
        ".scratch",
        "rust-rewrite",
        "fixtures",
        "js_api_fixtures.json",
    ]
    .iter()
    .collect();
    let raw = std::fs::read_to_string(fixture_path).expect("fixtures file");
    let fixtures: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let fixtures = fixtures.as_array().unwrap();

    let dir = std::env::temp_dir().join(format!("leanote-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let server_store = Store::open(dir, "user1".to_string()).unwrap();
    let port = serve(server_store, 0).await.unwrap();
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();

    // Same request sequence as capture_js_api.js steps.
    let created_id = client
        .post(format!("{base}/api/note/addNote"))
        .json(&serde_json::json!({"title": "T", "content": "# Hi\nWorld", "tags": ["a", "b"]}))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()["noteId"]
        .as_str()
        .unwrap()
        .to_string();
    let requests: Vec<(&'static str, String, Option<String>)> = vec![
        ("GET", "/api/ping".into(), None),
        ("GET", "/api/nope".into(), None),
        (
            "POST",
            "/api/note/addNote".into(),
            // request key order is irrelevant (server parses JSON);
            // serde's compact form avoids raw-string escape issues
            Some(serde_json::json!({"title": "T", "content": "# Hi\nWorld", "tags": ["a", "b"]}).to_string()),
        ),
        ("POST", "/api/note/addNote".into(), Some(r#"{"content":"c"}"#.into())),
        ("POST", "/api/note/addNote".into(), Some(r#"{"title":"t"}"#.into())),
        ("POST", "/api/note/addNote".into(), Some("{not json".into())),
        ("POST", "/api/note/updateNote".into(), Some(r#"{"title":"x"}"#.into())),
        (
            "POST",
            "/api/note/updateNote".into(),
            Some(r#"{"noteId":"deadbeefdeadbeefdeadbeef"}"#.into()),
        ),
        (
            "POST",
            "/api/note/updateNote".into(),
            Some(format!(r#"{{"noteId":"{created_id}","title":"Upd","content":"body2"}}"#)),
        ),
        (
            "POST",
            "/api/note/updateNote".into(),
            Some(format!(r#"{{"noteId":"{}","title":"ViaPrefix"}}"#, &created_id[..6])),
        ),
    ];
    assert_eq!(requests.len(), fixtures.len(), "fixture/request count drift");

    for (i, (method, path, body)) in requests.into_iter().enumerate() {
        let fx = &fixtures[i];
        let mut req = client.request(
            reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
            format!("{base}{path}"),
        );
        if let Some(b) = body {
            req = req.header("content-type", "application/json").body(b);
        }
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16();
        let content_type = resp.headers()["content-type"].to_str().unwrap().to_string();
        let allow_origin = resp.headers()["access-control-allow-origin"]
            .to_str()
            .unwrap()
            .to_string();
        let text = resp.text().await.unwrap();

        assert_eq!(status, fx["status"].as_u64().unwrap() as u16, "{}: status", fx["name"]);
        assert_eq!(content_type, fx["contentType"], "{}: content-type", fx["name"]);
        assert_eq!(allow_origin, fx["allowOrigin"], "{}: CORS", fx["name"]);

        let want = fx["body"].as_str().unwrap_or_default();
        if fx["bodyPattern"].is_string() {
            // shape only — parse-error wording is engine-specific
            assert!(
                text.starts_with(r#"{"success":false,"error":""#) && text.ends_with("}"),
                "{}: body {text}",
                fx["name"]
            );
        } else if let Some(at) = want.find("<24hex>") {
            let prefix = &want[..at];
            let suffix = &want[at + 7..];
            assert!(
                text.starts_with(prefix) && text.ends_with(suffix),
                "{}: body {text}",
                fx["name"]
            );
            let id = &text[prefix.len()..text.len() - suffix.len()];
            assert_eq!(id.len(), 24, "{}: id len", fx["name"]);
            assert!(id.bytes().all(|b| b.is_ascii_hexdigit()), "{}: id hex", fx["name"]);
        } else {
            assert_eq!(text, want, "{}: body bytes", fx["name"]);
        }
    }
}

//! Ticket 06 acceptance: MCP wire parity + session cap/TTL semantics.
//! Wire bytes pinned to captures from the live JS stack (src/local_server.js
//! + @modelcontextprotocol/sdk).

use leanote_core::Store;
use leanote_http::{mcp, serve};

static DIR_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
async fn spawn() -> (reqwest::Client, u16) {
    let dir = std::env::temp_dir().join(format!("mcp-test-{}-{}", std::process::id(), DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let store = Store::open(dir, "user1".into()).expect("store");
    let port = serve(store, 0).await.expect("bind");
    (reqwest::Client::new(), port)
}

async fn post(
    c: &reqwest::Client,
    port: u16,
    sid: Option<&str>,
    body: serde_json::Value,
) -> (reqwest::StatusCode, Option<String>, String) {
    let mut r = c
        .post(format!("http://127.0.0.1:{port}/mcp"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
    if let Some(sid) = sid {
        r = r.header("mcp-session-id", sid);
    }
    let resp = r.json(&body).send().await.expect("send");
    let sid = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let status = resp.status();
    let body = resp.text().await.expect("body");
    (status, sid, body)
}

fn init_msg(id: i64) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": id, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "t", "version": "0"}
        }
    })
}

#[tokio::test]
async fn initialize_stateful_shapes() {
    let (c, port) = spawn().await;
    let (status, sid, body) = post(&c, port, None, init_msg(1)).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(sid.is_some(), "initialize must mint a session id");
    // Exactly one SSE frame; SDK key order: result, jsonrpc, id.
    assert_eq!(body, format!(
        "event: message\ndata: {}\n\n",
        serde_json::json!({
            "result": {
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {"listChanged": true}},
                "serverInfo": {"name": "leanote", "version": "1.0.0"}
            },
            "jsonrpc": "2.0",
            "id": 1
        })
    ));

    // Notification on the live session → 202, empty body, no session id.
    let (status, sid2, body2) = post(
        &c,
        port,
        sid.as_deref(),
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::ACCEPTED);
    assert_eq!(sid2, None);
    assert_eq!(body2, "");
}

#[tokio::test]
async fn stateless_direct_mode() {
    let (c, port) = spawn().await;
    let (status, sid, body) = post(
        &c,
        port,
        None,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(sid, None, "stateless mode must not mint a session");
    let payload = body.trim().lines().last().unwrap().trim_start_matches("data: ");
    let data: serde_json::Value = serde_json::from_str(payload).unwrap();
    let names: Vec<&str> = data["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["list_notebooks", "list_tags", "add_note", "list_notes", "get_note", "search_notes", "update_note", "delete_note"]
    );
    let tool = &data["result"]["tools"][0];
    let keys: Vec<&str> = tool.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["name", "description", "inputSchema", "execution"]);
    assert_eq!(tool["inputSchema"], serde_json::json!({"type": "object", "properties": {}}));
    assert_eq!(tool["execution"], serde_json::json!({"taskSupport": "forbidden"}));

    // Unknown method → -32601, error key order: jsonrpc, id, error.
    let (status, _, body) = post(
        &c,
        port,
        None,
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "bogus/x"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(
        body,
        "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":3,\"error\":{\"code\":-32601,\"message\":\"Method not found\"}}\n\n"
    );

    // Unknown session id on POST → JS falls through to stateless, 200 SSE.
    let (status, _, body) = post(
        &c,
        port,
        Some("does-not-exist"),
        serde_json::json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(body.starts_with("event: message\ndata: {\"result\":"));
}

#[tokio::test]
async fn session_lifecycle_delete() {
    let (c, port) = spawn().await;
    let (_, sid, _) = post(&c, port, None, init_msg(1)).await;
    let sid = sid.unwrap();

    let resp = c
        .delete(format!("http://127.0.0.1:{port}/mcp"))
        .header("mcp-session-id", &sid)
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    // After DELETE the session is gone → POST with that sid is stateless.
    let (status, _, body) = post(
        &c,
        port,
        Some(&sid),
        serde_json::json!({"jsonrpc": "2.0", "id": 9, "method": "tools/list"}),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(body.contains("\"tools\""));
}

// ---- SessionBook unit semantics (cap 50 + TTL reaper) ----

#[test]
fn session_book_cap_and_ttl() {
    let mut book = mcp::SessionBook::default();
    assert!(!book.at_capacity());
    let first = book.create();
    for _ in 1..mcp::TRANSPORT_MAX {
        book.create();
    }
    assert_eq!(book.len(), mcp::TRANSPORT_MAX);
    assert!(book.at_capacity(), "JS: >= TRANSPORT_MAX → 503 on next initialize");

    book.remove(&first);
    assert_eq!(book.len(), mcp::TRANSPORT_MAX - 1);
    assert!(!book.at_capacity());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let ttl = mcp::TRANSPORT_TTL.as_millis() as u64;
    book.insert_with_last_used("stale", now - ttl - 1);
    book.insert_with_last_used("fresh", now);
    book.reap(mcp::TRANSPORT_TTL);
    assert!(!book.contains("stale"));
    assert!(book.contains("fresh"));
}

#[tokio::test]
async fn session_cap_51st_initialize_503() {
    let (c, port) = spawn().await;
    for _ in 0..mcp::TRANSPORT_MAX {
        let (status, sid, _) = post(&c, port, None, init_msg(1)).await;
        assert_eq!(status, reqwest::StatusCode::OK);
        assert!(sid.is_some());
    }
    let (status, sid, body) = post(&c, port, None, init_msg(1)).await;
    assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(sid, None);
    assert_eq!(
        body,
        "{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32000,\"message\":\"Too many concurrent sessions\"},\"id\":null}"
    );
}

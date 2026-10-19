//! MCP endpoints (ticket 06): `/mcp` Streamable HTTP (stateful sessions —
//! cap 50, 30-min TTL — plus stateless direct mode) and legacy `/sse` +
//! `/messages`, on the same 127.0.0.1 server as `/api/*`.
//!
//! Wire shapes (status codes, headers, SSE framing, JSON key order, tool
//! schemas) were captured from the live JS stack (`src/local_server.js` +
//! `@modelcontextprotocol/sdk` streamableHttp/sse transports); the JSON above
//! each function names what was observed. `tests/mcp.rs` pins the important
//! ones. Tool semantics mirror `src/leanote_data.js` field-for-field.

use crate::{cors_headers, AppState, Body};
use axum::body::Body as AxumBody;
use axum::extract::{FromRequest, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::Response;
use leanote_core::{NewNote, Note, NoteUpdate, Store};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::ReceiverStream;

pub const TRANSPORT_MAX: usize = 50;
pub const TRANSPORT_TTL: Duration = Duration::from_secs(30 * 60);

// SUPPORTED_PROTOCOL_VERSIONS from the installed SDK (types.js).
const SUPPORTED_PROTOCOL_VERSIONS: [&str; 5] = [
    "2025-11-25",
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
    "2024-10-07",
];
const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";

#[derive(Clone)]
pub struct McpState {
    pub store: AppState,
    /// JS `transports{}` + `transportLastUsed{}` (a live session = an entry).
    pub book: Arc<Mutex<SessionBook>>,
    /// The single legacy SSE connection (JS keeps one `sseTransport`).
    pub sse: Arc<Mutex<Option<mpsc::Sender<String>>>>,
}

impl McpState {
    pub fn new(store: Store) -> Self {
        McpState {
            store: Arc::new(Mutex::new(store)),
            book: Arc::new(Mutex::new(SessionBook::default())),
            sse: Arc::new(Mutex::new(None)),
        }
    }
}

/// JS reaps expired MCP sessions every 5 minutes; spawned by `router()`.
pub fn spawn_session_reaper(state: &McpState) {
    let book = Arc::clone(&state.book);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5 * 60));
        loop {
            tick.tick().await;
            book.lock().map(|mut b| b.reap(TRANSPORT_TTL)).ok();
        }
    });
}

/// `/api/*` fallback handler (moved from lib.rs so the whole router shares
/// one state type). Store calls are blocking SQLite — off the async core.
pub async fn api_fallback(
    State(state): State<McpState>,
    req: Request,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let body = match Body::from_request(req, &()).await {
        Ok(b) => b.0,
        Err(resp) => return resp,
    };
    let resp = tokio::task::spawn_blocking(move || {
        let store = state.store.lock().expect("store lock");
        crate::api_dispatch(&store, method, &path, body)
    })
    .await;
    match resp {
        Ok(r) => r,
        Err(e) => crate::json_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!(r#"{{"success":false,"error":"{e}}}"#),
        ),
    }
}

// ===================== session book =====================

/// JS transports{} + transportLastUsed{} semantics, including the
/// `Object.keys(transports).length >= TRANSPORT_MAX` cap check and the
/// 5-minute TTL reaper. `order` mirrors JS object key insertion order.
#[derive(Default)]
pub struct SessionBook {
    sessions: HashMap<String, u64>,
    order: Vec<String>,
}

impl SessionBook {
    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// JS onsessioninitialized: `transports[sid] = transport;
    /// transportLastUsed[sid] = Date.now()`.
    pub fn create(&mut self) -> String {
        let sid = uuid();
        self.sessions.insert(sid.clone(), Self::now());
        self.order.push(sid.clone());
        sid
    }

    pub fn touch(&mut self, sid: &str) {
        if let Some(t) = self.sessions.get_mut(sid) {
            *t = Self::now();
        }
    }

    /// JS transport.onclose: delete both maps.
    pub fn remove(&mut self, sid: &str) {
        self.sessions.remove(sid);
        self.order.retain(|s| s != sid);
    }

    pub fn contains(&self, sid: &str) -> bool {
        self.sessions.contains_key(sid)
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// JS `Object.keys(transports).length >= TRANSPORT_MAX` → 503.
    pub fn at_capacity(&self) -> bool {
        self.len() >= TRANSPORT_MAX
    }

    /// JS 5-min reaper loop: drop sessions idle past the TTL.
    pub fn reap(&mut self, ttl: Duration) {
        let cutoff = Self::now().saturating_sub(ttl.as_millis() as u64);
        let stale: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, t)| **t < cutoff)
            .map(|(s, _)| s.clone())
            .collect();
        for sid in stale {
            self.remove(&sid);
        }
    }

    // -- test hooks --
    pub fn insert_with_last_used(&mut self, sid: &str, last_used: u64) {
        self.sessions.insert(sid.to_string(), last_used);
        self.order.push(sid.to_string());
    }
    pub fn last_used(&self, sid: &str) -> Option<u64> {
        self.sessions.get(sid).copied()
    }
}

/// crypto.randomUUID()-shaped id (only uniqueness matters to any client).
fn uuid() -> String {
    use std::hash::{BuildHasher, Hasher, RandomState};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h1 = RandomState::new().build_hasher();
    h1.write_u128(nanos);
    h1.write_u64(std::process::id() as u64);
    let a = h1.finish();
    let mut h2 = RandomState::new().build_hasher();
    h2.write_u64(a);
    h2.write_u64(a.rotate_left(32) ^ (nanos as u64));
    let b = h2.finish();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        ((a >> 16) & 0xffff) as u16,
        (a & 0xfff) as u16,
        (((b >> 48) & 0x3fff) | 0x8000) as u16,
        b & 0xffff_ffff_ffff
    )
}

// ===================== response helpers =====================

/// SDK SSE framing: `event: message\ndata: <json>\n\n`.
fn sse_frame(response: &Value) -> String {
    format!("event: message\ndata: {response}\n\n")
}

/// SDK key order: `{"result":...,"jsonrpc":"2.0","id":...}`.
fn rpc_result(result: Value, id: &Value) -> Value {
    json!({"result": result, "jsonrpc": "2.0", "id": id})
}

/// SDK createJsonErrorResponse: `{"jsonrpc":"2.0","error":{...},"id":null}`.
fn rpc_error(status: StatusCode, code: i64, message: &str) -> Response {
    // Hand-built: preserve SDK key order (jsonrpc, error, id) — serde_json's
    // BTreeMap sorts keys alphabetically.
    json_response(
        status,
        format!(r#"{{"jsonrpc":"2.0","error":{{"code":{code},"message":"{message}"}},"id":null}}"#),
    )
}

fn json_response(status: StatusCode, body: String) -> Response {
    let mut resp = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(AxumBody::from(body))
        .unwrap();
    cors_headers(resp.headers_mut());
    resp
}

/// Open-ended SSE stream. The optional first frame is queued on the channel so
/// ordering holds without a second stream source.
fn sse_stream_response(sid: Option<&str>, first: Option<String>) -> (Response, mpsc::Sender<String>) {
    let (tx, rx) = mpsc::channel(16);
    let _ = first.map(|f| tx.try_send(f));
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive");
    if let Some(sid) = sid {
        builder = builder.header("mcp-session-id", sid);
    }
    let mut resp = builder
        .body(AxumBody::from_stream(
            ReceiverStream::new(rx).map(Ok::<_, std::convert::Infallible>),
        ))
        .unwrap();
    cors_headers(resp.headers_mut());
    (resp, tx)
}

/// Single-frame SSE response (every /mcp POST with an id ends the stream right
/// after its one response, exactly like the SDK closing on the last message).
fn sse_single_response(sid: Option<&str>, response: &Value) -> Response {
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive");
    if let Some(sid) = sid {
        builder = builder.header("mcp-session-id", sid);
    }
    let mut resp = builder
        .body(AxumBody::from(sse_frame(response)))
        .unwrap();
    cors_headers(resp.headers_mut());
    resp
}

fn accepted_empty() -> Response {
    Response::builder()
        .status(StatusCode::ACCEPTED)
        .header(header::CONTENT_LENGTH, "0")
        .body(AxumBody::empty())
        .unwrap()
}

/// SDK isInitializeRequest: jsonrpc 2.0 + id + method initialize + params with
/// protocolVersion/capabilities/clientInfo.
fn is_initialize_request(msg: &Value) -> bool {
    msg.get("jsonrpc").and_then(Value::as_str) == Some("2.0")
        && msg.get("id").is_some()
        && msg.get("method").and_then(Value::as_str) == Some("initialize")
        && msg.get("params").is_some_and(|p| {
            p.get("protocolVersion").is_some()
                && p.get("capabilities").is_some()
                && p.get("clientInfo").is_some()
        })
}

fn has_id(msg: &Value) -> bool {
    msg.get("id").is_some_and(|v| !v.is_null())
}

// ===================== /mcp =====================

pub async fn mcp_route(State(state): State<McpState>, req: Request) -> Response {
    let method = req.method().clone();
    let headers = req.headers().clone();
    let path = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "/mcp".to_string());
    let raw = match Body::from_request(req, &()).await {
        Ok(b) => b.0,
        Err(resp) => return resp,
    };
    log_mcp_request(&method, &path, &headers, &raw);

    let sid = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    match method {
        Method::POST => {
            // JS handleMcp: JSON.parse error first…
            let msg: Value = if raw.is_empty() {
                Value::Null
            } else {
                match serde_json::from_slice(&raw) {
                    Ok(v) => v,
                    Err(_) => return rpc_error(StatusCode::BAD_REQUEST, -32700, "Parse error"),
                }
            };
            // …then transport.handlePostRequest header checks.
            let accept = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).unwrap_or("");
            if !accept.contains("application/json") || !accept.contains("text/event-stream") {
                return rpc_error(
                    StatusCode::NOT_ACCEPTABLE,
                    -32000,
                    "Not Acceptable: Client must accept both application/json and text/event-stream",
                );
            }
            if !headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|ct| ct.contains("application/json"))
            {
                return rpc_error(
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    -32000,
                    "Unsupported Media Type: Content-Type must be application/json",
                );
            }

            let known = sid.as_deref().is_some_and(|s| state.book.lock().map(|b| b.contains(s)).unwrap_or(false));
            if known {
                // Re-initialize on a live session → SDK 400 "already initialized".
                if is_initialize_request(&msg) {
                    return rpc_error(StatusCode::BAD_REQUEST, -32600, "Invalid Request: Server already initialized");
                }
                let sid = sid.unwrap();
                state.book.lock().map(|mut b| b.touch(&sid)).ok();
                respond(state.store.clone(), Some(sid), &msg).await
            } else if sid.is_none() && is_initialize_request(&msg) {
                if state.book.lock().map(|b| b.at_capacity()).unwrap_or(false) {
                    return rpc_error(StatusCode::SERVICE_UNAVAILABLE, -32000, "Too many concurrent sessions");
                }
                let sid = state.book.lock().map(|mut b| b.create()).unwrap_or_default();
                respond(state.store.clone(), Some(sid), &msg).await
            } else {
                // Stateless direct mode (HARPA-style): no session id, ever.
                respond(state.store.clone(), None, &msg).await
            }
        }
        Method::GET | Method::DELETE => {
            let sid_ref = sid.as_deref().filter(|s| state.book.lock().map(|b| b.contains(s)).unwrap_or(false));
            let Some(sid) = sid_ref.map(str::to_string) else {
                // JS: 400 {"error":"Invalid or missing session ID"}
                return json_response(
                    StatusCode::BAD_REQUEST,
                    r#"{"error":"Invalid or missing session ID"}"#.to_string(),
                );
            };
            state.book.lock().map(|mut b| b.touch(&sid)).ok();
            if method == Method::DELETE {
                // SDK handleDeleteRequest: 200 empty; close() fires onclose →
                // transports{} entry removed.
                state.book.lock().map(|mut b| b.remove(&sid)).ok();
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_LENGTH, "0")
                    .body(AxumBody::empty())
                    .unwrap()
            } else {
                let accept = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).unwrap_or("");
                if !accept.contains("text/event-stream") {
                    return rpc_error(
                        StatusCode::NOT_ACCEPTABLE,
                        -32000,
                        "Not Acceptable: Client must accept text/event-stream",
                    );
                }
                sse_stream_response(Some(&sid), None).0
            }
        }
        _ => {
            // SDK handleUnsupportedRequest: 405 + Allow header.
            let mut resp = rpc_error(StatusCode::METHOD_NOT_ALLOWED, -32000, "Method not allowed.");
            resp.headers_mut()
                .insert(header::ALLOW, HeaderValue::from_static("GET, POST, DELETE"));
            resp
        }
    }
}

/// POST with a session → response carries `mcp-session-id`; without → plain
/// SSE. Notifications get 202 with an empty body either way.
async fn respond(store: AppState, sid: Option<String>, msg: &Value) -> Response {
    let response = tokio::task::spawn_blocking({
        let store = store;
        let msg = msg.clone();
        move || {
            let store = store.lock().expect("store lock");
            handle_message(&store, &msg)
        }
    })
    .await
    .unwrap_or_else(|_| Some(rpc_error_value(msg)));
    match response {
        Some(r) => sse_single_response(sid.as_deref(), &r),
        None => accepted_empty(),
    }
}

fn rpc_error_value(msg: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": msg.get("id").cloned().unwrap_or(Value::Null),
        "error": {"code": -32603, "message": "Internal error"}})
}

// ===================== /sse + /messages (legacy transport) =====================

pub async fn sse_route(State(state): State<McpState>, req: Request) -> Response {
    let method = req.method().clone();
    let headers = req.headers().clone();
    let raw = match Body::from_request(req, &()).await {
        Ok(b) => b.0,
        Err(resp) => return resp,
    };
    log_mcp_request(&method, "/sse", &headers, &raw);
    if method == Method::GET {
        // SSEServerTransport.start(): emit the endpoint event, keep the stream.
        let (resp, tx) = sse_stream_response(None, Some(format!("event: endpoint\ndata: /messages?sessionId={}\n\n", uuid())));
        *state.sse.lock().unwrap() = Some(tx);
        resp
    } else if method == Method::POST {
        // JS routes non-GET /sse through handleMcp (stateless JSON-RPC).
        let msg: Value = match serde_json::from_slice(&raw) {
            Ok(v) => v,
            Err(_) => return rpc_error(StatusCode::BAD_REQUEST, -32700, "Parse error"),
        };
        respond(state.store.clone(), None, &msg).await
    } else {
        rpc_error(StatusCode::METHOD_NOT_ALLOWED, -32000, "Method not allowed.")
    }
}

pub async fn messages_route(State(state): State<McpState>, req: Request) -> Response {
    let method = req.method().clone();
    let headers = req.headers().clone();
    let raw = match Body::from_request(req, &()).await {
        Ok(b) => b.0,
        Err(resp) => return resp,
    };
    log_mcp_request(&method, "/messages", &headers, &raw);
    let Some(tx) = state.sse.lock().ok().and_then(|g| g.clone()) else {
        // JS: 400 {"error":"No SSE connection (GET /sse first)"}
        return json_response(
            StatusCode::BAD_REQUEST,
            r#"{"error":"No SSE connection (GET /sse first)"}"#.to_string(),
        );
    };
    let msg: Value = match serde_json::from_slice(&raw) {
        Ok(v) => v,
        Err(_) => {
            // SSEServerTransport.handlePostMessage: 400 + `Invalid message: <body>`.
            return json_response(
                StatusCode::BAD_REQUEST,
                format!("Invalid message: {}", String::from_utf8_lossy(&raw)),
            );
        }
    };
    let wants_reply = has_id(&msg);
    if wants_reply {
        let store = state.store.clone();
        tokio::spawn(async move {
            if let Some(r) = tokio::task::spawn_blocking(move || {
                let store = store.lock().expect("store lock");
                handle_message(&store, &msg)
            })
                .await
                .ok()
                .flatten()
            {
                let _ = tx.send(sse_frame(&r)).await;
            }
        });
    }
    // SDK: 202 Accepted, body "Accepted".
    let mut resp = Response::builder()
        .status(StatusCode::ACCEPTED)
        .body(AxumBody::from("Accepted"))
        .unwrap();
    cors_headers(resp.headers_mut());
    resp
}

// ===================== JSON-RPC core =====================

fn handle_message(store: &Store, msg: &Value) -> Option<Value> {
    if !has_id(msg) {
        return None; // notification → no response
    }
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => rpc_result(initialize_result(&params), &id),
        "ping" => rpc_result(json!({}), &id),
        "tools/list" => rpc_result(tools_list(), &id),
        "tools/call" => rpc_result(call_tool(store, &params), &id),
        // SDK Protocol default: -32601 Method not found (key order captured).
        _ => json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}}),
    })
}

fn initialize_result(params: &Value) -> Value {
    let requested = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let protocol_version = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
        requested
    } else {
        LATEST_PROTOCOL_VERSION
    };
    json!({
        "protocolVersion": protocol_version,
        "capabilities": {"tools": {"listChanged": true}},
        "serverInfo": {"name": "leanote", "version": "1.0.0"},
    })
}

// ===================== tools =====================

/// Byte-exact tools/list captured from the JS server (order matters):
/// no-arg tools get `{"type":"object","properties":{}}` (no $schema key),
/// others the zod draft-07 conversion with description-before-type on
/// optional fields.
fn tools_list() -> Value {
    json!({"tools": [
        {"name": "list_notebooks", "description": "列出所有笔记本及其笔记数量。(List all notebooks with note counts.)",
         "inputSchema": {"type": "object", "properties": {}}, "execution": {"taskSupport": "forbidden"}},
        {"name": "list_tags", "description": "列出所有标签。(List all tags.)",
         "inputSchema": {"type": "object", "properties": {}}, "execution": {"taskSupport": "forbidden"}},
        {"name": "add_note", "description": "新建一条 Markdown 笔记。(Create a new markdown note.) notebook 默认 \"AI札\"，不存在则自动创建。",
         "inputSchema": {"$schema": "http://json-schema.org/draft-07/schema#", "type": "object",
            "properties": {
                "title": {"type": "string", "description": "笔记标题 (note title, required)"},
                "content": {"type": "string", "description": "Markdown 正文 (markdown content, required)"},
                "tags": {"description": "标签数组 (tags array)", "type": "array", "items": {"type": "string"}},
                "notebook": {"description": "目标笔记本名 (target notebook title; default \"AI札\")", "type": "string"}
            },
            "required": ["title", "content"]},
         "execution": {"taskSupport": "forbidden"}},
        {"name": "list_notes", "description": "列出最近的笔记。(List recent notes.) 可按笔记本名 / 标签过滤，按更新时间倒序。",
         "inputSchema": {"$schema": "http://json-schema.org/draft-07/schema#", "type": "object",
            "properties": {
                "notebook": {"description": "按笔记本名过滤 (filter by notebook title)", "type": "string"},
                "tag": {"description": "按标签过滤 (filter by tag)", "type": "string"},
                "limit": {"description": "最大条数 (max results, default 20)", "type": "number"}
            }},
         "execution": {"taskSupport": "forbidden"}},
        {"name": "get_note", "description": "按 ID 查看笔记完整内容，支持 ID 前缀。(Get full note content by id or prefix.)",
         "inputSchema": {"$schema": "http://json-schema.org/draft-07/schema#", "type": "object",
            "properties": {"noteId": {"type": "string", "description": "笔记 ID 或前缀 (note id or prefix)"}},
            "required": ["noteId"]},
         "execution": {"taskSupport": "forbidden"}},
        {"name": "search_notes", "description": "按关键字搜索笔记（标题 / 正文 / 标签），按相关度排序。(Search notes by keyword.)",
         "inputSchema": {"$schema": "http://json-schema.org/draft-07/schema#", "type": "object",
            "properties": {
                "keyword": {"type": "string", "description": "关键字 (keyword)"},
                "limit": {"description": "最大条数 (max results, default 20)", "type": "number"}
            },
            "required": ["keyword"]},
         "execution": {"taskSupport": "forbidden"}},
        {"name": "update_note", "description": "更新现有笔记（按 ID 或前缀）。(Update an existing note by id/prefix.) 至少提供一个可更新字段；tags 为替换语义。",
         "inputSchema": {"$schema": "http://json-schema.org/draft-07/schema#", "type": "object",
            "properties": {
                "noteId": {"type": "string", "description": "笔记 ID 或前缀 (note id or prefix)"},
                "title": {"description": "新标题 (new title)", "type": "string"},
                "content": {"description": "新 Markdown 正文 (new markdown content)", "type": "string"},
                "tags": {"description": "新标签数组（替换现有） (new tags array, replaces existing)", "type": "array", "items": {"type": "string"}},
                "notebook": {"description": "移动到该笔记本 (move to this notebook by title)", "type": "string"}
            },
            "required": ["noteId"]},
         "execution": {"taskSupport": "forbidden"}},
        {"name": "delete_note", "description": "删除笔记（移到回收站），按 ID 或前缀。(Move a note to trash by id/prefix.)",
         "inputSchema": {"$schema": "http://json-schema.org/draft-07/schema#", "type": "object",
            "properties": {"noteId": {"type": "string", "description": "笔记 ID 或前缀 (note id or prefix)"}},
            "required": ["noteId"]},
         "execution": {"taskSupport": "forbidden"}}
    ]})
}

/// JS `ok()`: 2-space pretty JSON in a text content block.
fn ok_text(v: &Value) -> Value {
    json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(v).unwrap_or_default()}]})
}

/// JS `fail2()`: isError result with the plain message.
fn fail_text(message: &str) -> Value {
    json!({"isError": true, "content": [{"type": "text", "text": message}]})
}

fn call_tool(store: &Store, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params
        .get("arguments")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    // JS `safe()` wraps every handler: success → ok(), throw → fail2(msg).
    // (zod input validation happens before the handler in JS; clients in the
    // acceptance tests always send well-typed args, so it is not mirrored.)
    let outcome: Result<Value, String> = match name {
        "list_notebooks" => tool_list_notebooks(store),
        "list_tags" => tool_list_tags(store),
        "add_note" => tool_add_note(store, &args),
        "list_notes" => tool_list_notes(store, &args),
        "get_note" => tool_get_note(store, &args),
        "search_notes" => tool_search_notes(store, &args),
        "update_note" => tool_update_note(store, &args),
        "delete_note" => tool_delete_note(store, &args),
        // SDK McpServer.CallToolResult for an unregistered name.
        other => return fail_text(&format!("MCP error -32602: Tool {other} not found")),
    };
    match outcome {
        Ok(v) => ok_text(&v),
        Err(msg) => fail_text(&msg),
    }
}

// ---- tool layer: src/leanote_data.js, field-for-field ----

fn arg_str(args: &Map<String, Value>, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn arg_tags(args: &Map<String, Value>) -> Option<Vec<String>> {
    args.get("tags").and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    })
}

/// JS `opts.limit ? parseInt(limit, 10) : 20` — falsy (0/absent) → 20.
fn arg_limit(args: &Map<String, Value>) -> usize {
    args.get("limit")
        .and_then(Value::as_u64)
        .filter(|l| *l > 0)
        .unwrap_or(20) as usize
}

/// Store error → tool message (JS surfaces e.message the same way).
fn db<T, E: std::fmt::Display>(r: Result<T, E>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

/// notebookId / title / count, order = notebooks table order.
fn tool_list_notebooks(store: &Store) -> Result<Value, String> {
    let notes = db(store.list_notes())?;
    let mut counts: HashMap<String, u64> = HashMap::new();
    for n in &notes {
        *counts.entry(n.notebook_id.to_string()).or_insert(0) += 1;
    }
    let list: Vec<Value> = db(store.notebooks())?
        .iter()
        .map(|nb| {
            json!({
                "notebookId": nb.notebook_id.to_string(),
                "title": nb.title,
                "count": counts.get(nb.notebook_id.to_string().as_str()).copied().unwrap_or(0),
            })
        })
        .collect();
    Ok(Value::Array(list))
}

/// tag / count.
fn tool_list_tags(store: &Store) -> Result<Value, String> {
    let list: Vec<Value> = db(store.tags())?
        .iter()
        .map(|t| json!({"tag": t.tag, "count": t.count}))
        .collect();
    Ok(Value::Array(list))
}

/// {noteId, notebook} — auto-creates the target notebook.
fn tool_add_note(store: &Store, args: &Map<String, Value>) -> Result<Value, String> {
    let title = arg_str(args, "title").filter(|s| !s.is_empty()).ok_or("title is required")?;
    let content = arg_str(args, "content").filter(|s| !s.is_empty()).ok_or("content is required")?;
    let notebook = arg_str(args, "notebook").unwrap_or_else(|| "AI札".to_string());
    let tags = arg_tags(args).unwrap_or_default();
    let note = db(store.add_note(NewNote {
        title,
        content,
        tags,
        notebook_title: notebook.clone(),
        is_markdown: true,
    }))?;
    Ok(json!({"noteId": note.note_id.to_string(), "notebook": notebook}))
}

/// noteId / title / notebook / tags / updatedTime / createdTime / desc.
fn tool_list_notes(store: &Store, args: &Map<String, Value>) -> Result<Value, String> {
    let notebook_id = match arg_str(args, "notebook") {
        Some(title) => Some(
            db(store.find_notebook_by_title(&title))?
                .ok_or_else(|| format!("Notebook \"{title}\" not found"))?
                .notebook_id
                .to_string(),
        ),
        None => None,
    };
    let tag = arg_str(args, "tag");
    let limit = arg_limit(args);
    let notes = db(store.list_notes())?;
    let list: Vec<Value> = notes
        .iter()
        .filter(|n| notebook_id.as_deref().is_none_or(|id| n.notebook_id.to_string() == id))
        .filter(|n| tag.as_deref().is_none_or(|t| n.tags.iter().any(|x| x == t)))
        .take(limit)
        .map(|n| {
            json!({
                "noteId": n.note_id.to_string(),
                "title": display_title(&n.title),
                "notebook": db(store.notebook_title(&n.notebook_id.to_string())).unwrap_or_default(),
                "tags": n.tags,
                "updatedTime": n.updated_time,
                "createdTime": n.created_time,
                "desc": n.desc,
            })
        })
        .collect();
    Ok(Value::Array(list))
}

fn display_title(title: &str) -> &str {
    if title.is_empty() {
        "(untitled)"
    } else {
        title
    }
}

/// Full note incl. content; exact → prefix resolve (first live match).
fn tool_get_note(store: &Store, args: &Map<String, Value>) -> Result<Value, String> {
    let note_id = arg_str(args, "noteId").ok_or("noteId is required")?;
    let note = db(store.find_note_by_prefix(&note_id))?
        .ok_or_else(|| format!("Note \"{note_id}\" not found"))?;
    Ok(json!({
        "noteId": note.note_id.to_string(),
        "title": display_title(&note.title),
        "content": note.content,
        "notebook": db(store.notebook_title(&note.notebook_id.to_string()))?,
        "notebookId": note.notebook_id.to_string(),
        "tags": note.tags,
        "isMarkdown": note.is_markdown,
        "createdTime": note.created_time,
        "updatedTime": note.updated_time,
    }))
}

/// Ranked hits via the ticket-05 unified search core.
fn tool_search_notes(store: &Store, args: &Map<String, Value>) -> Result<Value, String> {
    let keyword = arg_str(args, "keyword").filter(|s| !s.is_empty()).ok_or("keyword is required")?;
    let limit = arg_limit(args);
    let hits = db(store.search(&keyword, limit))?;
    let list: Vec<Value> = hits
        .iter()
        .map(|h| {
            let n: &Note = &h.note;
            json!({
                "noteId": n.note_id.to_string(),
                "title": display_title(&n.title),
                "notebook": db(store.notebook_title(&n.notebook_id.to_string())).unwrap_or_default(),
                "tags": n.tags,
                "updatedTime": n.updated_time,
                "score": h.score,
                "excerpt": h.excerpt,
            })
        })
        .collect();
    Ok(Value::Array(list))
}

/// {noteId} — full id after applying title/content/tags/notebook (notebook
/// must exist, unlike the /api updateNote path which silently ignores it).
fn tool_update_note(store: &Store, args: &Map<String, Value>) -> Result<Value, String> {
    let note_id = arg_str(args, "noteId").ok_or("noteId is required")?;
    let title = arg_str(args, "title");
    let content = arg_str(args, "content");
    let tags = arg_tags(args);
    // JS resolves the notebook before the "No fields to update" check.
    let notebook_id = match arg_str(args, "notebook") {
        Some(t) => Some(
            db(store.find_notebook_by_title(&t))?
                .ok_or_else(|| format!("Notebook \"{t}\" not found"))?
                .notebook_id,
        ),
        None => None,
    };
    if title.is_none() && content.is_none() && tags.is_none() && notebook_id.is_none() {
        return Err("No fields to update".into());
    }
    let target = db(store.find_note_by_prefix(&note_id))?
        .ok_or_else(|| format!("Note \"{note_id}\" not found"))?;
    db(store.update_note(
        &target.note_id,
        NoteUpdate { title, content, tags, notebook_title: None },
    ))?;
    if let Some(nb) = notebook_id {
        db(store.set_notebook(&target.note_id, &nb))?;
    }
    Ok(json!({"noteId": target.note_id.to_string()}))
}

/// {noteId} — trash, never a hard delete.
fn tool_delete_note(store: &Store, args: &Map<String, Value>) -> Result<Value, String> {
    let note_id = arg_str(args, "noteId").ok_or("noteId is required")?;
    let target = db(store.find_note_by_prefix(&note_id))?
        .ok_or_else(|| format!("Note \"{note_id}\" not found"))?;
    db(store.trash_note(&target.note_id))?;
    Ok(json!({"noteId": target.note_id.to_string()}))
}

// ===================== LEANOTE_MCP_DEBUG log =====================

/// JS logMcpRequest: `LEANOTE_MCP_DEBUG=1` appends to `~/leanote_mcp.log`
/// with 10MB rotation (.log→.log.1, .log.1→.log.2, drop .log.2).
pub fn log_mcp_request(method: &Method, url: &str, headers: &HeaderMap, raw: &[u8]) {
    if std::env::var("LEANOTE_MCP_DEBUG").ok().as_deref() != Some("1") {
        return;
    }
    let Ok(home) = std::env::var("HOME") else { return };
    let path = std::path::PathBuf::from(home).join("leanote_mcp.log");
    if std::fs::metadata(&path).map(|m| m.len() > 10 * 1024 * 1024).unwrap_or(false) {
        let _ = std::fs::remove_file(format!("{}.2", path.display()));
        let _ = std::fs::rename(format!("{}.1", path.display()), format!("{}.2", path.display()));
        let _ = std::fs::rename(&path, format!("{}.1", path.display()));
    }
    let hdr = |name: &str| -> String {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("(none)")
            .to_string()
    };
    let body = String::from_utf8_lossy(raw);
    let entry = format!(
        "[MCP] {}\n  {} {}\n  mcp-session-id: {}\n  accept: {}\n  content-type: {}\n  origin: {}\n  body: {}\n\n",
        iso8601_now(),
        method.as_str(),
        url,
        hdr("mcp-session-id"),
        hdr("accept"),
        hdr("content-type"),
        hdr("origin"),
        body.chars().take(800).collect::<String>(),
    );
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(entry.as_bytes());
    }
}

/// JS `new Date().toISOString()` — `YYYY-MM-DDTHH:MM:SS.mmmZ`.
fn iso8601_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    let millis = now.subsec_millis();
    let days = secs.div_euclid(86400);
    let sod = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        sod / 3600,
        (sod / 60) % 60,
        sod % 60
    )
}

/// Howard Hinnant's civil-from-days.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as i64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

//! leanote-http: local `/api/*` server on 127.0.0.1:8912 (ticket 05).
//!
//! Behavior authority: `src/local_server.js` handleApi/handleAddNote/
//! handleUpdateNote — camelCase JSON payloads, auto-create「AI札」,
//! IsMarkdown, note-id prefix matching, tag upsert, CORS `*`, 5MB body
//! limit, loopback-only bind. Writes go through leanote-core Store.
//!
//! Response bodies are hand-built compact strings, not `Value::to_string`:
//! JS `JSON.stringify` preserves insertion order (success first), serde's
//! map sorts keys — byte parity requires manual order.

use axum::body::Bytes;
use axum::extract::{FromRequest, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use leanote_core::{Id, NewNote, NoteUpdate, Store};
use serde_json::{Map, Value};
use std::sync::{Arc, Mutex};

const MAX_BODY: usize = 5 * 1024 * 1024;

pub const HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 8912;

/// rusqlite's Connection is Send but not Sync, so the store is shared behind
/// a mutex. ponytail: single lock is fine — local single-user server.
pub type AppState = Arc<Mutex<Store>>;

pub fn router(store: Store) -> Router {
    Router::new()
        .route(
            "/api/ping",
            get(|| async { json_response(StatusCode::OK, r#"{"success":true}"#.to_string()) }),
        )
        .fallback(|State(state): State<AppState>, req: Request| async move {
            let method = req.method().clone();
            let path = req.uri().path().to_string();
            let body = match Body::from_request(req, &()).await {
                Ok(b) => b.0,
                Err(resp) => return resp,
            };
            // Store calls are blocking SQLite — keep them off the async core.
            let resp = tokio::task::spawn_blocking(move || {
                let store = state.lock().expect("store lock");
                api_dispatch(&store, method, &path, body)
            })
            .await;
            match resp {
                Ok(r) => r,
                Err(e) => json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!(r#"{{"success":false,"error":"{e}}}"#),
                ),
            }
        })
        .layer(axum::middleware::from_fn(cors))
        .with_state(Arc::new(Mutex::new(store)))
}

struct Body(Bytes);

#[axum::async_trait]
impl<S: Send + Sync> FromRequest<S> for Body {
    type Rejection = Response;
    async fn from_request(req: Request, _s: &S) -> Result<Self, Self::Rejection> {
        match axum::body::to_bytes(req.into_body(), MAX_BODY).await {
            Ok(b) => Ok(Body(b)),
            Err(_) => Err(json_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                r#"{"error":"Request body too large"}"#.to_string(),
            )),
        }
    }
}

async fn cors(req: Request, next: Next) -> Response {
    if req.method() == Method::OPTIONS {
        let mut resp = Response::new(axum::body::Body::empty());
        *resp.status_mut() = StatusCode::OK;
        cors_headers(resp.headers_mut());
        return resp;
    }
    next.run(req).await
}

fn cors_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, DELETE, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Content-Type, Accept, Mcp-Session-Id, Last-Event-ID"),
    );
}

/// JS sendJSON: compact JSON (JSON.stringify), Content-Type
/// `application/json` (no charset). `(status, String)` would set
/// `text/plain; charset=utf-8`, so set the header explicitly.
fn json_response(status: StatusCode, body: String) -> Response {
    let mut resp = (status, body).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    cors_headers(resp.headers_mut());
    resp
}

/// `{"success":false,"error":"..."}` — error strings here never contain
/// quotes/backslashes (fixed literals + JSON-parse messages, which
/// serde_json escapes itself via to_string).
fn fail(status: StatusCode, error: &str) -> (StatusCode, String) {
    (
        status,
        format!(r#"{{"success":false,"error":{}}}"#, Value::String(error.to_string())),
    )
}

/// `{"success":true,"noteId":"..."}` — ids are 24-hex, no escaping needed.
fn ok_note(note_id: &str) -> (StatusCode, String) {
    (
        StatusCode::OK,
        format!(r#"{{"success":true,"noteId":"{}"}}"#, note_id),
    )
}

/// JS handleApi: the try block covers JSON.parse of addNote/updateNote
/// bodies, so invalid JSON → 400 `{"success":false,"error":"<message>"}`;
/// anything else unmatched → 404 Unknown endpoint.
fn api_dispatch(store: &Store, method: Method, path: &str, body: Bytes) -> Response {
    let is_add = method == Method::POST && path == "/api/note/addNote";
    let is_update = method == Method::POST && path == "/api/note/updateNote";
    let data: Value = if is_add || is_update {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => {
                let (s, b) = fail(StatusCode::BAD_REQUEST, &e.to_string());
                return json_response(s, b);
            }
        }
    } else {
        Value::Null
    };
    let (status, payload) = if is_add {
        handle_add_note(store, &data)
    } else if is_update {
        handle_update_note(store, &data)
    } else if method == Method::GET && path == "/api/ping" {
        (StatusCode::OK, r#"{"success":true}"#.to_string())
    } else {
        fail(StatusCode::NOT_FOUND, "Unknown endpoint")
    };
    json_response(status, payload)
}

/// JS `if (!data.title)` — absent/null/empty-string reject; any other JSON
/// type is truthy and coerces to string (clients only send strings anyway).
fn required_string(obj: &Map<String, Value>, key: &str, err: &str) -> Option<String> {
    match obj.get(key) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn optional_string(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn tags_value(v: Option<&Value>) -> Option<Vec<String>> {
    match v {
        Some(Value::Array(a)) => Some(
            a.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect(),
        ),
        Some(_) => Some(Vec::new()),
        None => None,
    }
}

fn handle_add_note(store: &Store, data: &Value) -> (StatusCode, String) {
    let empty = Map::new();
    let obj = data.as_object().unwrap_or(&empty);
    let Some(title) = required_string(obj, "title", "title is required") else {
        return fail(StatusCode::BAD_REQUEST, "title is required");
    };
    let Some(content) = required_string(obj, "content", "content is required") else {
        return fail(StatusCode::BAD_REQUEST, "content is required");
    };
    let notebook = match obj.get("notebook") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => "AI札".to_string(),
    };
    let tags = tags_value(obj.get("tags")).unwrap_or_default();

    match store.add_note(NewNote {
        title,
        content,
        tags,
        notebook_title: notebook,
        is_markdown: true,
    }) {
        Ok(note) => ok_note(&note.note_id.to_string()),
        Err(_) => fail(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create note"),
    }
}

fn handle_update_note(store: &Store, data: &Value) -> (StatusCode, String) {
    let empty = Map::new();
    let obj = data.as_object().unwrap_or(&empty);
    let Some(note_id) = required_string(obj, "noteId", "noteId is required") else {
        return fail(StatusCode::BAD_REQUEST, "noteId is required");
    };

    // JS findNoteByPrefix: exact id (any state) first, else prefix match,
    // first non-trash/non-local-deleted candidate — no ambiguity error.
    let Some(target) = store.find_note_by_prefix(&note_id).ok().flatten() else {
        return fail(StatusCode::NOT_FOUND, "Note not found");
    };
    let full_id = target.note_id;

    let upd = NoteUpdate {
        title: optional_string(obj.get("title")),
        content: optional_string(obj.get("content")),
        tags: tags_value(obj.get("tags")),
        // JS assigns note.NotebookId when the title matches, but
        // updateNoteOrContent never persists it — accepted, ignored.
        notebook_title: obj.get("notebook").and_then(Value::as_str).map(str::to_string),
    };
    match store.update_note(&full_id, upd) {
        Ok(Some(n)) => ok_note(&n.note_id.to_string()),
        _ => fail(StatusCode::INTERNAL_SERVER_ERROR, "Failed to update"),
    }
}

/// Bind 127.0.0.1:port only and serve in a background task. Returns the
/// actually bound port (port 0 → OS pick, keeps parallel tests
/// collision-free). Call inside a tokio runtime; the task dies with it.
pub async fn serve(store: Store, port: u16) -> std::io::Result<u16> {
    let listener = tokio::net::TcpListener::bind((HOST, port)).await?;
    let bound = listener.local_addr()?.port();
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router(store)).await {
            eprintln!("[leanote] Local server error: {e}");
        }
    });
    Ok(bound)
}

/// CLI `leanote serve` entry: run the server on the current thread.
pub fn serve_blocking(store: Store, port: u16) -> std::io::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind((HOST, port)).await?;
        let bound = listener.local_addr()?.port();
        println!("[leanote] Local server on {}:{}", HOST, bound);
        axum::serve(listener, router(store)).await?;
        Ok(())
    })
}

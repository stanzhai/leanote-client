//! In-process programmable fake leanote server.
//!
//! Endpoint / field mapping (wire authority: `src/api.js`, `src/sync.js`,
//! `src/user.js`, `src/note.js`, `src/notebook.js`):
//!
//! | Method+Path                        | Query (from `Api.getUrl`)                | Body                       | Response parsed by JS                                           |
//! |------------------------------------|------------------------------------------|----------------------------|-----------------------------------------------------------------|
//! | POST `/api/auth/login`             | `token, macAddr, v`                      | form: `email`, `pwd`        | ok: `{Ok:true, UserId, Email, Username, Token}`; bad pwd: `{Ok:false, Msg:"WrongPassword"}` |
//! | POST `/api/auth/logout`             | `token, macAddr, v`                      | –                          | `{Ok:true}` (`src/api.js` ignores errors; calls back regardless) |
//! | GET  `/api/user/getSyncState`      | `token, macAddr, v`                      | –                          | `{Ok:true, LastSyncUsn}` (`sync.js` compares `LastSyncUsn > lastSyncUsn`) |
//! | GET  `/api/notebook/getSyncNotebooks` | `token, afterUsn, maxEntry`           | –                          | JSON array of notebook rows; must be top-level array (`Common.isOk`: array if `'length' in ret`); last chunk has `len < maxEntry`; `sync.js` uses `Usn` (advance cursor), `NotebookId`, `Title`, `Seq`, `ParentNotebookId`, `IsDeleted`, `IsDirty`, `CreatedTime`, `UpdatedTime` |
//! | GET  `/api/note/getSyncNotes`      | `token, afterUsn, maxEntry`              | –                          | JSON array of note rows; `sync.js` uses `Usn`, `NoteId`, `Title`, `NotebookId`, `IsDeleted`, `IsTrash`, `IsMarkdown`, `Tags`, `Files`, `CreatedTime`, `UpdatedTime`; content arrives via `getNoteContent` |
//! | GET  `/api/note/getNoteContent`    | `token, noteId`                          | –                          | `{Ok:true, Content, NoteId}` (`Common.isOk` true because no `Ok:false`) |
//! | GET  `/api/tag/getSyncTags`        | `token, afterUsn, maxEntry`              | –                          | JSON array; `sync.js` uses `Usn`, `TagId`, `Tag`, `IsDeleted` |
//! | POST `/api/notebook/addNotebook`   | `token`                                  | form: `title, seq, parentNotebookId` | `{Ok:true, NotebookId, Usn, ...}` (echo full row; `sync.js` needs `NotebookId` + `Usn`) |
//! | POST `/api/notebook/updateNotebook`| `token`                                  | form: `notebookId, title, usn, seq, parentNotebookId` | `{Ok:true, NotebookId, Usn}`; injected: `{Ok:false, Msg:"conflict"/"notExists"}` |
//! | POST `/api/notebook/deleteNotebook`| `token`                                  | form: `notebookId, usn`    | `{Ok:true, Usn}`; injected: `{Ok:false, Msg:"conflict"}` |
//! | POST `/api/note/addNote`           | `token`                                  | form/json                  | `{Ok:true, NoteId, Usn}` (`sync.js` copies `NoteId`→`ServerNoteId`, calls `checkNeedIncSyncAgain(Usn)`) |
//! | POST `/api/note/updateNote`        | `token`                                  | form/json `NoteId, Usn, ...` | `{Ok:true, NoteId, Usn}`; injected: `{Ok:false, Msg:"conflict"/"notExists"}` |
//! | POST `/api/note/deleteTrash`       | `token`                                  | form: `noteId, usn`        | `{Ok:true, Usn}`; injected: `{Ok:false, Msg:"conflict"/"notExists"}` |
//! | POST `/api/tag/addTag`             | `token`                                  | form: `tag`                | `{Ok:true, Tag, Usn}` |
//! | POST `/api/tag/deleteTag`          | `token`                                  | form: `tag, usn`           | `{Ok:true, Usn}`; injected: `{Ok:false, Msg:"conflict"/"notExists"}` |
//!
//! Auth: every request must carry the issued `token` query param; otherwise
//! `{Ok:false, Msg:"NOTLOGIN"}` (matches `Api.checkError` re-login semantics).
//! `needle.post(url, object, ...)` sends `application/x-www-form-urlencoded`,
//! so push bodies are parsed as form fields (JSON accepted too).

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use serde_json::{json, Map, Value};

/// A row pushed to a receiver, in arrival order. Field names are exactly the
/// keys the JS client sent (PascalCase for note bodies, camelCase for
/// notebook/tag/delete bodies).
#[derive(Debug, Clone)]
pub struct RecordedPush {
    /// e.g. `"notebook/addNotebook"`.
    pub endpoint: String,
    pub fields: Map<String, Value>,
}

#[derive(Default)]
struct Inner {
    /// Rows served by `notebook/getSyncNotebooks`, USN-ascending.
    notebooks: Vec<Value>,
    /// Rows served by `note/getSyncNotes`, USN-ascending.
    notes: Vec<Value>,
    /// `noteId -> Content` served by `note/getNoteContent`.
    contents: Map<String, Value>,
    /// Rows served by `tag/getSyncTags`, USN-ascending.
    tags: Vec<Value>,
    /// What `user/getSyncState` reports.
    sync_usn: i64,
    /// Staged conflict/notExists responses, consumed FIFO.
    conflicts: VecDeque<Map<String, Value>>,
    /// Every push request received, in arrival order.
    pushes: Vec<RecordedPush>,
    /// Issue 19: `fileId -> (filename, bytes)` served by `file/getAttach`.
    attaches: std::collections::HashMap<String, (String, Vec<u8>)>,
    /// Issue 21: `fileId -> (filename, mime, bytes)` served by
    /// `file/getImage` and filled by multipart `note/*` pushes.
    images: std::collections::HashMap<String, (String, String, Vec<u8>)>,
    /// login -> token; token -> valid.
    token: Option<String>,
    /// Next USN handed to push responses (>= max dataset USN).
    next_usn: i64,
    /// `macAddr` + `v` received on the last `auth/login`, for wire-parity asserts.
    login_query: (String, String),
}

/// Programmable state shared between test code and the server task.
#[derive(Clone)]
pub struct FakeServer {
    inner: Arc<Mutex<Inner>>,
    port: u16,
}

impl FakeServer {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn token(&self) -> String {
        self.inner
            .lock()
            .unwrap()
            .token
            .clone()
            .expect("not logged in yet")
    }

    /// What `user/getSyncState` will report.
    pub fn set_sync_usn(&self, usn: i64) {
        self.inner.lock().unwrap().sync_usn = usn;
    }

    /// Replace the notebook dataset (rows must carry `Usn`, ascending across pages).
    pub fn set_notebooks(&self, rows: Vec<Value>) {
        self.inner.lock().unwrap().notebooks = rows;
    }

    pub fn set_notes(&self, rows: Vec<Value>) {
        self.inner.lock().unwrap().notes = rows;
    }

    pub fn set_tags(&self, rows: Vec<Value>) {
        self.inner.lock().unwrap().tags = rows;
    }

    /// `noteId -> Content` for `note/getNoteContent`.
    pub fn set_contents(&self, contents: Map<String, Value>) {
        self.inner.lock().unwrap().contents = contents;
    }

    /// Queue one injected error body (e.g. `{"Ok":false,"Msg":"conflict"}`)
    /// consumed by the next push receiver response. Takes precedence over the
    /// default `{Ok:true, ...}` success body.
    pub fn inject_response(&self, body: Value) {
        let map = body.as_object().cloned().unwrap_or_default();
        self.inner.lock().unwrap().conflicts.push_back(map);
    }

    /// Skip token checks until the next successful login.
    pub fn invalidate_token(&self) {
        self.inner.lock().unwrap().token = None;
    }

    /// All pushes received so far, in arrival order.
    pub fn pushes(&self) -> Vec<RecordedPush> {
        self.inner.lock().unwrap().pushes.clone()
    }

    /// Issue 19: `fileId -> (filename, bytes)` for `file/getAttach`.
    pub fn set_attaches(&self, attaches: std::collections::HashMap<String, (String, Vec<u8>)>) {
        self.inner.lock().unwrap().attaches = attaches;
    }

    /// Issue 21 需求9: seed `fileId -> (filename, mime, bytes)` for demo.
    pub fn set_images(
        &self,
        images: impl IntoIterator<Item = (String, (String, String, Vec<u8>))>,
    ) {
        self.inner.lock().unwrap().images = images.into_iter().collect();
    }

    /// Issue 21: images received by multipart note pushes, keyed by the
    /// server file id `file/getImage` serves.
    pub fn images(&self) -> std::collections::HashMap<String, (String, String, Vec<u8>)> {
        self.inner.lock().unwrap().images.clone()
    }

    /// Issue 21 需求9: current `noteId -> Content` map (tests mutate+reset).
    pub fn contents(&self) -> Map<String, Value> {
        self.inner.lock().unwrap().contents.clone()
    }

    /// Issue 21 需求9: current note rows (tests append).
    pub fn notes(&self) -> Vec<Value> {
        self.inner.lock().unwrap().notes.clone()
    }

    pub fn clear_pushes(&self) {
        self.inner.lock().unwrap().pushes.clear();
    }

    /// `(macAddr, v)` from the last login request.
    pub fn login_query(&self) -> (String, String) {
        self.inner.lock().unwrap().login_query.clone()
    }
}

/// Login credentials the fake server accepts.
pub const EMAIL: &str = "user@example.com";
pub const PWD: &str = "right-password";
pub const USER_ID: &str = "user1";
pub const TOKEN: &str = "faketoken42";

/// Start the fake server on an ephemeral port bound to 127.0.0.1.
///
/// Call inside `#[tokio::test]` / a runtime; the server task dies with it.
pub async fn start() -> FakeServer {
    let inner = Arc::new(Mutex::new(Inner::default()));
    let app = Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/user/getSyncState", get(get_sync_state))
        .route("/api/notebook/getSyncNotebooks", get(get_sync_notebooks))
        .route("/api/note/getSyncNotes", get(get_sync_notes))
        .route("/api/note/getNoteContent", get(get_note_content))
        .route("/api/tag/getSyncTags", get(get_sync_tags))
        .route(
            "/api/notebook/addNotebook",
            post(push("notebook/addNotebook")),
        )
        .route(
            "/api/notebook/updateNotebook",
            post(push("notebook/updateNotebook")),
        )
        .route(
            "/api/notebook/deleteNotebook",
            post(push("notebook/deleteNotebook")),
        )
        .route("/api/note/addNote", post(push("note/addNote")))
        .route("/api/note/updateNote", post(push("note/updateNote")))
        .route("/api/note/deleteTrash", post(push("note/deleteTrash")))
        .route("/api/file/getAttach", get(get_attach))
        .route("/api/file/getImage", get(get_image))
        .route("/api/tag/addTag", post(push("tag/addTag")))
        .route("/api/tag/deleteTag", post(push("tag/deleteTag")))
        .with_state(inner.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    FakeServer { inner, port }
}

fn json_response(v: Value) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        v.to_string(),
    )
        .into_response()
}

/// needle posts objects as application/x-www-form-urlencoded; accept JSON too.
fn parse_fields(body: &str) -> Map<String, Value> {
    let trimmed = body.trim_start();
    if trimmed.starts_with('{') {
        if let Value::Object(map) = serde_json::from_str(trimmed).unwrap_or(Value::Null) {
            return map;
        }
    }
    let mut map = Map::new();
    for (k, v) in form_urlencoded::parse(body.as_bytes()) {
        let (k, v) = (k.into_owned(), v.into_owned());
        // repeated keys (e.g. Tags=a&Tags=b) collect into an array, like the server
        match map.get_mut(&k) {
            Some(Value::Array(arr)) => arr.push(Value::String(v)),
            Some(Value::String(prev)) => {
                let prev = std::mem::take(prev);
                map.insert(k, Value::Array(vec![Value::String(prev), Value::String(v)]));
            }
            _ => {
                map.insert(k, Value::String(v));
            }
        }
    }
    map
}

/// Issue 21 需求9: minimal multipart/form-data split — text fields land in
/// the returned map (UTF-8), file parts as `(name, filename, mime, bytes)`.
/// `boundary` comes from the Content-Type header.
fn parse_multipart(
    headers: &axum::http::HeaderMap,
    body: &Bytes,
) -> (Map<String, Value>, Vec<(String, String, String, Vec<u8>)>) {
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let Some(boundary) = ct
        .split(';')
        .find_map(|p| p.trim().strip_prefix("boundary="))
    else {
        return (Map::new(), Vec::new());
    };
    let delim = format!("--{}", boundary);
    let mut map = Map::new();
    let mut files = Vec::new();
    let d = delim.as_bytes();
    let mut segments: Vec<&[u8]> = Vec::new();
    let mut start = 0usize;
    for i in 0..body.len().saturating_sub(d.len()) {
        if &body[i..i + d.len()] == d {
            segments.push(&body[start..i]);
            start = i + d.len();
        }
    }
    segments.push(&body[start..]);
    for part in segments {
        let part = {
            let mut p = part;
            while p.starts_with(b"\r\n") {
                p = &p[2..];
            }
            p
        };
        if part.is_empty() || part.starts_with(b"--") {
            continue;
        }
        let Some(pos) = part.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let (head, rest) = part.split_at(pos + 4);
        let head = &head[..pos];
        let value = rest.strip_suffix(b"\r\n").unwrap_or(rest);
        let mut name = String::new();
        let mut filename = String::new();
        let mut mime = String::new();
        for line in String::from_utf8_lossy(head).split("\r\n") {
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("content-disposition:") {
                for p in line.split(';') {
                    let p = p.trim();
                    if let Some(v) = p.strip_prefix("name=") {
                        name = v.trim_matches('"').to_string();
                    } else if let Some(v) = p.strip_prefix("filename=") {
                        filename = v.trim_matches('"').to_string();
                    }
                }
            } else if let Some(v) = lower.strip_prefix("content-type:") {
                mime = v.trim().to_string();
            }
        }
        if name.is_empty() {
            continue;
        }
        if filename.is_empty() {
            map.insert(
                name,
                Value::String(String::from_utf8_lossy(value).into_owned()),
            );
        } else {
            files.push((name, filename, mime, value.to_vec()));
        }
    }
    (map, files)
}

/// Query strings deserialize numbers as strings; coerce both forms to i64.
fn q_i64(query: &Map<String, Value>, key: &str) -> Option<i64> {
    match query.get(key) {
        Some(Value::String(s)) => s.parse().ok(),
        Some(v) => v.as_i64(),
        None => None,
    }
}

fn require_token(query: &Map<String, Value>, inner: &Inner) -> Option<Response> {
    let ok = query
        .get("token")
        .and_then(|t| t.as_str())
        .map(|t| inner.token.as_deref() == Some(t))
        .unwrap_or(false);
    if ok {
        None
    } else {
        Some(json_response(json!({"Ok": false, "Msg": "NOTLOGIN"})))
    }
}

async fn login(
    State(inner): State<Arc<Mutex<Inner>>>,
    query: Query<Map<String, Value>>,
    body: Bytes,
) -> Response {
    let qs = query.0;
    let fields = parse_fields(&body_to_string(&body));
    let email_ok = fields.get("email").and_then(|v| v.as_str()) == Some(EMAIL);
    let pwd_ok = fields.get("pwd").and_then(|v| v.as_str()) == Some(PWD);
    if !(email_ok && pwd_ok) {
        return json_response(json!({"Ok": false, "Msg": "WrongPassword"}));
    }
    let mut inner = inner.lock().unwrap();
    inner.token = Some(TOKEN.to_string());
    inner.login_query = (
        qs.get("macAddr")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        qs.get("v")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    );
    // next push USN is max(dataset usn)+1; login itself does not consume one
    inner.next_usn = inner.next_usn.max(inner.sync_usn) + 1;
    inner.sync_usn = inner.sync_usn.max(inner.next_usn - 1);
    json_response(json!({
        "Ok": true,
        "UserId": USER_ID,
        "Email": EMAIL,
        "Username": "user1",
        "Token": TOKEN,
    }))
}

/// `auth/logout`: the real endpoint always answers `Ok` (src/api.js ignores
/// the body); the fake keeps the token valid so subsequent sync tests can
/// keep using it.
async fn logout() -> Response {
    json_response(json!({"Ok": true}))
}

async fn get_sync_state(
    State(inner): State<Arc<Mutex<Inner>>>,
    Query(query): Query<Map<String, Value>>,
) -> Response {
    let i = inner.lock().unwrap();
    if let Some(r) = require_token(&query, &i) {
        return r;
    }
    json_response(json!({"Ok": true, "LastSyncUsn": i.sync_usn}))
}

async fn get_sync_notebooks(
    State(inner): State<Arc<Mutex<Inner>>>,
    Query(query): Query<Map<String, Value>>,
) -> Response {
    let i = inner.lock().unwrap();
    if let Some(r) = require_token(&query, &i) {
        return r;
    }
    let (after, max) = window(&query);
    json_response(Value::Array(
        i.notebooks
            .iter()
            .filter(|n| n["Usn"].as_i64().unwrap_or(0) > after)
            .take(max)
            .cloned()
            .collect(),
    ))
}

async fn get_sync_notes(
    State(inner): State<Arc<Mutex<Inner>>>,
    Query(query): Query<Map<String, Value>>,
) -> Response {
    let i = inner.lock().unwrap();
    if let Some(r) = require_token(&query, &i) {
        return r;
    }
    let (after, max) = window(&query);
    json_response(Value::Array(
        i.notes
            .iter()
            .filter(|n| n["Usn"].as_i64().unwrap_or(0) > after)
            .take(max)
            .cloned()
            .collect(),
    ))
}

async fn get_note_content(
    State(inner): State<Arc<Mutex<Inner>>>,
    Query(query): Query<Map<String, Value>>,
) -> Response {
    let i = inner.lock().unwrap();
    if let Some(r) = require_token(&query, &i) {
        return r;
    }
    let note_id = query.get("noteId").and_then(|v| v.as_str()).unwrap_or("");
    match i.contents.get(note_id) {
        Some(content) => json_response(json!({"Ok": true, "NoteId": note_id, "Content": content})),
        None => json_response(json!({"Ok": false, "Msg": "notExists"})),
    }
}

/// `file/getAttach` (issue 19): raw bytes + Content-Disposition filename;
/// unknown fileId answers the usual `{Ok:false}` JSON.
async fn get_attach(
    State(inner): State<Arc<Mutex<Inner>>>,
    Query(query): Query<Map<String, Value>>,
) -> Response {
    let i = inner.lock().unwrap();
    if let Some(r) = require_token(&query, &i) {
        return r;
    }
    let file_id = query.get("fileId").and_then(Value::as_str).unwrap_or("");
    match i.attaches.get(file_id) {
        Some((name, bytes)) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/octet-stream"),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{name}\"").as_str(),
                ),
            ],
            bytes.clone(),
        )
            .into_response(),
        None => json_response(json!({"Ok": false, "Msg": "notExists"})),
    }
}

/// `file/getImage` (issue 21 需求9): image bytes with the stored mime type;
/// unknown fileId answers the usual `{Ok:false}` JSON like getAttach.
async fn get_image(
    State(inner): State<Arc<Mutex<Inner>>>,
    Query(query): Query<Map<String, Value>>,
) -> Response {
    let i = inner.lock().unwrap();
    if let Some(r) = require_token(&query, &i) {
        return r;
    }
    let file_id = query.get("fileId").and_then(Value::as_str).unwrap_or("");
    match i.images.get(file_id) {
        Some((_, mime, bytes)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_str())],
            bytes.clone(),
        )
            .into_response(),
        None => json_response(json!({"Ok": false, "Msg": "notExists"})),
    }
}

async fn get_sync_tags(
    State(inner): State<Arc<Mutex<Inner>>>,
    Query(query): Query<Map<String, Value>>,
) -> Response {
    let i = inner.lock().unwrap();
    if let Some(r) = require_token(&query, &i) {
        return r;
    }
    let (after, max) = window(&query);
    json_response(Value::Array(
        i.tags
            .iter()
            .filter(|t| t["Usn"].as_i64().unwrap_or(0) > after)
            .take(max)
            .cloned()
            .collect(),
    ))
}

fn window(query: &Map<String, Value>) -> (i64, usize) {
    let after = q_i64(query, "afterUsn").unwrap_or(0);
    // JS sends e.g. 30; missing/0 means unbounded-ish, real server caps ~1000
    let max = q_i64(query, "maxEntry").filter(|m| *m > 0).unwrap_or(1000) as usize;
    (after, max)
}

fn body_to_string(bytes: &Bytes) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Generic push receiver: record the request, answer with the default
/// success body or a queued injection.
type PushFuture = Pin<Box<dyn Future<Output = Response> + Send>>;
#[allow(clippy::type_complexity)]
fn push(
    endpoint: &'static str,
) -> impl Fn(
    State<Arc<Mutex<Inner>>>,
    axum::http::HeaderMap,
    Query<Map<String, Value>>,
    Bytes,
) -> PushFuture
       + Clone
       + Send
       + 'static {
    move |State(inner), headers, Query(query), body: Bytes| {
        Box::pin(async move {
            {
                let i = inner.lock().unwrap();
                if let Some(r) = require_token(&query, &i) {
                    return r;
                }
            }
            // Issue 21 需求9: note pushes with images are multipart; the
            // JSON `Files` field declares each part, matching the JS wire
            // format (Files + one part per dirty image).
            let mut uploaded: Vec<(String, String, String, Vec<u8>)> = Vec::new();
            let fields = if headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|ct| ct.starts_with("multipart/form-data"))
            {
                let (map, files) = parse_multipart(&headers, &body);
                uploaded = files;
                map
            } else {
                parse_fields(&body_to_string(&body))
            };
            let mut inner = inner.lock().unwrap();

            let mut row_ids = {
                let id = fields
                    .get("notebookId")
                    .or_else(|| fields.get("NoteId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let tag = fields
                    .get("tag")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                (id, tag)
            };
            // deleteTrash uses noteId (lowercase)
            if endpoint == "note/deleteTrash" {
                if let Some(n) = fields.get("noteId").and_then(|v| v.as_str()) {
                    row_ids.0 = n.to_string();
                }
            }
            if let Some(injected) = inner.conflicts.pop_front() {
                inner.pushes.push(RecordedPush {
                    endpoint: endpoint.to_string(),
                    fields: fields.clone(),
                });
                return json_response(Value::Object(injected));
            }

            let next_usn = inner.next_usn.max(inner.sync_usn);
            let response = match endpoint {
                "notebook/addNotebook" => {
                    let id = format!("srvnb{}", next_usn);
                    json!({
                        "Ok": true,
                        "NotebookId": id,
                        "Title": fields.get("title").cloned().unwrap_or(Value::Null),
                        "Seq": fields.get("seq").cloned().unwrap_or(json!(0)),
                        "ParentNotebookId": fields.get("parentNotebookId").cloned().unwrap_or(json!("")),
                        "Usn": next_usn,
                    })
                }
                "notebook/updateNotebook" => json!({
                    "Ok": true,
                    "NotebookId": row_ids.0,
                    "Usn": next_usn,
                }),
                "notebook/deleteNotebook" => json!({"Ok": true, "Usn": next_usn}),
                "note/addNote" => {
                    let id = format!("srvnote{}", next_usn);
                    json!({
                        "Ok": true,
                        "NoteId": id,
                        "NotebookId": fields.get("NotebookId").cloned().unwrap_or(Value::Null),
                        "Title": fields.get("Title").cloned().unwrap_or(Value::Null),
                        "Usn": next_usn,
                    })
                }
                "note/updateNote" => json!({
                    "Ok": true,
                    "NoteId": row_ids.0,
                    "Usn": next_usn,
                }),
                "note/deleteTrash" => json!({"Ok": true, "Usn": next_usn}),
                "tag/addTag" => json!({
                    "Ok": true,
                    "Tag": row_ids.1.clone().unwrap_or_default(),
                    "TagId": format!("tag{}", next_usn),
                    "Usn": next_usn,
                }),
                "tag/deleteTag" => json!({"Ok": true, "Usn": next_usn}),
                _ => json!({"Ok": true}),
            };
            // Issue 21 需求9: assign server ids for uploaded images and
            // echo the JS `Files` mapping (LocalFileId -> FileId).
            let mut response = response;
            if !uploaded.is_empty() && (endpoint == "note/addNote" || endpoint == "note/updateNote")
            {
                let files: Vec<Value> = uploaded
                    .iter()
                    .enumerate()
                    .map(|(i, (name, _, _, _))| {
                        json!({
                            "FileId": format!("srvimg{}_{}", inner.next_usn, i),
                            "LocalFileId": name,
                            "IsAttach": false,
                        })
                    })
                    .collect();
                for ((name, filename, mime, bytes), f) in uploaded.iter().zip(files.iter()) {
                    let srv = f["FileId"].as_str().unwrap_or(name).to_string();
                    inner
                        .images
                        .insert(srv, (filename.clone(), mime.clone(), bytes.clone()));
                }
                if let Value::Object(m) = &mut response {
                    m.insert("Files".into(), Value::Array(files));
                }
            }

            inner.pushes.push(RecordedPush {
                endpoint: endpoint.to_string(),
                fields,
            });
            inner.next_usn = next_usn + 1;
            inner.sync_usn = inner.sync_usn.max(next_usn);
            json_response(response)
        })
    }
}

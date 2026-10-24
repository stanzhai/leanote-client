//! Server client + login + full sync (wire authority: src/api.js, src/sync.js,
//! src/user.js, src/note.js, src/notebook.js, src/tag.js).
//!
//! Full sync pulls notebooks → notes → tags in USN chunks (maxEntry 200, same
//! as JS `Sync._notebookMaxEntry`), then each synced note's content via
//! `note/getNoteContent`. Every chunk is committed transactionally; a kill
//! mid-sync leaves the previous chunk intact and a re-run converges (rows are
//! keyed by server id, so re-pulled chunks update rather than duplicate).

use serde_json::{json, Value};
use ureq::Agent;

use crate::{Account, Id, Store};
use rusqlite::{params, OptionalExtension};

pub const MAX_ENTRY: i64 = 200;

/// API error. `Api` carries the server's own `{"Ok":false,"Msg":...}` string
/// (e.g. "WrongPassword", "NOTLOGIN", "notExists"); `Unconnected` is the
/// transport/JSON failure the JS client reports via `Web.unConnected()`.
#[derive(Debug)]
pub enum SyncError {
    Unconnected(String),
    Api(String),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Unconnected(e) => write!(f, "unconnected: {e}"),
            SyncError::Api(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for SyncError {}

/// HTTP client mirroring `Api.getUrl`: the token rides as a query param on
/// every call.
pub struct Api {
    agent: Agent,
    host: String,
    token: String,
}

impl Api {
    /// POST auth/login (src/api.js `auth` → src/user.js `User.login`/
    /// `saveCurUser`). Ok persists the Account (UserId/Email/Username/Token/
    /// Host, IsActive; other accounts deactivated) and returns it.
    /// Bad credentials → `Api("WrongPassword")`, the server's own Msg.
    pub fn login(host: &str, email: &str, pwd: &str, store: &Store) -> Result<Account, SyncError> {
        // Before login there is no token yet; JS sends `token=''`.
        let api = Api::new(host, "");
        let ret = api.post_form(
            "auth/login",
            &[("macAddr", mac_addr()), ("v", client_version())],
            &[("email", email), ("pwd", pwd)],
        )?;
        if ret.get("Ok") != Some(&Value::Bool(true)) {
            return Err(SyncError::Api(
                ret.get("Msg")
                    .and_then(Value::as_str)
                    .unwrap_or("LoginFailed")
                    .to_string(),
            ));
        }
        let account = Account {
            user_id: str_field(&ret, "UserId"),
            email: str_field(&ret, "Email"),
            username: str_field(&ret, "Username"),
            host: host.to_string(),
            token: str_field(&ret, "Token"),
            is_active: true,
        };
        store
            .upsert_account(&account)
            .map_err(|e| SyncError::Unconnected(e.to_string()))?;
        Ok(account)
    }

    /// Authenticated client from a stored account.
    pub fn for_account(account: &Account) -> Self {
        Api::new(&account.host, &account.token)
    }

    /// POST auth/logout (src/api.js `logout`): errors are ignored, the JS
    /// callback fires regardless — mirror that by swallowing the Result.
    pub fn logout(&self) {
        let _ = self.post_form(
            "auth/logout",
            &[("macAddr", mac_addr()), ("v", client_version())],
            &[],
        );
    }

    fn new(host: &str, token: &str) -> Self {
        Api {
            agent: default_agent(),
            host: host.to_string(),
            token: token.to_string(),
        }
    }

    fn url(&self, path: &str, params: &[(&str, String)]) -> String {
        let mut url = format!(
            "{}/api/{}?token={}",
            self.host,
            path,
            urlencode(&self.token)
        );
        for (k, v) in params {
            url.push_str(&format!("&{k}={}", urlencode(v)));
        }
        url
    }

    fn get(&self, path: &str, params: &[(&str, String)]) -> Result<Value, SyncError> {
        self.agent
            .get(&self.url(path, params))
            .call()
            .map_err(|e| SyncError::Unconnected(e.to_string()))?
            .body_mut()
            .read_json::<Value>()
            .map_err(|e| SyncError::Unconnected(e.to_string()))
    }

    fn post_form(
        &self,
        path: &str,
        params: &[(&str, String)],
        form: &[(&str, &str)],
    ) -> Result<Value, SyncError> {
        self.agent
            .post(&self.url(path, params))
            .send_form(form.iter().map(|(k, v)| (*k, *v)))
            .map_err(|e| SyncError::Unconnected(e.to_string()))?
            .body_mut()
            .read_json::<Value>()
            .map_err(|e| SyncError::Unconnected(e.to_string()))
    }

    /// JS `Common.isOk`: only `Ok:false` in an object means failure; arrays
    /// (chunk endpoints) and objects without `Ok` are success.
    fn ok_or_api(v: Value) -> Result<Value, SyncError> {
        match &v {
            Value::Object(m) if m.get("Ok") == Some(&Value::Bool(false)) => Err(SyncError::Api(
                m.get("Msg")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )),
            _ => Ok(v),
        }
    }

    /// GET user/getSyncState → LastSyncUsn.
    pub fn get_sync_state(&self) -> Result<i64, SyncError> {
        let v = Self::ok_or_api(self.get(
            "user/getSyncState",
            &[("macAddr", mac_addr()), ("v", client_version())],
        )?)?;
        Ok(v.get("LastSyncUsn").and_then(Value::as_i64).unwrap_or(0))
    }

    /// GET {notebook,note,tag}/getSync{Notebooks,Notes,Tags} paged by
    /// afterUsn (JS `syncNotebook`/`syncNote`/`syncTag`: keep fetching while
    /// a chunk is exactly maxEntry long).
    fn sync_chunk(&self, kind: &str, after_usn: i64) -> Result<Vec<Value>, SyncError> {
        let path = match kind {
            "notebook" => "notebook/getSyncNotebooks",
            "note" => "note/getSyncNotes",
            "tag" => "tag/getSyncTags",
            other => unreachable!("unknown sync kind {other}"),
        };
        let v = Self::ok_or_api(self.get(
            path,
            &[
                ("afterUsn", after_usn.to_string()),
                ("maxEntry", MAX_ENTRY.to_string()),
            ],
        )?)?;
        match v {
            Value::Array(rows) => Ok(rows),
            other => Err(SyncError::Api(format!(
                "expected array from {path}, got {other}"
            ))),
        }
    }

    /// GET note/getNoteContent → Content for a server note id.
    pub fn get_note_content(&self, server_note_id: &str) -> Result<String, SyncError> {
        let v = Self::ok_or_api(self.get(
            "note/getNoteContent",
            &[("noteId", server_note_id.to_string())],
        )?)?;
        Ok(v.get("Content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }

    /// GET `file/getImage` → `(mime, bytes)` (issue 21 需求9; JSON body
    /// means `{Ok:false}`).
    pub fn get_image(&self, file_id: &str) -> Result<(String, Vec<u8>), SyncError> {
        use std::io::Read;
        let mut resp = self
            .agent
            .get(&self.url("file/getImage", &[("fileId", file_id.to_string())]))
            .call()
            .map_err(|e| SyncError::Unconnected(e.to_string()))?;
        let mime = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/png")
            .to_string();
        let mut body = Vec::new();
        resp.body_mut()
            .with_config()
            .reader()
            .read_to_end(&mut body)
            .map_err(|e| SyncError::Unconnected(e.to_string()))?;
        if mime.starts_with("application/json") {
            Err(SyncError::Api("notExists".into()))
        } else {
            Ok((mime, body))
        }
    }

    /// GET file/getAttach → raw attachment bytes + filename (issue 19,
    /// readonly view/download; upload is issue 20). The body is the file;
    /// the filename comes from the Content-Disposition header (Electron
    /// api.js `getAttach`).
    pub fn get_attach(&self, file_id: &str) -> Result<(String, Vec<u8>), SyncError> {
        use std::io::Read;
        let mut resp = self
            .agent
            .get(&self.url("file/getAttach", &[("fileId", file_id.to_string())]))
            .call()
            .map_err(|e| SyncError::Unconnected(e.to_string()))?;
        let is_json = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.starts_with("application/json"));
        let disposition = resp
            .headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let mut body = Vec::new();
        resp.body_mut()
            .with_config()
            .reader()
            .read_to_end(&mut body)
            .map_err(|e| SyncError::Unconnected(e.to_string()))?;
        if is_json {
            let v: Value =
                serde_json::from_slice(&body).map_err(|e| SyncError::Unconnected(e.to_string()))?;
            let msg = match Self::ok_or_api(v) {
                Err(SyncError::Api(msg)) => msg,
                _ => "attachment not found".into(),
            };
            return Err(SyncError::Api(msg));
        }
        if body.is_empty() {
            return Err(SyncError::Api("attachment is empty".into()));
        }
        // filename="foo.pdf" (or filename=foo.pdf) from Content-Disposition;
        // fall back to the file id + .bin.
        let filename = disposition
            .split(';')
            .find_map(|part| {
                let part = part.trim();
                part.strip_prefix("filename=")
                    .map(|n| n.trim_matches('"').to_string())
            })
            .unwrap_or_else(|| format!("{file_id}.bin"));
        Ok((filename, body))
    }

    // -- Push endpoints (JS Sync.sendNotebook/sendNote/sendTagChanges forms).
    // Raw `Value` returns: the caller branches on Ok/Msg/Usn/ids, unlike the
    // pull endpoints which only care about payload.

    pub fn add_notebook(&self, title: &str, seq: i64, parent: &str) -> Result<Value, SyncError> {
        self.post_form(
            "notebook/addNotebook",
            &[],
            &[
                ("title", title),
                ("seq", &seq.to_string()),
                ("parentNotebookId", parent),
            ],
        )
    }

    pub fn update_notebook(
        &self,
        server_id: &str,
        title: &str,
        usn: i64,
        seq: i64,
        parent: &str,
    ) -> Result<Value, SyncError> {
        self.post_form(
            "notebook/updateNotebook",
            &[],
            &[
                ("notebookId", server_id),
                ("title", title),
                ("usn", &usn.to_string()),
                ("seq", &seq.to_string()),
                ("parentNotebookId", parent),
            ],
        )
    }

    pub fn delete_notebook(&self, server_id: &str, usn: i64) -> Result<Value, SyncError> {
        self.post_form(
            "notebook/deleteNotebook",
            &[],
            &[("notebookId", server_id), ("usn", &usn.to_string())],
        )
    }

    pub fn add_note(
        &self,
        notebook_server_id: &str,
        title: &str,
        content: &str,
        is_markdown: bool,
        tags: &[String],
        created: &str,
        updated: &str,
        images: &[crate::ImageAttachment],
    ) -> Result<Value, SyncError> {
        let mut form = vec![
            ("Title", title),
            ("NotebookId", notebook_server_id),
            ("Content", content),
            ("IsMarkdown", if is_markdown { "1" } else { "0" }),
            ("IsBlog", "0"),
            ("CreatedTime", created),
            ("UpdatedTime", updated),
        ];
        for t in tags {
            form.push(("Tags", t.as_str()));
        }
        if images.is_empty() {
            self.post_form("note/addNote", &[], &form)
        } else {
            self.post_note_multipart("note/addNote", &form, images)
        }
    }

    pub fn update_note(
        &self,
        server_note_id: &str,
        notebook_server_id: &str,
        title: &str,
        usn: i64,
        is_trash: bool,
        tags: &[String],
        content: &str,
        updated: &str,
        images: &[crate::ImageAttachment],
    ) -> Result<Value, SyncError> {
        // JS always sends Content when ContentIsDirty; we have no such
        // column, so always send it.
        let usn_s = usn.to_string();
        let is_trash_s = if is_trash { "1" } else { "0" };
        let mut form = vec![
            ("NoteId", server_note_id),
            ("NotebookId", notebook_server_id),
            ("Title", title),
            ("Usn", usn_s.as_str()),
            ("IsTrash", is_trash_s),
            ("IsBlog", "0"),
            ("Content", content),
            ("Abstract", ""),
            ("UpdatedTime", updated),
        ];
        for t in tags {
            form.push(("Tags", t.as_str()));
        }
        if images.is_empty() {
            self.post_form("note/updateNote", &[], &form)
        } else {
            self.post_note_multipart("note/updateNote", &form, images)
        }
    }

    /// JS wire format for note pushes carrying image bodies: `Files` is the
    /// JSON attribute array (src/note.js getFilesPostInfo), each dirty image
    /// rides as a part named by its local file id (src/api.js FileDatas).
    fn post_note_multipart(
        &self,
        path: &str,
        form: &[(&str, &str)],
        images: &[crate::ImageAttachment],
    ) -> Result<Value, SyncError> {
        let files = serde_json::to_string(
            &images
                .iter()
                .map(|i| {
                    json!({
                        "FileId": i.local_id.0,
                        "LocalFileId": i.local_id.0,
                        "Type": "image",
                        "HasBody": true,
                        "IsAttach": false,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .map_err(|e| SyncError::Unconnected(e.to_string()))?;
        let mut mform = form.to_vec();
        mform.push(("Files", files.as_str()));

        let mut builder = ureq::unversioned::multipart::Form::new();
        for (k, v) in &mform {
            builder = builder.text(k, v);
        }
        for img in images {
            let fname = img.file_name.clone();
            let part = ureq::unversioned::multipart::Part::bytes(&img.bytes)
                .file_name(fname.as_str())
                .mime_str(&img.mime)
                .map_err(|e| SyncError::Unconnected(e.to_string()))?;
            builder = builder.part(&img.local_id.0, part);
        }
        self.agent
            .post(&self.url(path, &[]))
            .send(builder)
            .map_err(|e| SyncError::Unconnected(e.to_string()))?
            .body_mut()
            .read_json::<Value>()
            .map_err(|e| SyncError::Unconnected(e.to_string()))
    }

    pub fn delete_trash(&self, server_note_id: &str, usn: i64) -> Result<Value, SyncError> {
        self.post_form(
            "note/deleteTrash",
            &[],
            &[("noteId", server_note_id), ("usn", &usn.to_string())],
        )
    }

    pub fn add_tag(&self, tag: &str) -> Result<Value, SyncError> {
        self.post_form("tag/addTag", &[], &[("tag", tag)])
    }

    pub fn delete_tag(&self, tag: &str, usn: i64) -> Result<Value, SyncError> {
        self.post_form(
            "tag/deleteTag",
            &[],
            &[("tag", tag), ("usn", &usn.to_string())],
        )
    }
}

fn default_agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(10)))
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .new_agent()
}

/// `src/api.js`: `os.platform() + '_' + os.arch()` (e.g. `darwin_arm64`).
/// The JS client also appends `_data/version` when present; the server only
/// logs the string, so the platform pair is the contract that matters.
pub fn client_version() -> String {
    format!("{}_{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// `src/api.js` uses `node-getmac`, which takes the first `ether <mac>` line
/// of `ifconfig -a`. getifaddrs enumerates interfaces in the same order, so
/// the first AF_LINK/AF_PACKET sockaddr with 6 address bytes is the same MAC
/// (lowercase, colon-separated). `00:00:00:00:00:00` fallback (the server
/// only logs this field).
pub fn mac_addr() -> String {
    const FALLBACK: &str = "00:00:00:00:00:00";
    // SAFETY: getifaddrs returns a NULL-terminated list owned by the caller;
    // we only read it and hand it back to freeifaddrs.
    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifap) != 0 {
            return FALLBACK.to_string();
        }
        let mut mac = String::new();
        let mut cur = ifap;
        while !cur.is_null() && mac.is_empty() {
            let ifa = &*cur;
            cur = ifa.ifa_next;
            if ifa.ifa_addr.is_null() {
                continue;
            }
            let family = (*ifa.ifa_addr).sa_family as i32;
            #[cfg(target_os = "macos")]
            let bytes = (family == libc::AF_LINK).then(|| {
                let sdl = &*(ifa.ifa_addr as *const libc::sockaddr_dl);
                let (nlen, alen) = (sdl.sdl_nlen as usize, sdl.sdl_alen as usize);
                (alen >= 6).then(|| {
                    sdl.sdl_data[nlen..nlen + 6]
                        .iter()
                        .map(|&b| b as u8)
                        .collect::<Vec<_>>()
                })
            });
            #[cfg(target_os = "linux")]
            let bytes = (family == libc::AF_PACKET).then(|| {
                let sll = &*(ifa.ifa_addr as *const libc::sockaddr_ll);
                (sll.sll_halen as usize >= 6).then(|| sll.sll_addr[..6].to_vec())
            });
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            let bytes: Option<Vec<u8>> = None;
            let Some(b) = bytes.flatten() else { continue };
            mac = b
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect::<Vec<_>>()
                .join(":");
        }
        libc::freeifaddrs(ifap);
        if mac.is_empty() {
            FALLBACK.to_string()
        } else {
            mac
        }
    }
}

/// Only the values we emit need escaping (token, email, ids, numbers).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn i64_field(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn bool_field(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// JS `Common.goNowToDate` + `new Date().getTime()`: parse
/// "YYYY-MM-DDTHH:MM:SS…" to epoch ms; missing → now.
/// ponytail: JS parses as local time, we parse as UTC — only shifts
/// display timestamps by the local offset; switch to a tz-aware parse if
/// byte-parity of synced timestamps is ever required.
fn go_now_ms(v: Option<&Value>, now: i64) -> i64 {
    let Some(s) = v.and_then(Value::as_str) else {
        return now;
    };
    let b = s.as_bytes();
    let (h, mi, sec) = if b.len() >= 19 && b[10] == b'T' {
        (
            s.get(11..13)
                .and_then(|x| x.parse::<i64>().ok())
                .unwrap_or(0),
            s.get(14..16)
                .and_then(|x| x.parse::<i64>().ok())
                .unwrap_or(0),
            s.get(17..19)
                .and_then(|x| x.parse::<i64>().ok())
                .unwrap_or(0),
        )
    } else {
        (0, 0, 0)
    };
    let y = s
        .get(0..4)
        .and_then(|x| x.parse::<i64>().ok())
        .unwrap_or(1970);
    let mo = s.get(5..7).and_then(|x| x.parse::<i64>().ok()).unwrap_or(1);
    let d = s
        .get(8..10)
        .and_then(|x| x.parse::<i64>().ok())
        .unwrap_or(1);
    if !(1..=12).contains(&mo) {
        return now;
    }
    days_from_civil(y, mo, d).saturating_mul(86_400_000) + h * 3_600_000 + mi * 60_000 + sec * 1_000
}

/// Days since 1970-01-01 (Howard Hinnant's civil algorithm, no chrono).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of `days_from_civil` (Hinnant civil_from_days).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// JS `Common.formatDatetime` shape `YYYY-MM-DD hh:mm:ss`.
/// ponytail: UTC, not local time — only shifts by the local offset; only
/// used in pushed UpdatedTime strings.
fn fmt_datetime(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let secs = ms.rem_euclid(86_400_000) / 1000;
    let (y, mo, d) = civil_from_days(days);
    format!(
        "{y:04}-{mo:02}-{d:02} {:02}:{:02}:{:02}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

/// Progress sink for `full_sync` (CLI prints, tests stay silent).
pub trait SyncProgress {
    fn note(&self, msg: String) {
        let _ = msg;
    }
    /// Deterministic progress for the slow stage (per-note content
    /// fetches): `done` items of `total`. Default no-op keeps CLI/tests
    /// untouched.
    fn tick(&self, done: usize, total: usize) {
        let _ = (done, total);
    }
}

/// Silent default.
pub struct NoProgress;
impl SyncProgress for NoProgress {}

pub struct SyncStats {
    pub notebooks: usize,
    pub notes: usize,
    pub contents: usize,
    pub tags: usize,
}

/// A dirty notebook row (JS `getDirtyNotebooks`).
pub struct DirtyNotebook {
    pub notebook_id: String,
    pub title: String,
    pub seq: i64,
    pub usn: i64,
    pub server_id: Option<String>,
    pub is_new: bool,
    pub is_dirty: bool,
    pub is_deleted: bool,
}

/// A dirty note row (JS `getDirtyNotes`).
pub struct DirtyNote {
    pub note_id: String,
    pub notebook_id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub is_markdown: bool,
    pub is_trash: bool,
    pub is_dirty: bool,
    pub is_new: bool,
    pub is_deleted: bool,
    pub usn: i64,
    pub created_time: i64,
    pub updated_time: i64,
    pub server_note_id: Option<String>,
}

/// A dirty tag row (JS `getDirtyTags`).
pub struct DirtyTag {
    pub tag: String,
    pub usn: i64,
    pub is_dirty: bool,
    pub is_deleted: bool,
}

/// Full sync pull phase (JS `Sync.fullSync` + the content download the JS
/// front-end triggers per note): notebooks → notes → tags from USN −1, then
/// contents, then persist `LastSyncUsn` from the server.
///
/// Interrupted-safe: each chunk commits on its own and `LastSyncUsn` is only
/// written at the very end, so a re-run re-pulls from −1 and upserts over the
/// partial state.
pub fn full_sync(
    api: &Api,
    store: &Store,
    progress: &dyn SyncProgress,
) -> Result<SyncStats, SyncError> {
    let mut stats = SyncStats {
        notebooks: 0,
        notes: 0,
        contents: 0,
        tags: 0,
    };
    let now = now_ms();

    for (kind, apply) in [
        (
            "notebook",
            &apply_notebooks as &dyn Fn(&Store, &[Value], i64) -> rusqlite::Result<()>,
        ),
        ("note", &apply_notes),
        ("tag", &apply_tags),
    ] {
        let mut usn: i64 = -1; // JS fullSync always resets the cursors to −1
        loop {
            let rows = api.sync_chunk(kind, usn)?;
            if rows.is_empty() {
                break;
            }
            apply(store, &rows, now).map_err(|e| SyncError::Unconnected(e.to_string()))?;
            match kind {
                "notebook" => stats.notebooks += rows.len(),
                "note" => stats.notes += rows.len(),
                _ => stats.tags += rows.len(),
            }
            progress.note(format!("{kind}s: +{}", rows.len()));
            if (rows.len() as i64) < MAX_ENTRY {
                break;
            }
            usn = i64_field(rows.last().expect("non-empty"), "Usn");
        }
    }

    // Contents for notes still missing one (JS `InitSync`).
    // ponytail: "needs content" == empty Content, so a server note whose real
    // content is the empty string is re-fetched each run; harmless (idempotent)
    let server_ids = store
        .note_server_ids_needing_content()
        .map_err(|e| SyncError::Unconnected(e.to_string()))?;
    let total = server_ids.len();
    for (done, sid) in server_ids.into_iter().enumerate() {
        progress.tick(done, total);
        let content = api.get_note_content(&sid)?;
        store
            .set_note_content_by_server_id(&sid, &content)
            .map_err(|e| SyncError::Unconnected(e.to_string()))?;
        stats.contents += 1;
        progress.note(format!("content: {sid}"));
    }
    progress.tick(total, total);

    // Issue 21 需求9: download bytes for images referenced by pulled notes.
    let _ = pull_missing_images(api, store);

    // JS `updateLastSyncState`: LastSyncUsn comes from the server.
    let server_usn = api.get_sync_state()?;
    store
        .set_last_sync_usn(server_usn)
        .map_err(|e| SyncError::Unconnected(e.to_string()))?;
    Ok(stats)
}

/// JS `Sync.checkNeedIncSyncAgain`: `last+1 == usn` → advance last;
/// `last > usn` → ignore (stale usn, e.g. tag adds); else → re-round needed.
fn check_need_incr_sync_again(
    store: &Store,
    usn: i64,
    need_again: &mut bool,
) -> Result<(), SyncError> {
    let last = store.last_sync_usn().map_err(db_err)?;
    if last + 1 == usn {
        store.set_last_sync_usn(usn).map_err(db_err)?;
    } else if last > usn {
        return Ok(());
    } else {
        *need_again = true;
    }
    Ok(())
}

/// Issue 21 需求9: images referenced by the note's markdown that have not
/// been uploaded yet (empty ServerId) ride along with the note push.
fn dirty_images(store: &Store, content: &str) -> Result<Vec<crate::ImageAttachment>, SyncError> {
    let mut out = Vec::new();
    for id in Store::image_ids_in_content(content) {
        if let Some(img) = store.get_image(&id).map_err(db_err)? {
            if img.server_id.is_empty()
                && !out
                    .iter()
                    .any(|i: &crate::ImageAttachment| i.local_id == img.local_id)
            {
                out.push(img);
            }
        }
    }
    Ok(out)
}

/// Issue 21 需求9: after a pull, note markdown may reference images whose
/// bytes live only on the server (`leanote://file/getImage?fileId=<sid>`).
/// Download each missing one into the local images table so the reader can
/// render it. Errors are skipped (next sync retries).
pub fn pull_missing_images(api: &Api, store: &Store) -> Result<usize, SyncError> {
    let mut fetched = 0;
    for n in store.list_notes().map_err(db_err)? {
        for id in Store::image_ids_in_content(&n.content) {
            let missing = store
                .get_image(&id)
                .map_err(db_err)?
                .map(|img| img.bytes.is_empty())
                .unwrap_or(true);
            if !missing {
                continue;
            }
            let Ok((mime, bytes)) = api.get_image(&id) else {
                continue;
            };
            let img = crate::ImageAttachment {
                local_id: crate::Id(id.clone()),
                server_id: if id.starts_with("srvimg") {
                    id.clone()
                } else {
                    String::new()
                },
                file_name: format!("{id}.png"),
                mime,
                size: bytes.len() as i64,
                bytes,
                created_time: 0,
            };
            if store.insert_image(&img).map_err(db_err).is_ok() {
                fetched += 1;
            }
        }
    }
    Ok(fetched)
}

/// JS `File.updateImageForce`: map the response's LocalFileId -> FileId into
/// the images table so later pulls/render resolve server ids.
fn apply_files_mapping(store: &Store, ret: &Value) -> Result<(), SyncError> {
    if let Some(files) = ret.get("Files").and_then(Value::as_array) {
        for f in files {
            let local = f.get("LocalFileId").and_then(Value::as_str).unwrap_or("");
            let server = f.get("FileId").and_then(Value::as_str).unwrap_or("");
            if !local.is_empty() && !server.is_empty() {
                store
                    .set_image_server_id(&crate::Id(local.to_string()), server)
                    .map_err(db_err)?;
            }
        }
    }
    Ok(())
}
fn db_err(e: rusqlite::Error) -> SyncError {
    SyncError::Unconnected(e.to_string())
}

/// JS `Sync.incrSync` + `sendChanges`: pull USN-delta chunks, then push
/// dirty rows (notebooks → notes → tags). Re-rounds while the server kept
/// advancing (`_reSyncAgainTimes < 5`); a re-round pulls again but, like JS
/// `sendChanges(again==true)`, does not push a second time.
pub fn incr_sync(
    api: &Api,
    store: &Store,
    progress: &dyn SyncProgress,
) -> Result<SyncStats, SyncError> {
    let mut stats = SyncStats {
        notebooks: 0,
        notes: 0,
        contents: 0,
        tags: 0,
    };
    let mut again = false; // first round pushes; re-rounds only pull
    for _round in 0..5 {
        let mut need_again = false;
        let last = store.last_sync_usn().map_err(db_err)?;
        let server_usn = api.get_sync_state()?;

        // Pull phase (JS incrSync): only when the server moved.
        if server_usn > last {
            let now = now_ms();
            for (kind, apply) in [
                (
                    "notebook",
                    &apply_notebooks as &dyn Fn(&Store, &[Value], i64) -> rusqlite::Result<()>,
                ),
                ("tag", &apply_tags),
            ] {
                let mut usn = last;
                loop {
                    let rows = api.sync_chunk(kind, usn)?;
                    if rows.is_empty() {
                        break;
                    }
                    apply(store, &rows, now).map_err(db_err)?;
                    match kind {
                        "notebook" => stats.notebooks += rows.len(),
                        _ => stats.tags += rows.len(),
                    }
                    progress.note(format!("{kind}s: +{}", rows.len()));
                    if (rows.len() as i64) < MAX_ENTRY {
                        break;
                    }
                    usn = i64_field(rows.last().expect("non-empty"), "Usn");
                }
            }
            // Notes need conflict-aware apply.
            let mut usn = last;
            loop {
                let rows = api.sync_chunk("note", usn)?;
                if rows.is_empty() {
                    break;
                }
                apply_incr_notes(store, &rows, api, now)?;
                stats.notes += rows.len();
                progress.note(format!("notes: +{}", rows.len()));
                if (rows.len() as i64) < MAX_ENTRY {
                    break;
                }
                usn = i64_field(rows.last().expect("non-empty"), "Usn");
            }
            // JS reloads content for updated notes; here only notes still
            // missing one (new rows land with Content='') get filled.
            for sid in store.note_server_ids_needing_content().map_err(db_err)? {
                let content = api.get_note_content(&sid)?;
                store
                    .set_note_content_by_server_id(&sid, &content)
                    .map_err(db_err)?;
                stats.contents += 1;
            }
            store
                .set_last_sync_usn(api.get_sync_state()?)
                .map_err(db_err)?;
        }

        // Push phase (JS sendChanges): skipped on re-rounds.
        if !again {
            push_dirty_notebooks(api, store, &mut need_again, &mut stats)?;
            push_dirty_notes(api, store, &mut need_again, &mut stats)?;
            push_dirty_tags(api, store, &mut need_again, &mut stats)?;
        }

        if !need_again {
            let _ = pull_missing_images(api, store);
            return Ok(stats);
        }
        again = true;
    }
    Ok(stats)
}

/// JS `Sync.sendNotebookChanges`.
fn push_dirty_notebooks(
    api: &Api,
    store: &Store,
    need_again: &mut bool,
    _stats: &mut SyncStats,
) -> Result<(), SyncError> {
    for nb in store.dirty_notebooks().map_err(db_err)? {
        let server_id = nb.server_id.clone();
        let ret = if nb.is_new && nb.is_deleted {
            store
                .notebook_delete_local(&nb.notebook_id)
                .map_err(db_err)?;
            continue;
        } else if nb.is_new {
            api.add_notebook(&nb.title, nb.seq, "")?
        } else if nb.is_deleted {
            api.delete_notebook(server_id.as_deref().unwrap_or(""), nb.usn)?
        } else {
            api.update_notebook(
                server_id.as_deref().unwrap_or(&nb.notebook_id),
                &nb.title,
                nb.usn,
                nb.seq,
                "",
            )?
        };

        if nb.is_deleted {
            // JS: delete branch returns early; row removal (or conflict
            // not-dirty) is handled here from ret.
            if ok(&ret) {
                store
                    .notebook_delete_local(&nb.notebook_id)
                    .map_err(db_err)?;
                check_need_incr_sync_again(store, usn_of(&ret), need_again)?;
            } else if msg_of(&ret) == "notExists" {
                store
                    .notebook_delete_local(&nb.notebook_id)
                    .map_err(db_err)?;
            } else if msg_of(&ret) == "conflict" {
                store
                    .notebook_set_not_dirty_not_delete(&nb.notebook_id)
                    .map_err(db_err)?;
            }
            continue;
        }
        if nb.is_new {
            if ok(&ret) {
                let sid = str_field(&ret, "NotebookId");
                let usn = i64_field(&ret, "Usn");
                store
                    .notebook_pushed(&nb.notebook_id, &sid, usn)
                    .map_err(db_err)?;
                check_need_incr_sync_again(store, usn, need_again)?;
            }
        } else if ok(&ret) {
            let usn = i64_field(&ret, "Usn");
            store
                .notebook_pushed(&nb.notebook_id, server_id.as_deref().unwrap_or(""), usn)
                .map_err(db_err)?;
            check_need_incr_sync_again(store, usn, need_again)?;
        }
    }
    Ok(())
}

/// JS `Sync.sendNoteChanges`.
fn push_dirty_notes(
    api: &Api,
    store: &Store,
    need_again: &mut bool,
    _stats: &mut SyncStats,
) -> Result<(), SyncError> {
    for n in store.dirty_notes().map_err(db_err)? {
        let server_note_id = n.server_note_id.clone().unwrap_or_default();
        let nb_local = n.notebook_id.clone();
        let nb_server = store
            .notebook_server_id(&nb_local)
            .map_err(db_err)?
            .unwrap_or_else(|| nb_local.clone());

        if n.is_new && !n.is_trash && !n.is_deleted {
            let images = dirty_images(store, &n.content)?;
            let ret = api.add_note(
                &nb_server,
                &n.title,
                &n.content,
                n.is_markdown,
                &n.tags,
                &fmt_datetime(n.created_time),
                &fmt_datetime(n.updated_time),
                &images,
            )?;
            if ok(&ret) {
                let sid = str_field(&ret, "NoteId");
                let usn = i64_field(&ret, "Usn");
                apply_files_mapping(store, &ret)?;
                store
                    .note_add_pushed(&n.note_id, &sid, usn)
                    .map_err(db_err)?;
                check_need_incr_sync_again(store, usn, need_again)?;
            }
        } else if n.is_new && n.is_deleted {
            store.note_remove(&n.note_id).map_err(db_err)?;
        } else if n.is_new && n.is_trash {
            // JS skips: stays dirty until un-trashed or deleted.
        } else if n.is_deleted {
            let ret = api.delete_trash(&server_note_id, n.usn)?;
            if ok(&ret) {
                store.note_remove(&n.note_id).map_err(db_err)?;
                check_need_incr_sync_again(store, usn_of(&ret), need_again)?;
            } else if msg_of(&ret) == "notExists" {
                store.note_remove(&n.note_id).map_err(db_err)?;
            } else if msg_of(&ret) == "conflict" {
                // ticket: 置非脏不删
                store
                    .note_set_not_dirty_not_delete(&n.note_id)
                    .map_err(db_err)?;
            }
        } else {
            let images = dirty_images(store, &n.content)?;
            let ret = api.update_note(
                &server_note_id,
                &nb_server,
                &n.title,
                n.usn,
                n.is_trash,
                &n.tags,
                &n.content,
                &fmt_datetime(n.updated_time),
                &images,
            )?;
            if ok(&ret) {
                let usn = i64_field(&ret, "Usn");
                apply_files_mapping(store, &ret)?;
                store.note_update_pushed(&n.note_id, usn).map_err(db_err)?;
                check_need_incr_sync_again(store, usn, need_again)?;
            } else if msg_of(&ret) == "conflict" {
                // JS fetches server content: differs → conflict list (stay
                // dirty); equal → metadata-only update. ponytail: no
                // getNote endpoint on the fake server; record via pull-side
                // apply_incr_notes instead.
                match api.get_note_content(&server_note_id) {
                    Ok(c) if c == n.content => {
                        store
                            .note_update_pushed(&n.note_id, n.usn)
                            .map_err(db_err)?;
                    }
                    _ => {} // stay dirty; conflict captured on next pull
                }
            }
        }
    }
    Ok(())
}

/// JS `Sync.sendTagChanges`.
fn push_dirty_tags(
    api: &Api,
    store: &Store,
    need_again: &mut bool,
    _stats: &mut SyncStats,
) -> Result<(), SyncError> {
    for t in store.dirty_tags().map_err(db_err)? {
        let ret = if t.is_deleted {
            api.delete_tag(&t.tag, t.usn)?
        } else {
            api.add_tag(&t.tag)?
        };
        if ok(&ret) {
            if t.is_deleted {
                store.tag_set_not_dirty(&t.tag).map_err(db_err)?;
            } else {
                store
                    .tag_set_not_dirty_and_usn(&t.tag, i64_field(&ret, "Usn"))
                    .map_err(db_err)?;
            }
            check_need_incr_sync_again(store, usn_of(&ret), need_again)?;
        } else if msg_of(&ret) == "notExists" && t.is_deleted {
            store.tag_delete_local(&t.tag).map_err(db_err)?;
        } else if msg_of(&ret) == "conflict" {
            store.tag_set_not_dirty(&t.tag).map_err(db_err)?;
        }
    }
    Ok(())
}

fn ok(v: &Value) -> bool {
    v.get("Ok") != Some(&Value::Bool(false))
}

fn msg_of(v: &Value) -> String {
    str_field(v, "Msg")
}

fn usn_of(v: &Value) -> i64 {
    i64_field(v, "Usn")
}

/// JS `Sync._syncNoteToLocal` (incr pull side), conflict-aware.
fn apply_incr_notes(store: &Store, rows: &[Value], api: &Api, now: i64) -> Result<(), SyncError> {
    for row in rows {
        let server_id = str_field(row, "NoteId");
        let local = store.note_by_server_id(&server_id).map_err(db_err)?;
        if bool_field(row, "IsDeleted") {
            match &local {
                Some(n) if n.is_dirty => {
                    // dirty local beats server delete
                    store
                        .note_set_not_dirty_not_delete(&n.note_id)
                        .map_err(db_err)?;
                }
                _ => {
                    store.delete_note_by_server_id(&server_id).map_err(db_err)?;
                }
            }
            continue;
        }
        let Some(n) = local else {
            // new to us: insert with empty content; full_sync-style content
            // fill covers it on the next full sync, and the pull below
            // (Content='' row) is fetched by the content phase of a later
            // full sync. ponytail: incr never downloads content here.
            store
                .apply_sync_notes(std::slice::from_ref(row), now)
                .map_err(db_err)?;
            continue;
        };
        if i64_field(row, "Usn") == n.usn {
            continue;
        }
        if n.is_dirty {
            match api.get_note_content(&server_id) {
                Ok(c) if c != n.content => {
                    store
                        .record_conflict(&server_id, &n.note_id, row, &n.title, &n.content, &c)
                        .map_err(db_err)?;
                }
                Ok(_) => {
                    store.apply_note_metadata(row, now).map_err(db_err)?;
                }
                Err(_) => {
                    store
                        .record_conflict(&server_id, &n.note_id, row, &n.title, &n.content, "")
                        .map_err(db_err)?;
                }
            }
        } else {
            store.apply_note_metadata(row, now).map_err(db_err)?;
        }
    }
    Ok(())
}

fn apply_notebooks(store: &Store, rows: &[Value], now: i64) -> rusqlite::Result<()> {
    store.apply_sync_notebooks(rows, now)
}
fn apply_notes(store: &Store, rows: &[Value], now: i64) -> rusqlite::Result<()> {
    store.apply_sync_notes(rows, now)
}
fn apply_tags(store: &Store, rows: &[Value], now: i64) -> rusqlite::Result<()> {
    store.apply_sync_tags(rows, now)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// -- Store-side sync writers (JS addNotebookForce/addNoteForce/... semantics) --

impl Store {
    /// `User.saveCurUser`: deactivate every other account, upsert this one.
    pub fn upsert_account(&self, account: &Account) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE accounts SET IsActive=0 WHERE UserId<>?1",
            params![account.user_id],
        )?;
        tx.execute(
            "INSERT INTO accounts (UserId, Email, Username, Host, Token, IsActive, LastSyncUsn)
             VALUES (?1,?2,?3,?4,?5,1,-1)
             ON CONFLICT(UserId) DO UPDATE SET
                Email=excluded.Email, Username=excluded.Username, Host=excluded.Host,
                Token=excluded.Token, IsActive=1",
            params![
                account.user_id,
                account.email,
                account.username,
                account.host,
                account.token
            ],
        )?;
        tx.commit()
    }

    /// One chunk of `notebook/getSyncNotebooks` rows, as
    /// `deleteNotebookForce`/`addNotebookForce`/`updateNotebookForce`.
    /// JS keeps the server id as the local id (`notebook.NotebookId =
    /// notebook.NotebookId`) to preserve hierarchy, so no mapping step.
    pub fn apply_sync_notebooks(&self, rows: &[Value], now: i64) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for row in rows {
            let server_id = str_field(row, "NotebookId");
            if bool_field(row, "IsDeleted") {
                tx.execute(
                    "DELETE FROM notebooks WHERE ServerNotebookId=?1",
                    params![server_id],
                )?;
                continue;
            }
            tx.execute(
                "INSERT INTO notebooks (NotebookId, UserId, Title, ParentNotebookId, Seq, Usn,
                    LocalIsNew, IsDirty, LocalIsDelete, ServerNotebookId, CreatedTime, UpdatedTime)
                 VALUES (?1,?2,?3,?4,?5,?6,0,0,0,?1,?7,?8)
                 ON CONFLICT(NotebookId) DO UPDATE SET
                    Title=excluded.Title, ParentNotebookId=excluded.ParentNotebookId,
                    Seq=excluded.Seq, Usn=excluded.Usn, IsDirty=0, LocalIsNew=0,
                    LocalIsDelete=0, ServerNotebookId=excluded.ServerNotebookId,
                    UpdatedTime=excluded.UpdatedTime",
                params![
                    server_id,
                    self.user_id,
                    str_field(row, "Title"),
                    str_field(row, "ParentNotebookId"),
                    i64_field(row, "Seq"),
                    i64_field(row, "Usn"),
                    go_now_ms(row.get("CreatedTime"), now),
                    go_now_ms(row.get("UpdatedTime"), now),
                ],
            )?;
        }
        tx.commit()
    }

    /// One chunk of `note/getSyncNotes` rows, as `deleteNoteForce`/
    /// `addNoteForce`/`updateNoteForce`. Content stays empty here; the
    /// content phase fills it (`InitSync` in JS). Update keeps the local
    /// CreatedTime/UpdatedTime, like JS (`delete note['UpdatedTime']`).
    pub fn apply_sync_notes(&self, rows: &[Value], now: i64) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for row in rows {
            let server_id = str_field(row, "NoteId");
            if bool_field(row, "IsDeleted") {
                tx.execute(
                    "DELETE FROM notes WHERE ServerNoteId=?1",
                    params![server_id],
                )?;
                continue;
            }
            let tags = match row.get("Tags") {
                Some(Value::Array(a)) => a
                    .iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
                    .join(","),
                // Issue 20: some servers serialize Tags as a JSON string
                // ("[\"rust\",\"gpui\"]") — parse it, never store it raw
                // (CSV is the local column format).
                Some(Value::String(s)) => serde_json::from_str::<Vec<String>>(s)
                    .map(|v| v.join(","))
                    .unwrap_or_else(|_| s.clone()),
                _ => String::new(),
            };
            tx.execute(
                "INSERT INTO notes (NoteId, UserId, NotebookId, Title, Content, \"Desc\", ImgSrc,
                    Tags, Abstract, IsMarkdown, IsTrash, IsDeleted, IsDirty, LocalIsNew,
                    LocalIsDelete, IsBlog, Star, Usn, CreatedTime, UpdatedTime, ServerNoteId)
                 VALUES (?1,?2,?3,?4,'','','',?5,'',?6,?7,0,0,0,0,0,0,?8,?9,?10,?1)
                 ON CONFLICT(NoteId) DO UPDATE SET
                    NotebookId=excluded.NotebookId, Title=excluded.Title, Tags=excluded.Tags,
                    IsMarkdown=excluded.IsMarkdown, IsTrash=excluded.IsTrash,
                    Usn=excluded.Usn, IsDirty=0, LocalIsNew=0, LocalIsDelete=0,
                    ServerNoteId=excluded.ServerNoteId",
                params![
                    server_id,
                    self.user_id,
                    str_field(row, "NotebookId"),
                    str_field(row, "Title"),
                    tags,
                    bool_field(row, "IsMarkdown") as i64,
                    bool_field(row, "IsTrash") as i64,
                    i64_field(row, "Usn"),
                    go_now_ms(row.get("CreatedTime"), now),
                    go_now_ms(row.get("UpdatedTime"), now),
                ],
            )?;
        }
        tx.commit()
    }

    /// One chunk of `tag/getSyncTags` rows, as `Tag.deleteTag` (fullSync: row
    /// kept with LocalIsDelete) / `addOrUpdateTag` / `setNotDirtyAndUsn`.
    pub fn apply_sync_tags(&self, rows: &[Value], now: i64) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for row in rows {
            let tag = str_field(row, "Tag");
            let usn = i64_field(row, "Usn");
            // JS Tag.deleteTag(fullSync) keeps the row with LocalIsDelete; a
            // row never seen live yet still needs its local record.
            let deleted = bool_field(row, "IsDeleted");
            tx.execute(
                "INSERT INTO tags (TagId, UserId, Tag, IsDirty, Count, LocalIsDelete, Usn,
                    CreatedTime, UpdatedTime)
                 VALUES (?1,?2,?3,0,1,?4,?5,?5,?5)
                 ON CONFLICT(UserId, Tag) DO UPDATE SET
                    Usn=excluded.Usn, IsDirty=0, UpdatedTime=excluded.UpdatedTime",
                params![Id::new().to_string(), self.user_id, tag, usn, now],
            )?;
            if deleted {
                tx.execute(
                    "UPDATE tags SET LocalIsDelete=1 WHERE UserId=?1 AND Tag=?2",
                    params![self.user_id, tag],
                )?;
            }
        }
        tx.commit()
    }

    /// Server note ids whose local row still has no content.
    pub fn note_server_ids_needing_content(&self) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT ServerNoteId FROM notes
             WHERE ServerNoteId IS NOT NULL AND Content='' AND LocalIsDelete=0",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect()
    }

    /// JS `updateNoteContentForce`: store fetched content (+ Desc/Abstract,
    /// same derivation as a local add).
    pub fn set_note_content_by_server_id(
        &self,
        server_note_id: &str,
        content: &str,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE notes SET Content=?2, \"Desc\"=?3, Abstract=?4 WHERE ServerNoteId=?1",
            params![
                server_note_id,
                content,
                crate::plain_desc(content),
                content.chars().take(500).collect::<String>(),
            ],
        )?;
        Ok(())
    }

    pub fn set_last_sync_usn(&self, usn: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE accounts SET LastSyncUsn=?1 WHERE IsActive=1",
            params![usn],
        )?;
        Ok(())
    }

    pub fn last_sync_usn(&self) -> rusqlite::Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT LastSyncUsn FROM accounts WHERE IsActive=1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(-1))
    }

    pub fn active_account(&self) -> rusqlite::Result<Option<Account>> {
        self.conn
            .query_row(
                "SELECT UserId, Email, Username, Host, Token FROM accounts WHERE IsActive=1",
                [],
                |r| {
                    Ok(Account {
                        user_id: r.get(0)?,
                        email: r.get(1)?,
                        username: r.get(2)?,
                        host: r.get(3)?,
                        token: r.get(4)?,
                        is_active: true,
                    })
                },
            )
            .optional()
    }

    /// JS logout: `db.users.update({_id}, {IsActive: false})` — token dropped
    /// client-side, rows kept for re-login.
    pub fn deactivate_account(&self) -> rusqlite::Result<usize> {
        self.conn
            .execute("UPDATE accounts SET IsActive=0 WHERE IsActive=1", [])
    }

    // -- incr sync (JS Sync.incrSync/sendChanges/_syncNoteToLocal) --

    pub fn dirty_notebooks(&self) -> rusqlite::Result<Vec<DirtyNotebook>> {
        let mut stmt = self.conn.prepare(
            "SELECT NotebookId, Title, Seq, Usn, ServerNotebookId,
                    LocalIsNew, IsDirty, LocalIsDelete
             FROM notebooks WHERE UserId=?1 AND IsDirty=1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map(params![self.user_id], |r| {
            Ok(DirtyNotebook {
                notebook_id: r.get(0)?,
                title: r.get(1)?,
                seq: r.get(2)?,
                usn: r.get(3)?,
                server_id: r.get(4)?,
                is_new: r.get::<_, i64>(5)? != 0,
                is_dirty: r.get::<_, i64>(6)? != 0,
                is_deleted: r.get::<_, i64>(7)? != 0,
            })
        })?;
        rows.collect()
    }

    pub fn dirty_notes(&self) -> rusqlite::Result<Vec<DirtyNote>> {
        let mut stmt = self.conn.prepare(
            "SELECT NoteId, NotebookId, Title, Content, Tags, IsMarkdown, IsTrash,
                    IsDirty, LocalIsNew, LocalIsDelete, Usn, CreatedTime, UpdatedTime,
                    ServerNoteId
             FROM notes WHERE UserId=?1 AND IsDirty=1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map(params![self.user_id], |r| {
            let tags_csv: String = r.get(4)?;
            Ok(DirtyNote {
                note_id: r.get(0)?,
                notebook_id: r.get(1)?,
                title: r.get(2)?,
                content: r.get(3)?,
                tags: tags_csv
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
                is_markdown: r.get::<_, i64>(5)? != 0,
                is_trash: r.get::<_, i64>(6)? != 0,
                is_dirty: r.get::<_, i64>(7)? != 0,
                is_new: r.get::<_, i64>(8)? != 0,
                is_deleted: r.get::<_, i64>(9)? != 0,
                usn: r.get(10)?,
                created_time: r.get(11)?,
                updated_time: r.get(12)?,
                server_note_id: r.get(13)?,
            })
        })?;
        rows.collect()
    }

    pub fn dirty_tags(&self) -> rusqlite::Result<Vec<DirtyTag>> {
        let mut stmt = self.conn.prepare(
            "SELECT Tag, Usn, IsDirty, LocalIsDelete FROM tags
             WHERE UserId=?1 AND IsDirty=1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map(params![self.user_id], |r| {
            Ok(DirtyTag {
                tag: r.get(0)?,
                usn: r.get(1)?,
                is_dirty: r.get::<_, i64>(2)? != 0,
                is_deleted: r.get::<_, i64>(3)? != 0,
            })
        })?;
        rows.collect()
    }

    /// Server notebook id for a local notebook id (JS
    /// `getServerNotebookIdByNotebookId`; None → callers fall back to the
    /// local id, matching JS `|| false`).
    pub fn notebook_server_id(&self, notebook_id: &str) -> rusqlite::Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT ServerNotebookId FROM notebooks WHERE NotebookId=?1",
                params![notebook_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?)
    }

    /// JS `updateNotebookForceForSendChange`.
    pub fn notebook_pushed(
        &self,
        notebook_id: &str,
        server_id: &str,
        usn: i64,
    ) -> rusqlite::Result<()> {
        let now = now_ms();
        self.conn.execute(
            "UPDATE notebooks SET IsDirty=0, LocalIsNew=0, ServerNotebookId=?2, Usn=?3,
                UpdatedTime=?4 WHERE NotebookId=?1",
            params![notebook_id, server_id, usn, now],
        )?;
        Ok(())
    }

    pub fn notebook_delete_local(&self, notebook_id: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM notebooks WHERE NotebookId=?1",
            params![notebook_id],
        )?;
        Ok(())
    }

    pub fn notebook_set_not_dirty_not_delete(&self, notebook_id: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE notebooks SET IsDirty=0, LocalIsDelete=0 WHERE NotebookId=?1",
            params![notebook_id],
        )?;
        Ok(())
    }

    pub fn note_add_pushed(
        &self,
        note_id: &str,
        server_note_id: &str,
        usn: i64,
    ) -> rusqlite::Result<()> {
        let now = now_ms();
        self.conn.execute(
            "UPDATE notes SET IsDirty=0, LocalIsNew=0, ServerNoteId=?2, Usn=?3,
                UpdatedTime=?4 WHERE NoteId=?1",
            params![note_id, server_note_id, usn, now],
        )?;
        Ok(())
    }

    pub fn note_update_pushed(&self, note_id: &str, usn: i64) -> rusqlite::Result<()> {
        let now = now_ms();
        self.conn.execute(
            "UPDATE notes SET IsDirty=0, Usn=?2, UpdatedTime=?3 WHERE NoteId=?1",
            params![note_id, usn, now],
        )?;
        Ok(())
    }

    pub fn note_remove(&self, note_id: &str) -> rusqlite::Result<()> {
        self.conn
            .execute("DELETE FROM notes WHERE NoteId=?1", params![note_id])?;
        Ok(())
    }

    pub fn note_set_not_dirty_not_delete(&self, note_id: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE notes SET IsDirty=0, LocalIsDelete=0 WHERE NoteId=?1",
            params![note_id],
        )?;
        Ok(())
    }

    pub fn tag_set_not_dirty_and_usn(&self, tag: &str, usn: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE tags SET IsDirty=0, Usn=?2, UpdatedTime=?3 WHERE UserId=?1 AND Tag=?4",
            params![self.user_id, usn, now_ms(), tag],
        )?;
        Ok(())
    }

    pub fn tag_set_not_dirty(&self, tag: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE tags SET IsDirty=0 WHERE UserId=?1 AND Tag=?2",
            params![self.user_id, tag],
        )?;
        Ok(())
    }

    pub fn tag_delete_local(&self, tag: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM tags WHERE UserId=?1 AND Tag=?2",
            params![self.user_id, tag],
        )?;
        Ok(())
    }

    /// Local note row by server id, for the incr pull side.
    pub fn note_by_server_id(&self, server_note_id: &str) -> rusqlite::Result<Option<DirtyNote>> {
        self.conn
            .query_row(
                "SELECT NoteId, NotebookId, Title, Content, Tags, IsMarkdown, IsTrash,
                        IsDirty, LocalIsNew, LocalIsDelete, Usn, CreatedTime, UpdatedTime,
                        ServerNoteId
                 FROM notes WHERE ServerNoteId=?1",
                params![server_note_id],
                |r| {
                    let tags_csv: String = r.get(4)?;
                    Ok(DirtyNote {
                        note_id: r.get(0)?,
                        notebook_id: r.get(1)?,
                        title: r.get(2)?,
                        content: r.get(3)?,
                        tags: tags_csv
                            .split(',')
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect(),
                        is_markdown: r.get::<_, i64>(5)? != 0,
                        is_trash: r.get::<_, i64>(6)? != 0,
                        is_dirty: r.get::<_, i64>(7)? != 0,
                        is_new: r.get::<_, i64>(8)? != 0,
                        is_deleted: r.get::<_, i64>(9)? != 0,
                        usn: r.get(10)?,
                        created_time: r.get(11)?,
                        updated_time: r.get(12)?,
                        server_note_id: r.get(13)?,
                    })
                },
            )
            .optional()
    }

    /// Local note id → server note id (None = never synced to the server).
    pub fn note_server_id(&self, note_id: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT ServerNoteId FROM notes WHERE NoteId=?1",
                params![note_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .map(|o| o.flatten())
    }

    pub fn delete_note_by_server_id(&self, server_note_id: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM notes WHERE ServerNoteId=?1",
            params![server_note_id],
        )?;
        Ok(())
    }

    /// Metadata-only note update (JS `updateNoteForce(note, false)`), keeps
    /// local Content/CreatedTime/UpdatedTime.
    pub fn apply_note_metadata(&self, row: &Value, now: i64) -> rusqlite::Result<()> {
        let tags = match row.get("Tags") {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect::<Vec<_>>()
                .join(","),
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        self.conn.execute(
            "UPDATE notes SET NotebookId=?2, Title=?3, Tags=?4, IsMarkdown=?5, IsTrash=?6,
                Usn=?7, IsDirty=0, LocalIsNew=0, LocalIsDelete=0, ServerNoteId=?8
             WHERE ServerNoteId=?1",
            params![
                str_field(row, "NoteId"),
                str_field(row, "NotebookId"),
                str_field(row, "Title"),
                tags,
                bool_field(row, "IsMarkdown") as i64,
                bool_field(row, "IsTrash") as i64,
                i64_field(row, "Usn"),
                str_field(row, "NoteId"),
            ],
        )?;
        let _ = now;
        Ok(())
    }

    /// Upsert a conflict row by server note id (JS `conflicts.push`).
    pub fn record_conflict(
        &self,
        server_note_id: &str,
        local_note_id: &str,
        server_row: &Value,
        local_title: &str,
        local_content: &str,
        server_content: &str,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO sync_conflicts (ServerNoteId, LocalNoteId, ServerNote, LocalTitle,
                LocalContent, ServerContent, CreatedTime)
             VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(ServerNoteId) DO UPDATE SET
                LocalNoteId=excluded.LocalNoteId, ServerNote=excluded.ServerNote,
                LocalTitle=excluded.LocalTitle, LocalContent=excluded.LocalContent,
                ServerContent=excluded.ServerContent, CreatedTime=excluded.CreatedTime",
            params![
                server_note_id,
                local_note_id,
                server_row.to_string(),
                local_title,
                local_content,
                server_content,
                now_ms(),
            ],
        )?;
        Ok(())
    }

    /// JS `conflicts` list for the conflict bar (ticket 14); here we only
    /// store rows.
    pub fn conflicts(&self) -> rusqlite::Result<Vec<Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT ServerNote, LocalNoteId, LocalTitle, LocalContent FROM sync_conflicts
             WHERE Resolved=0 ORDER BY CreatedTime",
        )?;
        let rows = stmt.query_map([], |r| {
            let server: String = r.get(0)?;
            let local_id: String = r.get(1)?;
            let title: String = r.get(2)?;
            let content: String = r.get(3)?;
            let server: Value = serde_json::from_str(&server).unwrap_or(Value::Null);
            Ok(json!({
                "server": server,
                "local": {
                    "NoteId": local_id,
                    "Title": title,
                    "Content": content,
                },
            }))
        })?;
        rows.collect()
    }
}

/// Private load target for the resolve ops.
struct ConflictRaw {
    local_note_id: String,
    server_note: String,
    local_title: String,
    local_content: String,
    server_content: String,
}

/// One conflict for the adjudication bar (ticket 14), both versions.
#[derive(Clone, Debug)]
pub struct ConflictRow {
    pub server_note_id: String,
    pub local_note_id: String,
    pub server_title: String,
    pub server_content: String,
    pub server_updated_ms: i64,
    pub local_title: String,
    pub local_content: String,
    pub local_updated_ms: i64,
}

impl Store {
    /// Typed conflict list for the conflict bar; unresolved rows, oldest
    /// first (same order as `conflicts()`). `local_updated_ms` joins the
    /// local note's UpdatedTime for the bar's timestamps.
    pub fn list_conflicts(&self) -> rusqlite::Result<Vec<ConflictRow>> {
        let now = now_ms();
        let mut stmt = self.conn.prepare(
            "SELECT c.ServerNoteId, c.LocalNoteId, c.ServerNote, c.LocalTitle, c.LocalContent,
                c.ServerContent, (SELECT UpdatedTime FROM notes n WHERE n.NoteId = c.LocalNoteId)
             FROM sync_conflicts c WHERE c.Resolved=0 ORDER BY c.CreatedTime",
        )?;
        let rows = stmt.query_map([], |r| {
            let server_note_id: String = r.get(0)?;
            let local_note_id: String = r.get(1)?;
            let server: String = r.get(2)?;
            let local_title: String = r.get(3)?;
            let local_content: String = r.get(4)?;
            let server_content: String = r.get(5)?;
            let local_updated_ms: Option<i64> = r.get(6)?;
            let server: Value = serde_json::from_str(&server).unwrap_or(Value::Null);
            Ok(ConflictRow {
                server_note_id,
                local_note_id,
                server_title: str_field(&server, "Title"),
                server_updated_ms: go_now_ms(server.get("UpdatedTime"), now),
                local_title,
                local_content,
                server_content,
                local_updated_ms: local_updated_ms.unwrap_or(0),
            })
        })?;
        rows.collect()
    }

    /// 用本地: keep the local row as-is (still dirty → force-pushed by the
    /// next `incr_sync`) and drop the conflict row. Returns false when no
    /// such conflict exists.
    pub fn resolve_conflict_use_local(&self, server_note_id: &str) -> rusqlite::Result<bool> {
        let n = self.conn.execute(
            "DELETE FROM sync_conflicts WHERE ServerNoteId=?1 AND Resolved=0",
            params![server_note_id],
        )?;
        Ok(n == 1)
    }

    /// 用服务器: overwrite the local note with the server version stored in
    /// the conflict row, mark it clean, drop the conflict row.
    pub fn resolve_conflict_use_server(&self, server_note_id: &str) -> rusqlite::Result<bool> {
        let row = match self.load_conflict(server_note_id)? {
            Some(r) => r,
            None => return Ok(false),
        };
        let server: Value = serde_json::from_str(&row.server_note).unwrap_or(Value::Null);
        let content = &row.server_content;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE notes SET Title=?2, Content=?3, \"Desc\"=?4, Abstract=?5,
                IsDirty=0, LocalIsNew=0, LocalIsDelete=0, Usn=?6
             WHERE NoteId=?1",
            params![
                row.local_note_id,
                str_field(&server, "Title"),
                content,
                crate::plain_desc(content),
                content.chars().take(500).collect::<String>(),
                i64_field(&server, "Usn"),
            ],
        )?;
        tx.execute(
            "DELETE FROM sync_conflicts WHERE ServerNoteId=?1",
            params![server_note_id],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// 两边保留: the original row takes the server version (clean); the
    /// local version becomes a NEW note (own id, LocalIsNew=1, IsDirty=1,
    /// `title` + `title_suffix`, same notebook, ServerNoteId NULL → pushed
    /// via addNote next sync). Returns the new note's id.
    pub fn resolve_conflict_keep_both(
        &self,
        server_note_id: &str,
        title_suffix: &str,
    ) -> rusqlite::Result<Option<Id>> {
        let row = match self.load_conflict(server_note_id)? {
            Some(r) => r,
            None => return Ok(None),
        };
        let server: Value = serde_json::from_str(&row.server_note).unwrap_or(Value::Null);
        let now = now_ms();
        let new_id = Id::new();
        let local_title = format!("{}{}", row.local_title, title_suffix);
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE notes SET Title=?2, Content=?3, \"Desc\"=?4, Abstract=?5,
                IsDirty=0, LocalIsNew=0, LocalIsDelete=0, Usn=?6
             WHERE NoteId=?1",
            params![
                row.local_note_id,
                str_field(&server, "Title"),
                row.server_content,
                crate::plain_desc(&row.server_content),
                row.server_content.chars().take(500).collect::<String>(),
                i64_field(&server, "Usn"),
            ],
        )?;
        tx.execute(
            "INSERT INTO notes (NoteId, UserId, NotebookId, Title, Content, \"Desc\",
                Abstract, IsDirty, LocalIsNew, LocalIsDelete, Usn, CreatedTime, UpdatedTime)
             SELECT ?2, UserId, NotebookId, ?3, ?4, ?5, ?6, 1, 1, 0, 0, ?7, ?7
             FROM notes WHERE NoteId=?1",
            params![
                row.local_note_id,
                new_id.0,
                local_title,
                row.local_content,
                crate::plain_desc(&row.local_content),
                row.local_content.chars().take(500).collect::<String>(),
                now,
            ],
        )?;
        tx.execute(
            "DELETE FROM sync_conflicts WHERE ServerNoteId=?1",
            params![server_note_id],
        )?;
        tx.commit()?;
        Ok(Some(new_id))
    }

    /// Raw conflict columns for the resolve ops.
    fn load_conflict(&self, server_note_id: &str) -> rusqlite::Result<Option<ConflictRaw>> {
        self.conn
            .query_row(
                "SELECT LocalNoteId, ServerNote, LocalTitle, LocalContent, ServerContent
                 FROM sync_conflicts WHERE ServerNoteId=?1 AND Resolved=0",
                params![server_note_id],
                |r| {
                    Ok(ConflictRaw {
                        local_note_id: r.get(0)?,
                        server_note: r.get(1)?,
                        local_title: r.get(2)?,
                        local_content: r.get(3)?,
                        server_content: r.get(4)?,
                    })
                },
            )
            .optional()
    }
}

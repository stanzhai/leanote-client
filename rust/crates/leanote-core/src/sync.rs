//! Server client + login + full sync (wire authority: src/api.js, src/sync.js,
//! src/user.js, src/note.js, src/notebook.js, src/tag.js).
//!
//! Full sync pulls notebooks → notes → tags in USN chunks (maxEntry 200, same
//! as JS `Sync._notebookMaxEntry`), then each synced note's content via
//! `note/getNoteContent`. Every chunk is committed transactionally; a kill
//! mid-sync leaves the previous chunk intact and a re-run converges (rows are
//! keyed by server id, so re-pulled chunks update rather than duplicate).

use serde_json::Value;
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
            &[("macAddr", "rust".to_string()), ("v", version().to_string())],
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

    fn new(host: &str, token: &str) -> Self {
        Api {
            agent: default_agent(),
            host: host.to_string(),
            token: token.to_string(),
        }
    }

    fn url(&self, path: &str, params: &[(&str, String)]) -> String {
        let mut url = format!("{}/api/{}?token={}", self.host, path, urlencode(&self.token));
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
                m.get("Msg").and_then(Value::as_str).unwrap_or("").to_string(),
            )),
            _ => Ok(v),
        }
    }

    /// GET user/getSyncState → LastSyncUsn.
    pub fn get_sync_state(&self) -> Result<i64, SyncError> {
        let v = Self::ok_or_api(
            self.get("user/getSyncState", &[("macAddr", "rust".to_string())])?,
        )?;
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
        let v = Self::ok_or_api(
            self.get("note/getNoteContent", &[("noteId", server_note_id.to_string())])?,
        )?;
        Ok(v.get("Content").and_then(Value::as_str).unwrap_or("").to_string())
    }
}

fn default_agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(10)))
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .new_agent()
}

fn version() -> &'static str {
    concat!(env!("CARGO_PKG_VERSION"), "_rust")
}

/// Only the values we emit need escaping (token, email, ids, numbers).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
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
            s.get(11..13).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0),
            s.get(14..16).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0),
            s.get(17..19).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0),
        )
    } else {
        (0, 0, 0)
    };
    let y = s.get(0..4).and_then(|x| x.parse::<i64>().ok()).unwrap_or(1970);
    let mo = s.get(5..7).and_then(|x| x.parse::<i64>().ok()).unwrap_or(1);
    let d = s.get(8..10).and_then(|x| x.parse::<i64>().ok()).unwrap_or(1);
    if !(1..=12).contains(&mo) {
        return now;
    }
    days_from_civil(y, mo, d).saturating_mul(86_400_000)
        + h * 3_600_000
        + mi * 60_000
        + sec * 1_000
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

/// Progress sink for `full_sync` (CLI prints, tests stay silent).
pub trait SyncProgress {
    fn note(&self, msg: String) {
        let _ = msg;
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
    let mut stats = SyncStats { notebooks: 0, notes: 0, contents: 0, tags: 0 };
    let now = now_ms();

    for (kind, apply) in [
        ("notebook", &apply_notebooks as &dyn Fn(&Store, &[Value], i64) -> rusqlite::Result<()>),
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
    for sid in server_ids {
        let content = api.get_note_content(&sid)?;
        store
            .set_note_content_by_server_id(&sid, &content)
            .map_err(|e| SyncError::Unconnected(e.to_string()))?;
        stats.contents += 1;
        progress.note(format!("content: {sid}"));
    }

    // JS `updateLastSyncState`: LastSyncUsn comes from the server.
    let server_usn = api.get_sync_state()?;
    store
        .set_last_sync_usn(server_usn)
        .map_err(|e| SyncError::Unconnected(e.to_string()))?;
    Ok(stats)
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
        tx.execute("UPDATE accounts SET IsActive=0 WHERE UserId<>?1", params![account.user_id])?;
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
                tx.execute("DELETE FROM notebooks WHERE ServerNotebookId=?1", params![server_id])?;
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
                tx.execute("DELETE FROM notes WHERE ServerNoteId=?1", params![server_id])?;
                continue;
            }
            let tags = match row.get("Tags") {
                Some(Value::Array(a)) => a
                    .iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
                    .join(","),
                Some(Value::String(s)) => s.clone(),
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
        self.conn
            .execute("UPDATE accounts SET LastSyncUsn=?1 WHERE IsActive=1", params![usn])?;
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
}

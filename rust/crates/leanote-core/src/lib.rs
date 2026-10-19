//! leanote-core: domain types + SQLite store.
//!
//! Wire/storage field names are PascalCase exactly as the existing JS
//! (see src/sync.js, src/note.js — those are the authority).
//! New SQLite store at `~/Library/Application Support/leanote-rust/` —
//! NEVER reads/writes the Electron NeDB dir (ADR-0002).

use std::fmt;
use std::path::PathBuf;

pub mod sync;

use rusqlite::{params, Connection, OptionalExtension};

/// 24-hex-char id, same shape as the JS `objectId()` (12 random bytes hex).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Id(pub String);

impl Id {
    pub fn new() -> Self {
        let mut b = [0u8; 12];
        getrandom_bytes(&mut b);
        Id(hex(&b))
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::ops::Deref for Id {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

fn getrandom_bytes(buf: &mut [u8]) {
    // std-only: read from the OS entropy file. One syscall, no new dependency.
    #[cfg(unix)]
    {
        use std::io::Read;
        let mut f = std::fs::File::open("/dev/urandom").expect("open /dev/urandom");
        f.read_exact(buf).expect("read /dev/urandom");
    }
    #[cfg(not(unix))]
    {
        unimplemented!("non-unix id generation");
    }
}

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b.iter() {
        s.push_str(HEX[(*x >> 4) as usize]);
        s.push_str(HEX[(*x & 0xf) as usize]);
    }
    s
}
const HEX: [&str; 16] = [
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f",
];

// --- domain types (PascalCase names are the wire/storage contract) ---

#[derive(Clone, Debug, Default)]
pub struct Notebook {
    pub notebook_id: Id,
    pub user_id: String,
    pub title: String,
    pub parent_notebook_id: String, // "" = root; nested notebooks keep hierarchy
    pub seq: i64,                   // sibling ordering
    pub is_new: bool,               // LocalIsNew: created locally, not yet on server
    pub is_dirty: bool,
    pub is_deleted: bool, // LocalIsDelete
}

#[derive(Clone, Debug)]
pub struct Note {
    pub note_id: Id,
    pub user_id: String,
    pub notebook_id: Id,
    pub title: String,
    pub content: String,
    pub desc: String,
    pub img_src: String,
    pub tags: Vec<String>,
    pub abstract_: String,
    pub is_markdown: bool,
    pub is_trash: bool,
    pub is_dirty: bool,
    pub is_new: bool,     // LocalIsNew
    pub is_deleted: bool, // LocalIsDelete
    pub is_blog: bool,
    pub star: bool, // Star: local-only, never pushed to server (GLOSSARY)
    pub created_time: i64,
    pub updated_time: i64,
}

#[derive(Clone, Debug)]
pub struct Tag {
    pub tag_id: Id,
    pub user_id: String,
    pub tag: String,
    pub is_dirty: bool,
    pub count: i64,
    pub is_deleted: bool,
    pub created_time: i64,
    pub updated_time: i64,
}

#[derive(Clone, Debug, Default)]
pub struct Account {
    pub user_id: String,
    pub email: String,
    pub username: String,
    pub host: String,
    pub token: String,
    pub is_active: bool,
}

// --- store ---

pub const DB_FILE: &str = "leanote.db";

/// Rust app data dir; MUST differ from Electron's
/// `~/Library/Application Support/leanote/` (which contains NeDB `nedb55/`).
pub fn app_data_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME");
    #[cfg(target_os = "macos")]
    return PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("leanote-rust");
    #[cfg(not(target_os = "macos"))]
    return PathBuf::from(home).join(".config").join("leanote-rust");
}

pub struct Store {
    pub(crate) conn: Connection,
    pub(crate) user_id: String,
}

impl Store {
    pub fn open_default() -> rusqlite::Result<Self> {
        Self::open(app_data_dir(), default_user_id())
    }

    /// `dir` is the app data dir (e.g. `.../leanote-rust`); the SQLite file
    /// lives directly inside it.
    pub fn open(dir: PathBuf, user_id: String) -> rusqlite::Result<Self> {
        std::fs::create_dir_all(&dir).map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        let conn = Connection::open(dir.join(DB_FILE))?;
        // WAL: crash-safe + fast. journal_mode is persistent in the db file.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn, user_id })
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Raw scalar query for tests/tools that count rows directly.
    pub fn raw_query_row(&self, sql: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(sql, [], |r| r.get(0))
    }

    /// All writes go through transactions — a kill mid-write leaves the
    /// previous state, never a torn one.
    pub fn add_note(&self, note: NewNote) -> rusqlite::Result<Note> {
        let tx = self.conn.unchecked_transaction()?;
        let (notebook_id, _created) = ensure_notebook(&tx, &self.user_id, &note.notebook_title)?;
        let id = Id::new();
        let now = now_ms();
        let desc = plain_desc(&note.content);
        let abstract_: String = note.content.chars().take(500).collect();
        tx.execute(
            "INSERT INTO notes (NoteId, UserId, NotebookId, Title, Content, \"Desc\", ImgSrc, Tags, Abstract,
                IsMarkdown, IsTrash, IsDirty, LocalIsNew, LocalIsDelete, IsBlog, Star, CreatedTime, UpdatedTime)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0,1,1,0,0,0,?11,?11)",
            params![
                id.to_string(),
                self.user_id,
                notebook_id.to_string(),
                note.title,
                note.content,
                desc,
                "",
                tags_csv(&note.tags),
                abstract_,
                note.is_markdown as i64,
                now,
            ],
        )?;
        upsert_tags(&tx, &self.user_id, &note.tags, now)?;
        tx.commit()?;
        Ok(self.get_note(&id)?.expect("just inserted"))
    }

    /// Notes newest-first (matches JS `loadNotes` sort by UpdatedTime desc).
    pub fn list_notes(&self) -> rusqlite::Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(
            "SELECT NoteId, UserId, NotebookId, Title, Content, \"Desc\", ImgSrc, Tags, Abstract,
                IsMarkdown, IsTrash, IsDirty, LocalIsNew, LocalIsDelete, IsBlog, Star, CreatedTime, UpdatedTime
             FROM notes WHERE IsTrash=0 AND LocalIsDelete=0 AND IsDeleted=0
             ORDER BY UpdatedTime DESC, rowid ASC",
        )?;
        let rows = stmt.query_map([], row_to_note)?;
        rows.collect()
    }

    pub fn get_note(&self, id: &Id) -> rusqlite::Result<Option<Note>> {
        let mut stmt = self.conn.prepare(
            "SELECT NoteId, UserId, NotebookId, Title, Content, \"Desc\", ImgSrc, Tags, Abstract,
                IsMarkdown, IsTrash, IsDirty, LocalIsNew, LocalIsDelete, IsBlog, Star, CreatedTime, UpdatedTime
             FROM notes WHERE NoteId=?1 AND IsTrash=0 AND LocalIsDelete=0 AND IsDeleted=0",
        )?;
        stmt.query_row([id.to_string()], row_to_note).optional()
    }

    /// Unique-prefix resolve; same semantics as JS `show`:
    /// exact id first, else notes (newest-first) whose id starts with prefix.
    /// Inner Ok = found; Err = NotFound | Ambiguous.
    pub fn resolve_prefix(&self, prefix: &str) -> rusqlite::Result<Result<Note, PrefixError>> {
        if let Some(n) = self.get_note(&Id(prefix.to_string()))? {
            return Ok(Ok(n));
        }
        let notes = self.list_notes()?;
        let matches: Vec<&Note> = notes.iter().filter(|n| n.note_id.starts_with(prefix)).collect();
        Ok(match matches.as_slice() {
            [] => Err(PrefixError::NotFound),
            [one] => Ok((*one).clone()),
            many => Err(PrefixError::Ambiguous(many.len())),
        })
    }

    /// JS getNotebookTitle: fall back to the raw id when the notebook row is gone.
    pub fn notebook_title(&self, notebook_id: &str) -> rusqlite::Result<String> {
        match self
            .conn
            .query_row(
                "SELECT Title FROM notebooks WHERE NotebookId=?1",
                [notebook_id],
                |r| r.get(0),
            )
            .optional()?
        {
            Some(t) => Ok(t),
            None => Ok(notebook_id.to_string()),
        }
    }

    pub fn notebooks(&self) -> rusqlite::Result<Vec<Notebook>> {
        let mut stmt = self.conn.prepare(
            "SELECT NotebookId, UserId, Title, ParentNotebookId, Seq, LocalIsNew, IsDirty, LocalIsDelete
             FROM notebooks WHERE LocalIsDelete=0 ORDER BY Seq, rowid",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Notebook {
                notebook_id: Id(r.get(0)?),
                user_id: r.get(1)?,
                title: r.get(2)?,
                parent_notebook_id: r.get(3)?,
                seq: r.get(4)?,
                is_new: r.get::<_, i64>(5)? != 0,
                is_dirty: r.get::<_, i64>(6)? != 0,
                is_deleted: r.get::<_, i64>(7)? != 0,
            })
        })?;
        rows.collect()
    }

    pub fn tags(&self) -> rusqlite::Result<Vec<Tag>> {
        let mut stmt = self.conn.prepare(
            "SELECT TagId, UserId, Tag, IsDirty, Count, LocalIsDelete, CreatedTime, UpdatedTime
             FROM tags WHERE LocalIsDelete=0 ORDER BY Tag",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Tag {
                tag_id: Id(r.get(0)?),
                user_id: r.get(1)?,
                tag: r.get(2)?,
                is_dirty: r.get::<_, i64>(3)? != 0,
                count: r.get(4)?,
                is_deleted: r.get::<_, i64>(5)? != 0,
                created_time: r.get(6)?,
                updated_time: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn note_count(&self) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
    }

    /// JS handleUpdateNote + NoteService.updateNoteOrContent semantics:
    /// - desc/abstract recomputed only when content is provided
    /// - IsDirty/UpdatedTime only when Title/Content/Tags actually differ
    /// - no-op update (nothing differed) → Ok(Some(old)), nothing written
    /// - tags upsert only for a non-empty new tag list
    /// - `notebook_title` is a *title* (local-server semantics). The CLI
    ///   offline path also routes its resolved notebook *id* through this
    ///   field: when the value matches an existing notebook id, the note is
    ///   moved (JS CLI persists NotebookId); when it matches no id but does
    ///   match a title, it resolves to that id; otherwise ignored.
    pub fn update_note(&self, id: &Id, upd: NoteUpdate) -> rusqlite::Result<Option<Note>> {
        let Some(old) = self.get_note_any(id)? else {
            return Ok(None);
        };
        let title = upd.title.clone().unwrap_or_else(|| old.title.clone());
        let content = upd.content.clone().unwrap_or_else(|| old.content.clone());
        let tags = upd.tags.clone().unwrap_or_else(|| old.tags.clone());
        // JS CLI cmdEditInternal assigns fields.NotebookId = matched id.
        let notebook_id = match &upd.notebook_title {
            Some(v) => match self
                .conn
                .query_row(
                    "SELECT NotebookId FROM notebooks WHERE NotebookId=?1",
                    [v],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
            {
                Some(id) => Some(id),
                None => self
                    .conn
                    .query_row(
                        "SELECT NotebookId FROM notebooks WHERE Title=?1 AND LocalIsDelete=0",
                        [v],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()?
            },
            None => None,
        };
        let dirty = title != old.title
            || content != old.content
            || tags_csv(&tags) != tags_csv(&old.tags)
            || notebook_id.as_deref().map_or(false, |id| id != &*old.notebook_id);
        if !dirty {
            return Ok(Some(old));
        }
        let (desc, abstract_) = if upd.content.is_some() {
            (plain_desc(&content), content.chars().take(500).collect())
        } else {
            (old.desc.clone(), old.abstract_.clone())
        };
        let now = now_ms();

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE notes SET Title=?2, Content=?3, \"Desc\"=?4, Abstract=?5, Tags=?6,
                NotebookId=COALESCE(?8, NotebookId),
                IsDirty=1, LocalIsDelete=0, UpdatedTime=?7
             WHERE NoteId=?1",
            params![
                id.to_string(),
                title,
                content,
                desc,
                abstract_,
                tags_csv(&tags),
                now,
                notebook_id,
            ],
        )?;
        if let Some(t) = upd.tags.clone().filter(|t| !t.is_empty()) {
            upsert_tags(&tx, &self.user_id, &t, now)?;
        }
        tx.commit()?;
        self.get_note(id)
    }

    /// Note by exact id regardless of trash/deleted state (JS getNote uses
    /// findOne({NoteId}) with no status filter).
    pub fn get_note_any(&self, id: &Id) -> rusqlite::Result<Option<Note>> {
        let mut stmt = self.conn.prepare(
            "SELECT NoteId, UserId, NotebookId, Title, Content, \"Desc\", ImgSrc, Tags, Abstract,
                IsMarkdown, IsTrash, IsDirty, LocalIsNew, LocalIsDelete, IsBlog, Star, CreatedTime, UpdatedTime
             FROM notes WHERE NoteId=?1",
        )?;
        stmt.query_row([id.to_string()], row_to_note).optional()
    }

    /// JS local_server findNoteByPrefix: exact id first (any state), else
    /// prefix matches in insertion order, first non-trash/non-deleted wins.
    pub fn find_note_by_prefix(&self, prefix: &str) -> rusqlite::Result<Option<Note>> {
        if let Some(n) = self.get_note_any(&Id(prefix.to_string()))? {
            return Ok(Some(n));
        }
        let mut stmt = self.conn.prepare(
            "SELECT NoteId, UserId, NotebookId, Title, Content, \"Desc\", ImgSrc, Tags, Abstract,
                IsMarkdown, IsTrash, IsDirty, LocalIsNew, LocalIsDelete, IsBlog, Star, CreatedTime, UpdatedTime
             FROM notes WHERE NoteId LIKE ?1 || '%' ORDER BY rowid",
        )?;
        let rows = stmt.query_map([prefix.replace('%', "\\%").replace('_', "\\_")], row_to_note)?;
        for n in rows {
            let n = n?;
            if !n.is_trash && !n.is_deleted {
                return Ok(Some(n));
            }
        }
        Ok(None)
    }

    /// Unified search (spec §30): case-insensitive substring over
    /// Title+Content+Tags, weights Title 3 / Tags 2 / Content 1, excerpt
    /// window exactly as `src/leanote_cli.js` cmdSearch.
    pub fn search(&self, keyword: &str, limit: usize) -> rusqlite::Result<Vec<SearchHit>> {
        Ok(search_notes(&self.list_notes()?, keyword, limit))
    }

     pub fn journal_mode(&self) -> rusqlite::Result<String> {
         self.conn
             .query_row("PRAGMA journal_mode", [], |r| r.get(0))
     }
}

#[derive(Debug)]
pub enum PrefixError {
    NotFound,
    Ambiguous(usize),
}

/// Offline fallback user id — the JS CLI uses `user1` when no user exists.
fn default_user_id() -> String {
    "user1".to_string()
}

pub struct NewNote {
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub notebook_title: String,
    pub is_markdown: bool,
}

/// Fields the local HTTP updateNote accepts (None = leave unchanged).
/// `notebook` moves the note by target notebook *title*, like JS.
#[derive(Default)]
pub struct NoteUpdate {
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub notebook_title: Option<String>,
}

/// One ranked search result. `excerpt` mirrors the JS CLI's context window.
#[derive(Clone, Debug)]
pub struct SearchHit {
    pub note: Note,
    pub score: u32,
    pub excerpt: String,
}

/// Case-insensitive substring search over Title+Content+Tags.
/// Weights: Title 3, Tags 2, Content 1. Sort: score desc, then insertion
/// order (JS `Array.prototype.sort` is stable). Excerpt (verbatim from
/// `src/leanote_cli.js` cmdSearch): 30 units before the first Content hit,
/// kw.len+50 after, `...` on both cut sides, newlines → spaces; when
/// Content misses, Desc[:80] (Content fallback).
pub fn search_notes(notes: &[Note], keyword: &str, limit: usize) -> Vec<SearchHit> {
    if keyword.is_empty() {
        return Vec::new();
    }
    let kw_lower = keyword.to_lowercase();
    let mut out: Vec<SearchHit> = Vec::new();
    for n in notes {
        let in_title = n.title.to_lowercase().contains(&kw_lower);
        let in_content = n.content.to_lowercase().contains(&kw_lower);
        let in_tags = n.tags.iter().any(|t| t.to_lowercase().contains(&kw_lower));
        if !(in_title || in_content || in_tags) {
            continue;
        }
        let score = (in_title as u32) * 3 + (in_tags as u32) * 2 + (in_content as u32) * 1;
        out.push(SearchHit { note: n.clone(), score, excerpt: excerpt(n, keyword) });
    }
    out.sort_by(|a, b| b.score.cmp(&a.score));
    out.truncate(limit);
    out
}

/// JS: idx = content.toLowerCase().indexOf(kw); start = max(0, idx-30);
/// end = min(len, idx+kw.len+50); "..." when start>0 / end<len; \n -> " ".
/// All indices are JS UTF-16 code units, kw.len included.
fn excerpt(n: &Note, keyword: &str) -> String {
    let cu: Vec<u16> = n.content.encode_utf16().collect();
    let lower_cu: Vec<u16> = n.content.to_lowercase().encode_utf16().collect();
    let lower_kw: Vec<u16> = keyword.to_lowercase().encode_utf16().collect();
    match find_sub(&lower_cu, &lower_kw) {
        Some(i) => {
            let start = i.saturating_sub(30);
            let end = (i + keyword.encode_utf16().count() + 50).min(cu.len());
            let mut s = String::new();
            if start > 0 {
                s.push_str("...");
            }
            s.push_str(&String::from_utf16_lossy(&cu[start..end]).replace('\n', " "));
            if end < cu.len() {
                s.push_str("...");
            }
            s
        }
        None => {
            let src = if n.desc.is_empty() && !n.content.is_empty() {
                &n.content
            } else {
                &n.desc
            };
            utf16_prefix(src, 80)
        }
    }
}

fn find_sub(hay: &[u16], needle: &[u16]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// JS String.prototype.substring(0, n): UTF-16 code-unit prefix.
fn utf16_prefix(s: &str, n: usize) -> String {
    let cu: Vec<u16> = s.encode_utf16().collect();
    String::from_utf16_lossy(&cu[..n.min(cu.len())])
}

/// JS: content.replace(/\n/g," ").replace(/#/g,"").substring(0,50).trim()
pub(crate) fn plain_desc(content: &str) -> String {
    content
        .replace('\n', " ")
        .replace('#', "")
        .chars()
        .take(50)
        .collect::<String>()
        .trim()
        .to_string()
}

fn tags_csv(tags: &[String]) -> String {
    tags.join(",")
}

fn parse_tags_csv(s: String) -> Vec<String> {
    if s.is_empty() {
        Vec::new()
    } else {
        s.split(',').map(|p| p.to_string()).collect()
    }
}

/// Find or create the target notebook inside an open transaction.
/// Auto-created notebook gets Seq:-1 + dirty flags, like the JS CLI.
fn ensure_notebook(
    tx: &rusqlite::Transaction<'_>,
    user_id: &str,
    title: &str,
) -> rusqlite::Result<(Id, bool)> {
    if let Some(id) = tx
        .query_row(
            "SELECT NotebookId FROM notebooks WHERE Title=?1 AND UserId=?2 LIMIT 1",
            params![title, user_id],
            |r| r.get::<_, String>(0).map(Id),
        )
        .optional()?
    {
        return Ok((id, false));
    }
    let id = Id::new();
    tx.execute(
        "INSERT INTO notebooks (NotebookId, UserId, Title, ParentNotebookId, Seq, Usn, LocalIsNew, IsDirty, LocalIsDelete)
         VALUES (?1,?2,?3,'',-1,0,1,1,0)",
        params![id.to_string(), user_id, title],
    )?;
    Ok((id, true))
}

fn upsert_tags(
    tx: &rusqlite::Transaction<'_>,
    user_id: &str,
    tags: &[String],
    now: i64,
) -> rusqlite::Result<()> {
    for t in tags {
        let changed = tx.execute(
            "UPDATE tags SET UpdatedTime=?2, LocalIsDelete=0, IsDirty=1
             WHERE UserId=?3 AND Tag=?1",
            params![t, now, user_id],
        )?;
        if changed == 0 {
            tx.execute(
                "INSERT INTO tags (TagId, UserId, Tag, IsDirty, Count, LocalIsDelete, CreatedTime, UpdatedTime)
                 VALUES (?1,?2,?3,1,1,0,?4,?4)",
                params![Id::new().to_string(), user_id, t, now],
            )?;
        }
    }
    Ok(())
}

fn row_to_note(r: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        note_id: Id(r.get(0)?),
        user_id: r.get(1)?,
        notebook_id: Id(r.get(2)?),
        title: r.get(3)?,
        content: r.get(4)?,
        desc: r.get(5)?,
        img_src: r.get(6)?,
        tags: parse_tags_csv(r.get::<_, String>(7)?),
        abstract_: r.get(8)?,
        is_markdown: r.get::<_, i64>(9)? != 0,
        is_trash: r.get::<_, i64>(10)? != 0,
        is_dirty: r.get::<_, i64>(11)? != 0,
        is_new: r.get::<_, i64>(12)? != 0,
        is_deleted: r.get::<_, i64>(13)? != 0,
        is_blog: r.get::<_, i64>(14)? != 0,
        star: r.get::<_, i64>(15)? != 0,
        created_time: r.get(16)?,
        updated_time: r.get(17)?,
    })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS notebooks (
    NotebookId TEXT PRIMARY KEY,
    UserId TEXT NOT NULL,
    Title TEXT NOT NULL,
    ParentNotebookId TEXT NOT NULL DEFAULT '',
    Seq INTEGER NOT NULL DEFAULT 0,
    Usn INTEGER NOT NULL DEFAULT 0,
    LocalIsNew INTEGER NOT NULL DEFAULT 0,
    IsDirty INTEGER NOT NULL DEFAULT 0,
    LocalIsDelete INTEGER NOT NULL DEFAULT 0,
    CreatedTime INTEGER NOT NULL DEFAULT 0,
    UpdatedTime INTEGER NOT NULL DEFAULT 0,
    ServerNotebookId TEXT DEFAULT NULL
);
CREATE TABLE IF NOT EXISTS notes (
    NoteId TEXT PRIMARY KEY,
    UserId TEXT NOT NULL,
    NotebookId TEXT NOT NULL,
    Title TEXT NOT NULL DEFAULT '',
    Content TEXT NOT NULL DEFAULT '',
    \"Desc\" TEXT NOT NULL DEFAULT '',
    ImgSrc TEXT NOT NULL DEFAULT '',
    Tags TEXT NOT NULL DEFAULT '',
    Abstract TEXT NOT NULL DEFAULT '',
    IsMarkdown INTEGER NOT NULL DEFAULT 1,
    IsTrash INTEGER NOT NULL DEFAULT 0,
    IsDeleted INTEGER NOT NULL DEFAULT 0,
    IsDirty INTEGER NOT NULL DEFAULT 0,
    LocalIsNew INTEGER NOT NULL DEFAULT 0,
    LocalIsDelete INTEGER NOT NULL DEFAULT 0,
    IsBlog INTEGER NOT NULL DEFAULT 0,
    Star INTEGER NOT NULL DEFAULT 0,
    Usn INTEGER NOT NULL DEFAULT 0,
    CreatedTime INTEGER NOT NULL DEFAULT 0,
    UpdatedTime INTEGER NOT NULL DEFAULT 0,
    ServerNoteId TEXT DEFAULT NULL
);
CREATE TABLE IF NOT EXISTS tags (
    TagId TEXT PRIMARY KEY,
    UserId TEXT NOT NULL,
    Tag TEXT NOT NULL,
    IsDirty INTEGER NOT NULL DEFAULT 0,
    Count INTEGER NOT NULL DEFAULT 0,
    LocalIsDelete INTEGER NOT NULL DEFAULT 0,
    Usn INTEGER NOT NULL DEFAULT 0,
    CreatedTime INTEGER NOT NULL DEFAULT 0,
    UpdatedTime INTEGER NOT NULL DEFAULT 0,
    UNIQUE(UserId, Tag)
);
CREATE TABLE IF NOT EXISTS accounts (
    UserId TEXT PRIMARY KEY,
    Email TEXT NOT NULL DEFAULT '',
    Username TEXT NOT NULL DEFAULT '',
    Host TEXT NOT NULL DEFAULT '',
    Token TEXT NOT NULL DEFAULT '',
    LastSyncUsn INTEGER NOT NULL DEFAULT -1,
    IsActive INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(UpdatedTime DESC);
CREATE INDEX IF NOT EXISTS idx_notes_notebook ON notes(NotebookId);
CREATE INDEX IF NOT EXISTS idx_notebooks_user_title ON notebooks(UserId, Title);
";

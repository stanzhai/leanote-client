//! leanote-core: domain types + SQLite store.
//!
//! Wire/storage field names are PascalCase exactly as the existing JS
//! (see src/sync.js, src/note.js — those are the authority).
//! New SQLite store at `~/Library/Application Support/leanote-rust/` —
//! NEVER reads/writes the Electron NeDB dir (ADR-0002).

use std::fmt;
use std::path::PathBuf;

pub mod html2md;
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

/// One local content snapshot (ticket 10). Max 20 per note, consecutive
/// identical content deduped.
#[derive(Clone, Debug)]
pub struct NoteHistory {
    pub history_id: Id,
    pub note_id: Id,
    pub title: String,
    pub content: String,
    pub created_time: i64,
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

/// Column list + live-note filter shared by the browse queries (JS _getNotes).
pub(crate) const NOTES_LIVE_BASE: &str = "SELECT NoteId, UserId, NotebookId, Title, Content, \"Desc\", ImgSrc, Tags, Abstract,
    IsMarkdown, IsTrash, IsDirty, LocalIsNew, LocalIsDelete, IsBlog, Star, CreatedTime, UpdatedTime
 FROM notes WHERE IsTrash=0 AND LocalIsDelete=0 AND IsDeleted=0";

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

    /// JS Note._getNotes(notebookId, isTrash=false, isStar): live notes,
    /// optionally narrowed to one notebook and/or Star=1. Trash is a
    /// different query (JS getTrashNotes) → [`Store::list_trash`].
    pub fn list_notes_filtered(
        &self,
        notebook_id: Option<&Id>,
        starred: bool,
    ) -> rusqlite::Result<Vec<Note>> {
        let mut sql = NOTES_LIVE_BASE.to_string();
        if starred {
            sql.push_str(" AND Star=1");
        }
        if let Some(nb) = notebook_id {
            // ' is the only char needing escape in a SQLite string literal.
            sql.push_str(&format!(" AND NotebookId='{}'", nb.replace('\'', "''")));
        }
        sql.push_str(" ORDER BY UpdatedTime DESC, rowid ASC");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_note)?;
        rows.collect()
    }

    /// Notes carrying `tag` (JS searchNoteByTag: live notes, whole-tag match
    /// via `,tag,` inside the CSV-wrapped Tags column).
    pub fn list_notes_by_tag(&self, tag: &str) -> rusqlite::Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(&format!(
            "{NOTES_LIVE_BASE} AND (',' || Tags || ',') LIKE ?
             ORDER BY UpdatedTime DESC, rowid ASC"
        ))?;
        let pat = format!("%,{},%", tag);
        let rows = stmt.query_map([pat], row_to_note)?;
        rows.collect()
    }

    /// Trashed notes newest-first (JS getTrashNotes: IsTrash=1, no other filter).
    pub fn list_trash(&self) -> rusqlite::Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(
            "SELECT NoteId, UserId, NotebookId, Title, Content, \"Desc\", ImgSrc, Tags, Abstract,
                IsMarkdown, IsTrash, IsDirty, LocalIsNew, LocalIsDelete, IsBlog, Star, CreatedTime, UpdatedTime
             FROM notes WHERE IsTrash=1
             ORDER BY UpdatedTime DESC, rowid ASC",
        )?;
        let rows = stmt.query_map([], row_to_note)?;
        rows.collect()
    }

    /// Live-note count per notebook (JS reCountNotebookNumberNotes).
    pub fn notebook_note_counts(&self) -> rusqlite::Result<Vec<(Id, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT NotebookId, COUNT(*) FROM notes
             WHERE IsTrash=0 AND LocalIsDelete=0 AND IsDeleted=0
             GROUP BY NotebookId",
        )?;
        let rows = stmt.query_map([], |r| Ok((Id(r.get(0)?), r.get(1)?)))?;
        rows.collect()
    }

    /// Toggle Star. JS Note.star: sets Star + UpdatedTime ONLY — IsDirty
    /// stays untouched so the change never reaches the server.
    pub fn toggle_star(&self, id: &Id) -> rusqlite::Result<Option<bool>> {
        let n = self.conn.execute(
            "UPDATE notes SET Star = 1-Star, UpdatedTime=?2 WHERE NoteId=?1",
            params![id.to_string(), now_ms()],
        )?;
        if n == 0 {
            return Ok(None);
        }
        self.conn
            .query_row(
                "SELECT Star FROM notes WHERE NoteId=?1",
                [id.to_string()],
                |r| r.get::<_, i64>(0).map(|v| v != 0),
            )
            .map(Some)
    }

    /// Restore from trash into `notebook_id` (JS moveNote on a trashed
    /// note): clears IsTrash/LocalIsDelete, sets IsDirty so sync pushes it.
    pub fn restore_note(&self, id: &Id, notebook_id: &Id) -> rusqlite::Result<usize> {
        self.conn.execute(
            "UPDATE notes SET IsTrash=0, LocalIsDelete=0, IsDirty=1, NotebookId=?2, UpdatedTime=?3
             WHERE NoteId=?1",
            params![id.to_string(), notebook_id.to_string(), now_ms()],
        )
    }

    /// Hard delete (JS deleteTrash local-user branch): row removed, nothing
    /// left to push. Local history goes with it.
    pub fn hard_delete_note(&self, id: &Id) -> rusqlite::Result<usize> {
        self.conn.execute("DELETE FROM note_history WHERE NoteId=?1", [id.to_string()])?;
        self.conn.execute("DELETE FROM notes WHERE NoteId=?1", [id.to_string()])
    }

    /// Empty trash (JS clearTrash): every trashed row marked deleted+dirty.
    pub fn empty_trash(&self) -> rusqlite::Result<usize> {
        self.conn.execute(
            "UPDATE notes SET LocalIsDelete=1, IsDirty=1 WHERE IsTrash=1",
            [],
        )
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
    /// Ticket 12「转存为 Markdown」: convert a rich-text note's HTML body to
    /// Markdown IN PLACE — Content overwritten, IsMarkdown=1, IsDirty=1 so
    /// the next sync pushes updateNote. Explicit user action, one-way.
    /// Returns None when the note id is unknown.
    pub fn convert_to_markdown(&self, id: &Id) -> rusqlite::Result<Option<Note>> {
        let Some(old) = self.get_note_any(id)? else {
            return Ok(None);
        };
        if old.is_markdown {
            return Ok(Some(old)); // already Markdown, nothing to convert
        }
        let md = crate::html2md::html_to_markdown(&old.content);
        let now = now_ms();
        self.conn.execute(
            "UPDATE notes SET Content=?2, \"Desc\"=?3, Abstract=?4, IsMarkdown=1, IsDirty=1,
                UpdatedTime=?5 WHERE NoteId=?1",
            params![
                id.to_string(),
                md,
                plain_desc(&md),
                md.chars().take(500).collect::<String>(),
                now,
            ],
        )?;
        self.get_note(id)
    }

    // --- ticket 10: local note history ---

    /// Save title+content snapshot (inside an open tx): skip when identical
    /// to the newest row, else insert and trim to the 20 newest.
    fn push_history(
        tx: &rusqlite::Transaction<'_>,
        user_id: &str,
        note_id: &Id,
        title: &str,
        content: &str,
        now: i64,
    ) -> rusqlite::Result<()> {
        let last: Option<String> = tx
            .query_row(
                "SELECT Content FROM note_history WHERE NoteId=?1
                 ORDER BY CreatedTime DESC, rowid DESC LIMIT 1",
                [note_id.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        if last.as_deref() == Some(content) {
            return Ok(());
        }
        tx.execute(
            "INSERT INTO note_history (HistoryId, NoteId, UserId, Title, Content, CreatedTime)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![Id::new().to_string(), note_id.to_string(), user_id, title, content, now],
        )?;
        tx.execute(
            "DELETE FROM note_history WHERE NoteId=?1 AND HistoryId NOT IN (
                SELECT HistoryId FROM note_history WHERE NoteId=?1
                ORDER BY CreatedTime DESC, rowid DESC LIMIT 20)",
            [note_id.to_string()],
        )?;
        Ok(())
    }

    /// Update with an auto history row: when content actually changed, the
    /// pre-change content is snapshotted (dedup + trim inside), then the
    /// note row updates exactly like [`Store::update_note`] — IsDirty=1 so
    /// the next sync pushes it.
    pub fn save_note_with_history(
        &self,
        id: &Id,
        upd: NoteUpdate,
    ) -> rusqlite::Result<Option<Note>> {
        let Some(old) = self.get_note_any(id)? else {
            return Ok(None);
        };
        let now = now_ms();
        if upd.content.as_deref().is_some_and(|c| c != old.content) {
            let tx = self.conn.unchecked_transaction()?;
            Self::push_history(&tx, &self.user_id, id, &old.title, &old.content, now)?;
            tx.commit()?; // drop(tx) would ROLLBACK the snapshot
        }
        self.update_note(id, upd)
    }

    /// History rows newest-first, capped at 20 by construction.
    pub fn list_history(&self, note_id: &Id) -> rusqlite::Result<Vec<NoteHistory>> {
        let mut stmt = self.conn.prepare(
            "SELECT HistoryId, NoteId, Title, Content, CreatedTime FROM note_history
             WHERE NoteId=?1 ORDER BY CreatedTime DESC, rowid DESC LIMIT 20",
        )?;
        let rows = stmt.query_map([note_id.to_string()], |r| {
            Ok(NoteHistory {
                history_id: Id(r.get(0)?),
                note_id: Id(r.get(1)?),
                title: r.get(2)?,
                content: r.get(3)?,
                created_time: r.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// Restore an old version: it becomes the latest note content (dirty, so
    /// sync pushes it) and itself gets a fresh history row — restoring twice
    /// bounces between the same two versions without losing either.
    pub fn restore_history(&self, note_id: &Id, history_id: &Id) -> rusqlite::Result<Option<Note>> {
        let Some(h) = self
            .list_history(note_id)?
            .into_iter()
            .find(|h| h.history_id == *history_id)
        else {
            return Ok(None);
        };
        self.save_note_with_history(
            note_id,
            NoteUpdate {
                title: Some(h.title),
                content: Some(h.content),
                ..Default::default()
            },
        )
    }

    /// Drop all local history rows for a note (UI "clear history").
    pub fn purge_history(&self, note_id: &Id) -> rusqlite::Result<usize> {
        self.conn
            .execute("DELETE FROM note_history WHERE NoteId=?1", [note_id.to_string()])
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

    /// JS leanote_data findNote+resolveNote: exact id first (non-trash
    /// only), else prefix matches in insertion order, first live one wins.
    pub fn find_note_by_prefix(&self, prefix: &str) -> rusqlite::Result<Option<Note>> {
        if let Some(n) = self.get_note(&Id(prefix.to_string()))? {
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

    /// JS deleteNote (leanote_data.js): trash + local-delete flags, never a
    /// hard delete, dirty so sync pushes it.
    pub fn trash_note(&self, id: &Id) -> rusqlite::Result<usize> {
        self.conn.execute(
            "UPDATE notes SET IsTrash=1, LocalIsDelete=1, IsDirty=1, UpdatedTime=?2 WHERE NoteId=?1",
            params![id.to_string(), now_ms()],
        )
    }

    /// Live notebook by exact title for this user (MCP list/update filters).
    pub fn find_notebook_by_title(&self, title: &str) -> rusqlite::Result<Option<Notebook>> {
        let mut stmt = self.conn.prepare(
            "SELECT NotebookId, UserId, Title, ParentNotebookId, Seq, LocalIsNew, IsDirty, LocalIsDelete
             FROM notebooks WHERE UserId=?1 AND Title=?2 AND LocalIsDelete=0 ORDER BY rowid",
        )?;
        stmt.query_row(
            params![self.user_id, title],
            |r| {
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
            },
        )
        .optional()
    }

    /// MCP update_note "notebook" move (JS updateNoteFields): the /api
    /// updateNote path intentionally ignores notebook moves, MCP honors them.
    pub fn set_notebook(&self, id: &Id, notebook_id: &Id) -> rusqlite::Result<usize> {
        self.conn.execute(
            "UPDATE notes SET NotebookId=?2, IsDirty=1, UpdatedTime=?3 WHERE NoteId=?1",
            params![id.to_string(), notebook_id.to_string(), now_ms()],
        )
    }


    // ---- Ticket 11: notebook & tag management (offline, sync-pushed) ----

    /// JS Notebook.addNotebook: new row, LocalIsNew+IsDirty (pushed on the
    /// next sync). Seq = max sibling + 1 so it lands last in its group —
    /// the visible order the user expects, unlike the JS tree's Seq:-1.
    pub fn create_notebook(&self, title: &str, parent_id: &str) -> rusqlite::Result<Notebook> {
        let tx = self.conn.unchecked_transaction()?;
        let max_seq: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(Seq), -1) FROM notebooks
                 WHERE UserId=?1 AND ParentNotebookId=?2",
                params![self.user_id, parent_id],
                |r| r.get(0),
            )
            .unwrap_or(-1);
        let id = Id::new();
        let now = now_ms();
        tx.execute(
            "INSERT INTO notebooks (NotebookId, UserId, Title, ParentNotebookId, Seq, Usn,
                LocalIsNew, IsDirty, LocalIsDelete, CreatedTime, UpdatedTime)
             VALUES (?1,?2,?3,?4,?5,0,1,1,0,?6,?6)",
            params![id.to_string(), self.user_id, title, parent_id, max_seq + 1, now],
        )?;
        tx.commit()?;
        Ok(self
            .notebooks()?
            .into_iter()
            .find(|nb| nb.notebook_id == id)
            .expect("just inserted"))
    }

    /// JS Notebook.updateNotebookTitle: Title + IsDirty + UpdatedTime.
    pub fn rename_notebook(&self, id: &Id, title: &str) -> rusqlite::Result<usize> {
        self.conn.execute(
            "UPDATE notebooks SET Title=?2, IsDirty=1, UpdatedTime=?3 WHERE NotebookId=?1",
            params![id.to_string(), title, now_ms()],
        )
    }

    /// Ticket 11 (ticket text overrides the JS guard): deleting a notebook
    /// sends its notes to trash — IsTrash+LocalIsDelete+IsDirty, exactly
    /// `trash_note`, never a hard delete — so nothing is lost. Child
    /// notebooks are deleted the same soft way (LocalIsDelete+IsDirty; sync
    /// pushes notebook/deleteNotebook, or drops them locally when they were
    /// never pushed).
    pub fn delete_notebook(&self, id: &Id) -> rusqlite::Result<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let now = now_ms();
        // this notebook + every descendant.
        let mut all = vec![id.to_string()];
        let mut frontier = vec![id.to_string()];
        while let Some(p) = frontier.pop() {
            let kids: Vec<String> = {
                let mut stmt = tx.prepare(
                    "SELECT NotebookId FROM notebooks
                     WHERE ParentNotebookId=?1 AND LocalIsDelete=0",
                )?;
                let v = stmt
                    .query_map([p], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                v
            };
            for child in kids {
                frontier.push(child.clone());
                all.push(child);
            }
        }
        for nb_id in &all {
            // notes → trash (not lost)
            tx.execute(
                "UPDATE notes SET IsTrash=1, LocalIsDelete=1, IsDirty=1, UpdatedTime=?2
                 WHERE NotebookId=?1 AND IsTrash=0",
                params![nb_id, now],
            )?;
            tx.execute(
                "UPDATE notebooks SET LocalIsDelete=1, IsDirty=1, UpdatedTime=?2 WHERE NotebookId=?1",
                params![nb_id, now],
            )?;
        }
        tx.commit()?;
        Ok(all.len())
    }

    /// JS Notebook.dragNotebooks: the moved notebook plus EVERY sibling in
    /// both the old and the new parent group gets ParentNotebookId/Seq
    /// rewritten, IsDirty=1. Either pass the full new sibling order for the
    /// target group (`sibling_order` containing `id`), or an empty order and
    /// `new_index` to insert among the existing siblings.
    pub fn move_notebook(
        &self,
        id: &Id,
        new_parent: &str,
        new_index: usize,
        sibling_order: &[String],
    ) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let now = now_ms();
        let old_parent: String = tx
            .query_row(
                "SELECT ParentNotebookId FROM notebooks WHERE NotebookId=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_default();

        let rewrite = |group_parent: &str, order: &[String]| -> rusqlite::Result<()> {
            for (seq, nb) in order.iter().enumerate() {
                tx.execute(
                    "UPDATE notebooks SET ParentNotebookId=?2, Seq=?3, IsDirty=1, UpdatedTime=?4
                     WHERE NotebookId=?1",
                    params![nb, group_parent, seq as i64, now],
                )?;
            }
            Ok(())
        };

        // Old group minus the moved notebook, re-packed 0..n-1.
        let old_rest: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT NotebookId FROM notebooks
                 WHERE ParentNotebookId=?1 AND NotebookId<>?2 AND LocalIsDelete=0
                 ORDER BY Seq, rowid",
            )?;
            let v = stmt
                .query_map(params![old_parent, id.to_string()], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            v
        };
        rewrite(&old_parent, &old_rest)?;

        // New group: caller-provided order, or insert at new_index.
        if sibling_order.iter().any(|s| s == &id.to_string()) {
            rewrite(new_parent, sibling_order)?;
        } else {
            let mut group: Vec<String> = {
                let mut stmt = tx.prepare(
                    "SELECT NotebookId FROM notebooks
                     WHERE ParentNotebookId=?1 AND LocalIsDelete=0 ORDER BY Seq, rowid",
                )?;
                let v = stmt
                    .query_map([new_parent], |r| r.get(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                v
            };
            let at = new_index.min(group.len());
            group.insert(at, id.to_string());
            rewrite(new_parent, &group)?;
        }
        tx.commit()
    }

    /// Ticket 11: rename a tag globally — every note's Tags CSV rewritten
    /// (whole-tag match only), affected notes IsDirty, tag row retitled
    /// (IsDirty stays set, so the add of the new name pushes next sync).
    /// JS never shipped a global rename; the Electron UI edits per-note tags.
    pub fn rename_tag(&self, old: &str, new: &str) -> rusqlite::Result<usize> {
        if old == new || new.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        let now = now_ms();
        let rows: Vec<(String, String)> = {
            let mut stmt = tx.prepare("SELECT NoteId, Tags FROM notes WHERE LocalIsDelete=0")?;
            let v = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            v
        };
        let mut changed = 0;
        for (note_id, csv) in rows {
            let mut tags: Vec<String> =
                csv.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
            if !tags.iter().any(|t| t == old) {
                continue;
            }
            for t in tags.iter_mut() {
                if t == old {
                    *t = new.to_string();
                }
            }
            tx.execute(
                "UPDATE notes SET Tags=?2, IsDirty=1, UpdatedTime=?3 WHERE NoteId=?1",
                params![note_id, tags.join(","), now],
            )?;
            changed += 1;
        }
        // retitle the tag row; JS re-adds the new name on next use
        let n = tx.execute(
            "UPDATE tags SET Tag=?2, UpdatedTime=?3, LocalIsDelete=0, IsDirty=1
             WHERE UserId=?4 AND Tag=?1",
            params![old, new, now, self.user_id],
        )?;
        if n == 0 {
            tx.execute(
                "INSERT INTO tags (TagId, UserId, Tag, IsDirty, Count, LocalIsDelete, CreatedTime, UpdatedTime)
                 VALUES (?1,?2,?3,1,1,0,?4,?4)",
                params![Id::new().to_string(), self.user_id, new, now],
            )?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// JS Tag.deleteTag + Note.updateNoteToDeleteTag: tag row soft-deleted
    /// (LocalIsDelete+IsDirty → tag/deleteTag push), every note's Tags CSV
    /// loses the tag and goes IsDirty. Returns notes touched.
    pub fn delete_tag(&self, tag: &str) -> rusqlite::Result<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let now = now_ms();
        let rows: Vec<(String, String)> = {
            let mut stmt = tx.prepare("SELECT NoteId, Tags FROM notes WHERE LocalIsDelete=0")?;
            let v = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            v
        };
        let mut changed = 0;
        for (note_id, csv) in rows {
            let mut tags: Vec<String> =
                csv.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
            let before = tags.len();
            tags.retain(|t| t != tag);
            if tags.len() == before {
                continue;
            }
            tx.execute(
                "UPDATE notes SET Tags=?2, IsDirty=1, UpdatedTime=?3 WHERE NoteId=?1",
                params![note_id, tags.join(","), now],
            )?;
            changed += 1;
        }
        tx.execute(
            "UPDATE tags SET LocalIsDelete=1, IsDirty=1, UpdatedTime=?2 WHERE UserId=?3 AND Tag=?1",
            params![tag, now, self.user_id],
        )?;
        tx.commit()?;
        Ok(changed)
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
CREATE TABLE IF NOT EXISTS sync_conflicts (
    ServerNoteId TEXT PRIMARY KEY,
    LocalNoteId TEXT NOT NULL,
    ServerNote TEXT NOT NULL,
    LocalTitle TEXT NOT NULL,
    LocalContent TEXT NOT NULL,
    ServerContent TEXT NOT NULL,
    CreatedTime INTEGER NOT NULL DEFAULT 0,
    Resolved INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS note_history (
    HistoryId TEXT PRIMARY KEY,
    NoteId TEXT NOT NULL,
    UserId TEXT NOT NULL,
    Title TEXT NOT NULL DEFAULT '',
    Content TEXT NOT NULL,
    CreatedTime INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(UpdatedTime DESC);
CREATE INDEX IF NOT EXISTS idx_notes_notebook ON notes(NotebookId);
CREATE INDEX IF NOT EXISTS idx_notebooks_user_title ON notebooks(UserId, Title);
CREATE INDEX IF NOT EXISTS idx_note_history_note ON note_history(NoteId, CreatedTime DESC);
";

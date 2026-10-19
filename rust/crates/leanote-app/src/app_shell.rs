//! The main window: login form (right pane) → full-sync progress → Wake
//! three-pane shell (ADR-0003). Ticket 09: selectable nav + filtered list +
//! read-only Markdown reader, star toggle, trash actions.

use crate::browse::{self, NavNode};
use crate::conflict_bar;
use crate::i18n::{self, Lang};
use crate::theme::{self, Material};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::label::Label;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::assets::IconName as AssetIcon;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::{px, AnyElement, KeyDownEvent, SharedString, Task, Window};
use leanote_core::sync::{Api, SyncProgress, SyncStats};
use std::sync::{Arc, Mutex};

/// ADR-0003 fixed widths.
const LEFT_W: f32 = 224.;
const MID_W: f32 = 336.;
const READER_W: f32 = 720.;
/// macOS transparent titlebar height, same material as the sidebar.
const TITLEBAR_H: f32 = 44.;

pub enum Phase {
    Login,
    SigningIn,
    /// Progress lines from full_sync, newest last.
    Syncing { lines: Vec<SharedString> },
    /// Sync done → three-pane shell.
    Shell,
    /// Login or sync failed → back to login with an error line.
    Failed { error: SharedString },
}

#[derive(Clone, Copy)]
enum Group {
    Notebooks,
    Tags,
}

/// Which conflict-bar button was pressed (ticket 14).
#[derive(Clone, Copy)]
enum ConflictChoice {
    UseLocal,
    UseServer,
    KeepBoth,
}

/// In-flight nav edit (ticket 11): one at a time.
enum NavEdit {
    /// Create a notebook under this parent ("" = root).
    NewNotebook { parent: String, name: gpui_kit::Entity<InputState> },
    /// Rename this notebook.
    RenameNotebook { id: leanote_core::Id, name: gpui_kit::Entity<InputState> },
    /// Move this notebook: picking the target parent.
    MoveNotebook { id: leanote_core::Id },
    /// Rename this tag.
    RenameTag { old: String, name: gpui_kit::Entity<InputState> },
}

/// What the middle list is filtered to (left-nav selection).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavSelection {
    AllNotes,
    Starred,
    Trash,
    Notebook(String),
    Tag(String),
}

pub struct AppShell {
    phase: Phase,
    lang: Lang,
    data_dir: std::path::PathBuf,
    host: gpui_kit::Entity<InputState>,
    email: gpui_kit::Entity<InputState>,
    password: gpui_kit::Entity<InputState>,
    /// Login+sync futures; dropped (cancelled) with the entity.
    _task: Option<Task<()>>,
    // --- Ticket 09 browse state ---
    nav: NavSelection,
    notebooks_open: bool,
    tags_open: bool,
    /// Note currently shown in the reader (kept across list re-filters
    /// until the selection changes).
    selected: Option<leanote_core::Id>,
    /// Ticket 15: export outcome shown inline in the reader toolbar
    /// (e.g. PDF-fell-back-to-HTML reason). Cleared on selection change.
    export_status: Option<SharedString>,
    /// Ticket 13: ⌘K search overlay (dialog-hosted palette + last rows).
    search: crate::search_overlay::SearchOverlay,
    search_rows: Vec<crate::search_overlay::ResultRow>,
    // --- Ticket 12 rich-text state ---
    /// Set after a successful Convert to Markdown; cleared when the
    /// selection changes. Rendered as a one-line feedback next to the badge.
    converted_note: Option<leanote_core::Id>,
    // --- Ticket 11 nav management ---
    nav_edit: Option<NavEdit>,
    // --- Ticket 10 editor state ---
    /// Edit mode for `selected` (only entered for live IsMarkdown notes).
    editing: Option<EditState>,
    /// History popover open (only meaningful while editing).
    history_open: bool,
    /// Which history row is previewed (None = list view).
    history_preview: Option<leanote_core::Id>,
}

/// Right-pane edit mode (ticket 10): title + multiline source editor with
/// preview toggle. Buffer lives in the kit's TextareaState so IME
/// composition is handled entirely by the kit.
struct EditState {
    note_id: leanote_core::Id,
    title: gpui_kit::Entity<InputState>,
    content: gpui_kit::Entity<TextareaState>,
    /// Show rendered preview instead of source.
    preview: bool,
    /// Set for one frame after a save, to flash "Saved".
    saved_flash: bool,
}

/// Progress sink that forwards full_sync lines into a channel the UI drains.
struct ChannelProgress(Arc<Mutex<smol::channel::Sender<String>>>);

impl SyncProgress for ChannelProgress {
    fn note(&self, msg: String) {
        // ponytail: try_send, dropped lines only cost progress granularity
        let _ = self.0.lock().unwrap().try_send(msg);
    }
}

impl AppShell {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
        data_dir: std::path::PathBuf,
        lang: Lang,
        signed_in: bool,
        fake_prefill: Option<(&str, &str, &str)>,
    ) -> Self {
        let host = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("https://leanote.com");
            if let Some((h, _, _)) = fake_prefill {
                s = s.default_value(h);
            }
            s
        });
        let email = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("user@example.com");
            if let Some((_, e, _)) = fake_prefill {
                s = s.default_value(e);
            }
            s
        });
        let password = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("••••••••").masked(true);
            if let Some((_, _, p)) = fake_prefill {
                s = s.default_value(p);
            }
            s
        });
        // Enter in the password field submits.
        let pw = password.clone();
        cx.subscribe(&pw, |this, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.start_login(cx);
            }
        })
        .detach();

        Self {
            // Returning user: straight to the shell (startup incr-sync is
            // ticket 04 territory, not this one).
            phase: if signed_in {
                Phase::Shell
            } else {
                Phase::Login
            },
            lang,
            data_dir,
            host,
            email,
            password,
            _task: None,
            nav: NavSelection::AllNotes,
            notebooks_open: true,
            tags_open: true,
            selected: None,
            export_status: None,
            search: crate::search_overlay::SearchOverlay::new(window, cx),
            search_rows: Vec::new(),
            converted_note: None,
            nav_edit: None,
            editing: None,
            history_open: false,
            history_preview: None,
        }
    }

    pub fn start_login(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let (host, email, password) = (
            self.host.read(cx).value().to_string(),
            self.email.read(cx).value().to_string(),
            self.password.read(cx).value().to_string(),
        );
        if host.is_empty() || email.is_empty() || password.is_empty() {
            return;
        }
        if !matches!(self.phase, Phase::Login | Phase::Failed { .. }) {
            return; // already in flight or done
        }
        self.phase = Phase::SigningIn;
        cx.notify();
        self.run_login_sync(host, email, password, cx);
    }

    /// Login → full_sync on a background thread; progress streams back.
    fn run_login_sync(
        &mut self,
        host: String,
        email: String,
        password: String,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let (tx, rx) = smol::channel::unbounded::<String>();
        let tx = Arc::new(Mutex::new(tx));
        let dir = self.data_dir.clone();
        let progress = ChannelProgress(tx);

        // Login + sync are blocking ureq calls — off the UI thread.
        let work = cx.background_spawn(async move {
            let result: Result<SyncStats, String> = (|| {
                let Ok(store) = leanote_core::Store::open(dir.clone(), "user1".into()) else {
                    return Err("cannot open local store".into());
                };
                let account = Api::login(&host, &email, &password, &store).map_err(|e| e.to_string())?;
                // Re-open the store keyed to the real user so synced rows land
                // under the account that owns them.
                let Ok(store) = leanote_core::Store::open(dir, account.user_id.clone()) else {
                    return Err("cannot open local store".into());
                };
                let api = Api::for_account(&account);
                leanote_core::sync::full_sync(&api, &store, &progress).map_err(|e| e.to_string())
            })();
            result
        });

        // Await the work on the app thread; stream progress meanwhile.
        // The channel closes when the background task drops its sender, so
        // recv() ends exactly when the work does — one drain, no extra spawn.
        self._task = Some(cx.spawn(async move |this, cx| {
            let mut lines: Vec<SharedString> = Vec::new();
            let result = loop {
                // Poll the channel to completion; the sender dies with `work`.
                match rx.recv().await {
                    Ok(msg) => {
                        lines.push(msg.into());
                        if lines.len() > 200 {
                            lines.remove(0); // keep the tail
                        }
                        let snapshot = lines.clone();
                        let _ = this.update(cx, |shell, cx| {
                            if matches!(shell.phase, Phase::SigningIn) {
                                shell.phase = Phase::Syncing { lines: snapshot };
                                cx.notify();
                            } else if let Phase::Syncing { lines } = &mut shell.phase {
                                *lines = snapshot;
                                cx.notify();
                            }
                        });
                    }
                    Err(_) => break work.await,
                }
            };
            let _ = this.update(cx, |shell, cx| {
                match result {
                    Ok(_) => {
                        shell.phase = Phase::Shell;
                    }
                    Err(e) => {
                        leanote_core::logging::log("sync", &e);
                        shell.phase = Phase::Failed { error: e.into() };
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn dark(&self, window: &Window) -> bool {
        window.appearance() == gpui_kit::WindowAppearance::Dark
            || window.appearance() == gpui_kit::WindowAppearance::VibrantDark
    }

    pub(crate) fn lang(&self) -> i18n::Lang {
        self.lang
    }

    fn store(&self) -> Option<leanote_core::Store> {
        // Re-open per read: cheap (WAL) and always fresh after star/trash ops.
        leanote_core::Store::open(self.data_dir.clone(), "user1".into()).ok()
    }

    /// Notes for the current nav selection, in list order.
    fn visible_notes(&self) -> Vec<leanote_core::Note> {
        let Some(store) = self.store() else { return Vec::new() };
        let res = match &self.nav {
            NavSelection::AllNotes => store.list_notes_filtered(None, false),
            NavSelection::Starred => store.list_notes_filtered(None, true),
            NavSelection::Trash => store.list_trash(),
            NavSelection::Notebook(id) => {
                store.list_notes_filtered(Some(&leanote_core::Id(id.clone())), false)
            }
            NavSelection::Tag(tag) => store.list_notes_by_tag(tag),
        };
        res.unwrap_or_default()
    }

    fn notebook_names(&self) -> std::collections::HashMap<String, String> {
        self.store()
            .and_then(|s| s.notebooks().ok())
            .map(|nbs| {
                nbs.into_iter()
                    .map(|nb| (nb.notebook_id.to_string(), nb.title))
                    .collect::<std::collections::HashMap<_, _>>()
            })
            .unwrap_or_default()
    }

    /// Nav-tree rows for the Notebooks group (nesting + Seq + counts).
    fn notebook_nav_nodes(&self) -> Vec<NavNode> {
        let Some(store) = self.store() else { return Vec::new() };
        let (Ok(nbs), Ok(count_rows)) = (store.notebooks(), store.notebook_note_counts()) else {
            return Vec::new();
        };
        let counts: std::collections::HashMap<String, i64> =
            count_rows.into_iter().map(|(id, c)| (id.to_string(), c)).collect();
        browse::notebook_nodes(
            &nbs.iter()
                .map(|nb| (nb.notebook_id.to_string(), nb.title.clone(), nb.parent_notebook_id.clone(), nb.seq))
                .collect::<Vec<_>>(),
            &counts,
        )
    }

    /// Nav rows for the Tags group: tag name → live note count via
    /// list_notes_by_tag (the tags table's Count stays 1 after sync).
    fn tag_nav_nodes(&self) -> Vec<NavNode> {
        let Some(store) = self.store() else { return Vec::new() };
        let Ok(tags) = store.tags() else { return Vec::new() };
        tags.into_iter()
            .filter(|t| !t.is_deleted)
            .map(|t| NavNode {
                count: store.list_notes_by_tag(&t.tag).map(|n| n.len() as i64).unwrap_or(0),
                id: t.tag.clone(),
                title: format!("# {}", t.tag),
                depth: 0,
            })
            .collect()
    }

    fn select_nav(&mut self, nav: NavSelection, cx: &mut gpui_kit::Context<Self>) {
        self.nav = nav;
        self.selected = None; // new filter → no stale reader content
        self.close_editor();
        cx.notify();
    }

    fn select_note(&mut self, id: leanote_core::Id, cx: &mut gpui_kit::Context<Self>) {
        self.selected = Some(id);
        self.converted_note = None;
        self.close_editor();
        cx.notify();
    }


    fn close_editor(&mut self) {
        self.editing = None;
        self.history_open = false;
        self.history_preview = None;
    }

    /// Enter edit mode for the selected live markdown note. Buffer is seeded
    /// from the store; all editing (incl. IME composition) stays inside the
    /// kit's input state — this code never mutates the buffer on key events.
    fn start_editing(
        &mut self,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let Some(id) = self.selected.clone() else { return };
        let Some(note) = self.store().and_then(|s| s.get_note_any(&id).ok().flatten()) else {
            return;
        };
        if !note.is_markdown || note.is_trash || note.is_deleted {
            return;
        }
        let title = cx.new(|cx| InputState::new(window, cx).default_value(note.title.clone()));
        let content =
            cx.new(|cx| TextareaState::new(window, cx).default_value(note.content.clone()));
        self.editing = Some(EditState {
            note_id: id,
            title,
            content,
            preview: false,
            saved_flash: false,
        });
        self.history_open = false;
        self.history_preview = None;
        cx.notify();
    }

    /// Save button / ⌘S: write through save_note_with_history (snapshot of
    /// the pre-change content, IsDirty=1 so sync pushes it).
    fn save_editor(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let Some(ed) = &self.editing else { return };
        let title = ed.title.read(cx).value().to_string();
        let content = ed.content.read(cx).value().to_string();
        let id = ed.note_id.clone();
        if let Some(store) = self.store() {
            let _ = store.save_note_with_history(
                &id,
                leanote_core::NoteUpdate {
                    title: Some(title),
                    content: Some(content),
                    ..Default::default()
                },
            );
        }
        if let Some(ed) = &mut self.editing {
            ed.saved_flash = true;
        }
        cx.notify();
    }

    /// Leave edit mode without saving.
    fn stop_editing(&mut self, cx: &mut gpui_kit::Context<Self>) {
        self.close_editor();
        cx.notify();
    }

    /// New note in the current nav context's notebook (IsNew=1, dirty) and
    /// open it straight in the editor.
    fn new_note(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        // Notebook context: current notebook nav, else the first notebook.
        let store = match self.store() {
            Some(s) => s,
            None => return,
        };
        let notebook_title = match &self.nav {
            NavSelection::Notebook(id) => store
                .notebook_title(id)
                .unwrap_or_else(|_| id.to_string()),
            _ => store
                .notebooks()
                .unwrap_or_default()
                .into_iter()
                .next()
                .map(|nb| nb.title)
                .unwrap_or_else(|| "Inbox".into()),
        };
        let Ok(note) = store.add_note(leanote_core::NewNote {
            title: String::new(),
            content: String::new(),
            tags: vec![],
            notebook_title,
            is_markdown: true,
        }) else {
            return;
        };
        match &self.nav {
            NavSelection::Notebook(_) => {}
            _ => self.nav = NavSelection::AllNotes,
        }
        self.selected = Some(note.note_id.clone());
        self.close_editor();
        cx.notify();
        self.start_editing(window, cx);
    }
    fn restore_history_version(
        &mut self,
        history_id: leanote_core::Id,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let Some(ed) = &self.editing else { return };
        let note_id = ed.note_id.clone();
        let Some(store) = self.store() else { return };
        if store.restore_history(&note_id, &history_id).ok().flatten().is_some() {
            self.close_editor();
            self.selected = Some(note_id);
            self.start_editing(window, cx); // reopen seeded with restored content
        }
    }

    /// Ticket 13: ⌘K toggles the overlay from any phase (dialog layer is
    /// window-level, not pane-level).
    pub fn toggle_search(&self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        self.search.toggle(&cx.entity(), window, cx);
    }

    /// Overlay rows as of the last committed query (re-render source).
    pub(crate) fn overlay_rows(&self) -> &[crate::search_overlay::ResultRow] {
        &self.search_rows
    }

    /// Committed query → unified core search → rows. IME-safe: the palette
    /// only calls this on `InputEvent::Change` (committed text).
    pub(crate) fn run_overlay_search(&mut self, query: &str, cx: &mut gpui_kit::Context<Self>) {
        self.search_rows = if crate::search_overlay::should_search(query, false) {
            self.search_store(query)
        } else {
            Vec::new()
        };
        cx.notify();
    }

    /// ↩ on a row: open the note in the reader (`selected` drives it), then
    /// close the overlay. The middle list keeps its filter/nav untouched.
    pub(crate) fn confirm_overlay(
        &mut self,
        index: gpui_kit::component::IndexPath,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Some(row) = self.search_rows.get(index.row) {
            let id = leanote_core::Id(row.note_id.clone());
            self.select_note(id, cx);
        }
        window.close_dialog(cx);
    }

    fn search_store(&self, query: &str) -> Vec<crate::search_overlay::ResultRow> {
        let Some(store) = self.store() else { return Vec::new() };
        let hits = store.search(query, crate::search_overlay::RESULT_LIMIT).unwrap_or_default();
        crate::search_overlay::format_rows(&hits, |id| {
            store.notebook_title(id).unwrap_or_else(|_| id.to_string())
        })
    }

    /// Star is local-only: toggle_star never touches IsDirty (GLOSSARY).
    fn toggle_star(&mut self, id: leanote_core::Id, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.toggle_star(&id);
        }
        cx.notify();
    }

    /// Ticket 14 conflict bar actions. All three clear the conflict row; the
    /// store methods decide what happens to the note itself.
    fn resolve_conflict(
        &mut self,
        how: ConflictChoice,
        server_note_id: String,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Some(store) = self.store() {
            match how {
                ConflictChoice::UseLocal => {
                    let _ = store.resolve_conflict_use_local(&server_note_id);
                }
                ConflictChoice::UseServer => {
                    let _ = store.resolve_conflict_use_server(&server_note_id);
                }
                ConflictChoice::KeepBoth => {
                    let suffix = conflict_bar::keep_both_suffix(self.lang);
                    if let Ok(Some(new_id)) =
                        store.resolve_conflict_keep_both(&server_note_id, suffix)
                    {
                        self.selected = Some(new_id);
                    }
                }
            }
        }
        cx.notify();
    }

    fn restore_selected(&mut self, id: leanote_core::Id, cx: &mut gpui_kit::Context<Self>) {
        // Back to its ORIGINAL notebook (JS moveNote on a trashed note keeps
        // NotebookId; restore_note only clears trash flags + marks dirty).
        if let Some(store) = self.store() {
            if let Ok(Some(n)) = store.get_note_any(&id) {
                let _ = store.restore_note(&id, &n.notebook_id);
            }
        }
        self.selected = None;
        cx.notify();
    }

    fn hard_delete_selected(&mut self, id: leanote_core::Id, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.hard_delete_note(&id);
        }
        self.selected = None;
        cx.notify();
    }

    fn empty_trash(&mut self, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.empty_trash();
        }
        self.selected = None;
        cx.notify();
    }

    // --- Ticket 15: export HTML / Markdown / PDF / .leanote backup ---

    /// Native save panel → run `f` off the UI thread → outcome lands in
    /// `export_status` (label under the toolbar). Cancel = no-op.
    fn export_with_panel(
        &mut self,
        suggested: String,
        cx: &mut gpui_kit::Context<Self>,
        f: impl FnOnce(std::path::PathBuf) -> Result<(), String> + Send + 'static,
    ) {
        let rx = cx.prompt_for_new_path(&home_dir(), Some(&suggested));
        self._task = Some(cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            let done = cx.background_spawn(async move { f(path.clone()).map(|()| path) });
            let status: SharedString = match done.await {
                Ok(path) => path.display().to_string().into(),
                Err(reason) => reason.into(),
            };
            let _ = this.update(cx, |shell, cx| {
                shell.export_status = Some(status);
                cx.notify();
            });
        }));
    }

    /// Selected note → HTML (`md=false`) or Markdown (`md=true`) file.
    fn export_selected_file(&mut self, md: bool, cx: &mut gpui_kit::Context<Self>) {
        let Some(n) = self.selected_note() else { return };
        let name = format!(
            "{}.{}",
            crate::export::safe_filename(&n.title),
            if md { "md" } else { "html" }
        );
        self.export_with_panel(name, cx, move |path| {
            let body = if md {
                crate::export::note_to_markdown_file(&n)
            } else {
                crate::export::note_to_html(&n)
            };
            std::fs::write(&path, body).map_err(|e| e.to_string())
        });
    }

    /// Selected note → PDF via the server's exportPdf endpoint (src/api.js
    /// authority). Any failure — not synced, no account, transport, server
    /// error — falls back to a local HTML export with the reason shown.
    fn export_selected_pdf(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let Some(n) = self.selected_note() else { return };
        let (server_id, account) = match self.store() {
            Some(store) => (
                store.note_server_id(&n.note_id.to_string()).ok().flatten(),
                store.active_account().ok().flatten(),
            ),
            None => (None, None),
        };
        let not_synced = i18n::t(self.lang, i18n::Key::NotSynced).to_string();
        let fell_back = i18n::t(self.lang, i18n::Key::PdfFellBack).to_string();
        let name = format!("{}.pdf", crate::export::safe_filename(&n.title));
        self.export_with_panel(name, cx, move |path| {
            let pdf = match (&server_id, &account) {
                (Some(sid), Some(acc)) => Some(Api::for_account(acc).export_pdf(sid)),
                _ => None,
            };
            match pdf {
                Some(Ok(bytes)) => std::fs::write(&path, bytes).map_err(|e| e.to_string()),
                _ => {
                    // Fallback: HTML at the .pdf path with extension swapped.
                    let reason = if server_id.is_none() {
                        not_synced.clone()
                    } else {
                        fell_back.clone()
                    };
                    std::fs::write(
                        path.with_extension("html"),
                        crate::export::note_to_html(&n),
                    )
                    .map_err(|e| e.to_string())?;
                    Err(reason)
                }
            }
        });
    }

    /// Whole library → one `.leanote` JSON backup (JS export-leanote plugin
    /// shape; notebooks + every live note).
    fn export_backup(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let name = "leanote-backup.leanote".to_string();
        let dir = self.data_dir.clone();
        let user = "user1".to_string();
        self.export_with_panel(name, cx, move |path| {
            let store = leanote_core::Store::open(dir, user).map_err(|e| e.to_string())?;
            std::fs::write(&path, crate::export::backup_to_leanote_json(&store))
                .map_err(|e| e.to_string())
        });
    }

    fn selected_note(&self) -> Option<leanote_core::Note> {
        let id = self.selected.as_ref()?;
        self.store().and_then(|s| s.get_note_any(id).ok().flatten())
    }
}


/// Save-panel default directory (std only).
fn home_dir() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

impl AppShell {
    /// Ticket 12: one-way HTML → Markdown conversion of the selected
    /// rich-text note. Persisted (content rewritten, IsMarkdown=1, dirty)
    /// so the next push sends Markdown to the server; the reader re-renders
    /// straight from the converted store row.
    fn convert_selected_to_markdown(
        &mut self,
        id: leanote_core::Id,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Some(store) = self.store() {
            if matches!(store.convert_to_markdown(&id), Ok(Some(_))) {
                self.converted_note = Some(id);
            }
        }
        cx.notify();
    }
}

impl AppShell {
    // ---- Ticket 11: nav management actions (context menus + inline input) ----

    fn begin_new_notebook(
        &mut self,
        parent: String,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let name = cx.new(|cx| InputState::new(window, cx));
        self.nav_edit = Some(NavEdit::NewNotebook { parent, name });
        cx.notify();
    }

    fn begin_rename_notebook(
        &mut self,
        id: leanote_core::Id,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let name = cx.new(|cx| InputState::new(window, cx));
        self.nav_edit = Some(NavEdit::RenameNotebook { id, name });
        cx.notify();
    }

    fn begin_move_notebook(&mut self, id: leanote_core::Id, cx: &mut gpui_kit::Context<Self>) {
        self.nav_edit = Some(NavEdit::MoveNotebook { id });
        cx.notify();
    }

    fn begin_rename_tag(
        &mut self,
        old: String,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let name = cx.new(|cx| InputState::new(window, cx));
        self.nav_edit = Some(NavEdit::RenameTag { old, name });
        cx.notify();
    }

    /// Apply the in-flight edit (Enter / OK button). Empty input no-ops.
    fn apply_nav_edit(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let edit = self.nav_edit.take();
        match edit {
            Some(NavEdit::NewNotebook { parent, name }) => {
                let title = name.read(cx).value().to_string();
                let title = title.trim();
                if !title.is_empty() {
                    if let Some(store) = self.store() {
                        let _ = store.create_notebook(title, &parent);
                    }
                }
            }
            Some(NavEdit::RenameNotebook { id, name }) => {
                let title = name.read(cx).value().to_string();
                let title = title.trim();
                if !title.is_empty() {
                    if let Some(store) = self.store() {
                        let _ = store.rename_notebook(&id, title);
                    }
                }
            }
            Some(NavEdit::RenameTag { old, name }) => {
                let new = name.read(cx).value().to_string();
                let new = new.trim();
                if !new.is_empty() && new != old {
                    if let Some(store) = self.store() {
                        let _ = store.rename_tag(&old, new);
                    }
                    if self.nav == NavSelection::Tag(old.clone()) {
                        self.nav = NavSelection::Tag(new.to_string());
                    }
                }
            }
            // MoveNotebook resolves directly in move_notebook_to
            Some(NavEdit::MoveNotebook { .. }) | None => {}
        }
        cx.notify();
    }

    /// Context-menu "Move to…" target chosen: reparent as the last child.
    /// (Full drag-reorder is the drag-drop ticket's UI; Seq rewriting and
    /// the sibling-group push semantics are identical either way.)
    fn move_notebook_to(&mut self, id: leanote_core::Id, parent: String, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.move_notebook(&id, &parent, usize::MAX, &[]);
        }
        self.nav_edit = None;
        cx.notify();
    }

    fn delete_notebook_nav(&mut self, id: leanote_core::Id, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.delete_notebook(&id);
        }
        if self.nav == NavSelection::Notebook(id.to_string()) {
            self.nav = NavSelection::AllNotes;
            self.selected = None;
        }
        cx.notify();
    }

    fn delete_tag_nav(&mut self, tag: String, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.delete_tag(&tag);
        }
        if self.nav == NavSelection::Tag(tag) {
            self.nav = NavSelection::AllNotes;
            self.selected = None;
        }
        cx.notify();
    }
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let dark = self.dark(window);
        let lang = self.lang;
        let t = move |k: i18n::Key| i18n::t(lang, k);
        let base_bg = theme::material(Material::Base, dark);
        let list_bg = theme::material(Material::List, dark);

        // Whole window: material[0] shows through the 44px transparent titlebar.
        let root = gpui_kit::base::v_flex()
            .size_full()
            .bg(base_bg)
            .text_color(cx.theme().foreground)
            .on_action(
                cx.listener(|this, _: &crate::search_overlay::ToggleSearch, window, cx| {
                    // Any phase: the dialog layer is window-level.
                    this.toggle_search(window, cx);
                }),
            );

        // 44px strip shares sidebar material — drawn by the root bg.
        let titlebar = gpui_kit::base::h_flex().h(px(TITLEBAR_H)).w_full().flex_none();

        let mut body = gpui_kit::base::h_flex().flex_1().min_h_0().w_full();

        match &self.phase {
            Phase::Login | Phase::SigningIn | Phase::Failed { .. } => {
                // Not signed in: right pane IS the login form (spec §1). Left
                // + middle render as empty material so the window reads as
                // the three-pane app from the first frame.
                let error = match &self.phase {
                    Phase::Failed { error } => Some(error.clone()),
                    _ => None,
                };
                let busy = matches!(self.phase, Phase::SigningIn);
                // Built here: on_click needs cx.listener (Context of Self).
                let sign_in = if busy {
                    Button::new("sign-in").label(t(i18n::Key::SigningIn)).primary().disabled(true)
                } else {
                    Button::new("sign-in")
                        .label(t(i18n::Key::SignIn))
                        .primary()
                        .on_click(cx.listener(|this, _ev, _window, cx| this.start_login(cx)))
                };
                body = body
                    .child(gpui_kit::base::v_flex().w(px(LEFT_W)).h_full().flex_none().bg(base_bg))
                    .child(gpui_kit::base::v_flex().w(px(MID_W)).h_full().flex_none().bg(list_bg))
                    .child(render_login(dark, error, sign_in, &t, &self.host, &self.email, &self.password));
            }
            Phase::Syncing { lines } => {
                body = body
                    .child(gpui_kit::base::v_flex().w(px(LEFT_W)).h_full().flex_none().bg(base_bg))
                    .child(gpui_kit::base::v_flex().w(px(MID_W)).h_full().flex_none().bg(list_bg))
                    .child(render_syncing(dark, lines, &t));
            }
            Phase::Shell { .. } => {
                body = body
                    .child(self.render_nav(dark, &t, window, cx))
                    .child(self.render_list(dark, &t, window, cx))
                    .child(self.render_reader(dark, &t, window, cx));
            }
        }

        root.child(titlebar).child(body)
    }
}

fn small_label(text: impl Into<SharedString>, color: gpui_kit::Hsla) -> Label {
    Label::new(text).text_size(px(13.)).text_color(color)
}

fn render_login(
    dark: bool,
    error: Option<SharedString>,
    sign_in: gpui_kit::component::button::Button,
    t: &dyn Fn(i18n::Key) -> &'static str,
    host: &gpui_kit::Entity<InputState>,
    email: &gpui_kit::Entity<InputState>,
    password: &gpui_kit::Entity<InputState>,
) -> impl IntoElement + use<> {
    let fg = theme::foreground(dark);
    let muted = theme::muted_foreground(dark);
    let title = Label::new(t(i18n::Key::LoginTitle)).text_size(px(22.)).text_color(fg);
    let subtitle = small_label(t(i18n::Key::LoginSubtitle), muted);

    let host_field = gpui_kit::base::v_flex()
        .gap_1()
        .child(small_label(t(i18n::Key::Host), muted))
        .child(Input::new(host).appearance(true).bordered(true));
    let email_field = gpui_kit::base::v_flex()
        .gap_1()
        .child(small_label(t(i18n::Key::Email), muted))
        .child(Input::new(email).appearance(true).bordered(true));
    let password_field = gpui_kit::base::v_flex()
        .gap_1()
        .child(small_label(t(i18n::Key::Password), muted))
        .child(Input::new(password).appearance(true).bordered(true));

    let register = gpui_kit::component::link::Link::new("register")
        .href("https://leanote.com/register")
        .child(small_label(t(i18n::Key::Register), gpui_kit::hsla(0., 0., 1., 0.)))
        .on_click(|_ev, _window, cx| cx.open_url("https://leanote.com/register"));

    let mut form = gpui_kit::base::v_flex()
        .gap_3()
        .w(px(360.))
        .child(title)
        .child(subtitle)
        .child(host_field)
        .child(email_field)
        .child(password_field)
        .child(sign_in)
        .child(register);
    if let Some(err) = error {
        form = form.child(
            Label::new(format!("{} / {}: {}", t(i18n::Key::LoginFailed), t(i18n::Key::SyncFailed), err))
                .text_size(px(13.))
                .text_color(gpui_kit::hsla(0. / 360., 0.78, 0.46, 1.)),
        );
    }

    // Right pane: background material with a centered column.
    gpui_kit::base::v_flex()
        .flex_1()
        .h_full()
        .bg(theme::material(Material::Background, dark))
        .items_center()
        .justify_center()
        .child(form)
        .into_element()
}

fn render_syncing(dark: bool, lines: &[SharedString], t: &dyn Fn(i18n::Key) -> &'static str) -> impl IntoElement {
    let muted = theme::muted_foreground(dark);
    let rows = lines
        .iter()
        .rev()
        .take(8)
        .map(|l| small_label(l.clone(), muted))
        .collect::<Vec<_>>();
    gpui_kit::base::v_flex()
        .flex_1()
        .h_full()
        .bg(theme::material(Material::Background, dark))
        .items_start()
        .justify_center()
        .child(
            gpui_kit::base::v_flex()
                .gap_3()
                .w(px(360.))
                .child(
                    gpui_kit::base::h_flex()
                        .gap_2()
                        .child(Spinner::new())
                        .child(Label::new(t(i18n::Key::SyncingTitle)).text_size(px(22.)).text_color(theme::foreground(dark)))
                )
                .child(small_label(if lines.is_empty() { t(i18n::Key::SyncingSubtitle) } else { t(i18n::Key::SyncDone) }, muted))
                .child(gpui_kit::base::v_flex().gap_1().children(rows))
        )
        .into_element()
}

impl AppShell {
    /// One clickable nav row; `id` doubles as the element id.
    fn nav_row(
        &self,
        dark: bool,
        node: &NavNode,
        kind: &str,
        icon: Option<IconName>,
        selected: bool,
        cx: &mut gpui_kit::Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let fg = theme::foreground(dark);
        let sel_fg = theme::accent_foreground();
        let indent = px(12. * node.depth as f32);
        let mut row = gpui_kit::base::h_flex()
            .id(node.id.clone())
            .px_3()
            .h_8()
            .ml(indent)
            .rounded(px(theme::RADIUS_SELECTION))
            .when(selected, |el| el.bg(theme::accent()))
            .on_click({
                let sel: NavSelection = match kind {
                    "notebook" => NavSelection::Notebook(node.id.clone()),
                    "tag" => NavSelection::Tag(node.id.clone()),
                    "starred" => NavSelection::Starred,
                    "trash" => NavSelection::Trash,
                    _ => NavSelection::AllNotes,
                };
                cx.listener(move |this, _ev, _window, cx| this.select_nav(sel.clone(), cx))
            });
        if let Some(icon) = icon {
            row = row.child(
                Icon::new(icon)
                    .xsmall()
                    .text_color(if selected { sel_fg } else { theme::muted_foreground(dark) }),
            );
        }
        row.child(
            Label::new(node.title.clone())
                .text_size(px(14.))
                .text_color(if selected { sel_fg } else { fg }),
        )
        .child(gpui_kit::base::h_flex().flex_1())
        .child(
            small_label(
                node.count.to_string(),
                if selected { sel_fg } else { theme::muted_foreground(dark) },
            )
            .into_any_element(),
        )
    }

    /// Collapsible group header (Notebooks / Tags).
    fn group_header(
        &self,
        dark: bool,
        label: &str,
        _open: bool,
        group: Group,
        cx: &mut gpui_kit::Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let muted = theme::muted_foreground(dark);
        gpui_kit::base::h_flex()
            .id(label.to_string())
            .mt_3()
            .h_6()
            .gap_1()
            .on_click(cx.listener(move |this, _ev, _window, cx| {
                match group {
                    Group::Notebooks => this.notebooks_open = !this.notebooks_open,
                    Group::Tags => this.tags_open = !this.tags_open,
                }
                cx.notify();
            }))
            .child(Icon::new(IconName::ChevronRight).xsmall().text_color(muted))
            .child(Label::new(label).text_size(px(11.)).text_color(muted))
    }

    /// Left nav: identity area + All/Starred/Trash + Notebooks/Tags groups.
    fn render_nav(
        &mut self,
        dark: bool,
        t: &dyn Fn(i18n::Key) -> &'static str,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> gpui_kit::Div {
        let fg = theme::foreground(dark);
        let (notebooks_open, tags_open) = (self.notebooks_open, self.tags_open);
        let shell = cx.weak_entity();
        let lang = self.lang;

        let all = NavNode { id: "all".into(), title: t(i18n::Key::NavAllNotes).into(), count: 0, depth: 0 };
        let mut all_count = 0i64;
        if let Some(store) = self.store() {
            all_count = store.list_notes_filtered(None, false).map(|n| n.len() as i64).unwrap_or(0);
        }
        let all = NavNode { count: all_count, ..all };

        let mut starred_count = 0i64;
        if let Some(store) = self.store() {
            starred_count = store.list_notes_filtered(None, true).map(|n| n.len() as i64).unwrap_or(0);
        }
        let mut trash_count = 0i64;
        if let Some(store) = self.store() {
            trash_count = store.list_trash().map(|n| n.len() as i64).unwrap_or(0);
        }
        let starred = NavNode { id: "starred".into(), title: t(i18n::Key::NavStarred).into(), count: starred_count, depth: 0 };
        let trash = NavNode { id: "trash".into(), title: t(i18n::Key::NavTrash).into(), count: trash_count, depth: 0 };

        let all_row = self.nav_row(dark, &all, "fixed", Some(IconName::BookOpen), self.nav == NavSelection::AllNotes, cx);
        let starred_row = self.nav_row(dark, &starred, "starred", Some(IconName::Star), self.nav == NavSelection::Starred, cx);
        let trash_row = self.nav_row(dark, &trash, "trash", Some(IconName::Delete), self.nav == NavSelection::Trash, cx);

        let mut nav = gpui_kit::base::v_flex()
            .w(px(LEFT_W))
            .h_full()
            .flex_none()
            .bg(theme::material(Material::Base, dark))
            .pt(px(TITLEBAR_H)) // below the transparent titlebar
            .px_3()
            .gap_1()
            .child(Label::new(t(i18n::Key::AppName)).text_size(px(16.)).text_color(fg))
            .child(all_row)
            .child(starred_row)
            .child(trash_row)
            .child(self.group_header(dark, t(i18n::Key::NavNotebooks), notebooks_open, Group::Notebooks, cx));

        // Ticket 11: right-click menu on the Notebooks group header —
        // "New Notebook" at root level.
        nav = nav.child(
            self.group_header(dark, t(i18n::Key::NavNotebooks), notebooks_open, Group::Notebooks, cx)
                .context_menu({
                    let shell = shell.clone();
                    move |menu, _window, _cx| {
                        let shell = shell.clone();
                        menu.item(
                            PopupMenuItem::new(i18n::t(lang, i18n::Key::NewNotebook)).on_click(
                                move |_ev, window, cx| {
                                    let _ = shell.update(cx, |this, cx| {
                                        this.begin_new_notebook(String::new(), window, cx)
                                    });
                                },
                            ),
                        )
                    }
                }),
        );

        if notebooks_open {
            // Ticket 11: inline editor row when creating/renaming.
            if let Some(NavEdit::NewNotebook { parent, .. }) = &self.nav_edit {
                // render at the parent's nesting depth; root edit on top
                let depth = if parent.is_empty() {
                    0
                } else {
                    self.notebook_nav_nodes()
                        .iter()
                        .find(|n| n.id == *parent)
                        .map(|n| n.depth + 1)
                        .unwrap_or(0)
                };
                let _ = depth;
                // row itself is drawn in the loop below via nav_edit_row
            }
            for node in self.notebook_nav_nodes() {
                let sel = NavSelection::Notebook(node.id.clone());
                let is_sel = self.nav == sel;
                let row = self.nav_row(
                    dark,
                    &node,
                    "notebook",
                    Some(IconName::FolderClosed),
                    is_sel,
                    cx,
                );
                // nested-create affordance + management menu per notebook row
                let id = leanote_core::Id(node.id.clone());
                let title = node.title.clone();
                nav = nav.child(
                    row.context_menu({
                        let shell = shell.clone();
                        let id2 = id.clone();
                        let title2 = title.clone();
                        move |menu, _window, _cx| {
                            let shell_new = shell.clone();
                            let shell_ren = shell.clone();
                            let shell_mv = shell.clone();
                            let shell_del = shell.clone();
                            let id_new = id2.to_string();
                            let id_ren = id2.clone();
                            let id_mv = id2.clone();
                            let id_del = id2.clone();
                            let _ = title2;
                            menu
                                .item(
                                    PopupMenuItem::new(i18n::t(lang, i18n::Key::NewNotebook))
                                        .on_click(move |_ev, window, cx| {
                                            let _ = shell_new.update(cx, |this, cx| {
                                                this.begin_new_notebook(id_new.clone(), window, cx)
                                            });
                                        }),
                                )
                                .item(
                                    PopupMenuItem::new(i18n::t(lang, i18n::Key::RenameNotebook))
                                        .on_click(move |_ev, window, cx| {
                                            let _ = shell_ren.update(cx, |this, cx| {
                                                this.begin_rename_notebook(id_ren.clone(), window, cx)
                                            });
                                        }),
                                )
                                .item(
                                    PopupMenuItem::new(i18n::t(lang, i18n::Key::MoveNotebookTo))
                                        .on_click(move |_ev, _window, cx| {
                                            let _ = shell_mv.update(cx, |this, cx| {
                                                this.begin_move_notebook(id_mv.clone(), cx)
                                            });
                                        }),
                                )
                                .item(
                                    PopupMenuItem::new(i18n::t(lang, i18n::Key::DeleteNotebook))
                                        .on_click(move |_ev, _window, cx| {
                                            let _ = shell_del.update(cx, |this, cx| {
                                                this.delete_notebook_nav(id_del.clone(), cx)
                                            });
                                        }),
                                )
                        }
                    })
                    .when(node.depth > 0, |el| el),
                );
            }
            // Ticket 11: inline create/rename editor row.
            if let Some(edit_row) = self.nav_edit_row(dark, t, "notebook", window, cx) {
                nav = nav.child(edit_row);
            }
        }

        nav = nav.child(self.group_header(dark, t(i18n::Key::NavTags), tags_open, Group::Tags, cx));

        if tags_open {
            for node in self.tag_nav_nodes() {
                let sel = NavSelection::Tag(node.id.clone());
                let is_sel = self.nav == sel;
                let row = self.nav_row(dark, &node, "tag", None, is_sel, cx);
                let tag = node.id.clone();
                nav = nav.child(row.context_menu({
                    let shell = shell.clone();
                    move |menu, _window, _cx| {
                        let shell_ren = shell.clone();
                        let shell_del = shell.clone();
                        let tag_ren = tag.clone();
                        let tag_del = tag.clone();
                        menu
                            .item(
                                PopupMenuItem::new(i18n::t(lang, i18n::Key::RenameTag)).on_click(
                                    move |_ev, window, cx| {
                                        let _ = shell_ren.update(cx, |this, cx| {
                                            this.begin_rename_tag(tag_ren.clone(), window, cx)
                                        });
                                    },
                                ),
                            )
                            .item(
                                PopupMenuItem::new(i18n::t(lang, i18n::Key::DeleteTag)).on_click(
                                    move |_ev, _window, cx| {
                                        let _ = shell_del.update(cx, |this, cx| {
                                            this.delete_tag_nav(tag_del.clone(), cx)
                                        });
                                    },
                                ),
                            )
                    }
                }));
            }
            // tag rename editor
            if let Some(edit_row) = self.nav_edit_row(dark, t, "tag", window, cx) {
                nav = nav.child(edit_row);
            }
        }

        // "Move to…" picker replaces the notebook list while choosing
        if let Some(NavEdit::MoveNotebook { id }) = &self.nav_edit {
            let moved = id.clone();
            let mut picker = gpui_kit::base::v_flex().gap_1();
            for (pid, ptitle, pdepth) in self.notebook_move_targets(&moved) {
                let shell2 = shell.clone();
                let pid2 = pid.clone();
                let moved2 = moved.clone();
                picker = picker.child(
                    gpui_kit::base::h_flex()
                        .id(format!("mv-{pid}"))
                        .h_8()
                        .px_3()
                        .ml(px(12. * pdepth as f32))
                        .rounded(px(theme::RADIUS_SELECTION))
                        .on_click(move |_, _, cx| {
                            let _ = shell2.update(cx, |this, cx| {
                                this.move_notebook_to(moved2.clone(), pid2.clone(), cx)
                            });
                        })
                        .child(Label::new(format!("{}{}", "　".repeat(pdepth), ptitle)).text_size(px(14.))),
                );
            }
            // cancel
            let shell3 = shell.clone();
            picker = picker.child(
                Button::new("mv-cancel")
                    .label(t(i18n::Key::Cancel).to_string())
                    .on_click(move |_, _, cx| {
                        let _ = shell3.update(cx, |this, cx| {
                            this.nav_edit = None;
                            cx.notify();
                        });
                    }),
            );
            nav = nav.child(picker);
        }

        // Ticket 15: whole-library .leanote backup, pinned to the nav bottom.
        nav = nav
            .child(gpui_kit::base::v_flex().flex_1())
            .child(
                Button::new("export-backup")
                    .label(t(i18n::Key::ExportBackup))
                    .ghost()
                    .on_click(cx.listener(|this, _ev, _window, cx| this.export_backup(cx))),
            );

        nav
    }

    /// Flat (id, title, depth) of every valid move target: root + all
    /// notebooks except the moved one and its descendants (no cycles).
    fn notebook_move_targets(&self, moved: &leanote_core::Id) -> Vec<(String, String, usize)> {
        let Some(store) = self.store() else { return Vec::new() };
        let Ok(nbs) = store.notebooks() else { return Vec::new() };
        let by_id: std::collections::HashMap<String, String> =
            nbs.iter().map(|nb| (nb.notebook_id.to_string(), nb.parent_notebook_id.clone())).collect();
        // descendants of `moved` are invalid targets
        let mut banned = std::collections::HashSet::new();
        banned.insert(moved.to_string());
        loop {
            let mut grew = false;
            for (id, parent) in &by_id {
                if banned.contains(parent) && !banned.contains(id) {
                    banned.insert(id.clone());
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        let mut out = vec![(String::new(), i18n::t(self.lang, i18n::Key::RootLevel).to_string(), 0usize)];
        for node in self.notebook_nav_nodes() {
            if !banned.contains(&node.id) {
                out.push((node.id.clone(), node.title.clone(), node.depth as usize + 1));
            }
        }
        out
    }

    /// Input + OK/Cancel row for the in-flight New/Rename edit.
    /// `group` filters: "notebook" renders NewNotebook/RenameNotebook,
    /// "tag" renders RenameTag.
    fn nav_edit_row(
        &mut self,
        _dark: bool,
        t: &dyn Fn(i18n::Key) -> &'static str,
        group: &str,
        _window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> Option<AnyElement> {
        // borrow dance: take the input entity handle out temporarily
        let (input, indent) = match self.nav_edit.as_ref()? {
            NavEdit::NewNotebook { parent, name } if group == "notebook" => {
                let depth = if parent.is_empty() {
                    0
                } else {
                    self.notebook_nav_nodes()
                        .iter()
                        .find(|n| n.id == *parent)
                        .map(|n| n.depth as usize + 1)
                        .unwrap_or(0)
                };
                (name.clone(), depth)
            }
            NavEdit::RenameNotebook { .. } if group == "notebook" => {
                if let Some(NavEdit::RenameNotebook { name, .. }) = self.nav_edit.as_ref() {
                    (name.clone(), 1)
                } else {
                    unreachable!()
                }
            }
            NavEdit::RenameTag { name, .. } if group == "tag" => (name.clone(), 1),
            _ => return None,
        };
        let shell = cx.weak_entity();
        let ok = Button::new(SharedString::from(format!("nav-edit-ok-{group}")))
            .label(t(i18n::Key::Confirm).to_string())
            .small()
            .primary()
            .on_click(move |_ev, _window, cx| {
                let _ = shell.update(cx, |this: &mut AppShell, cx| this.apply_nav_edit(cx));
            });
        let shell = cx.weak_entity();
        let cancel = Button::new(SharedString::from(format!("nav-edit-cancel-{group}")))
            .label(t(i18n::Key::Cancel).to_string())
            .small()
            .on_click(move |_ev, _window, cx| {
                let _ = shell.update(cx, |this: &mut AppShell, cx| {
                    this.nav_edit = None;
                    cx.notify();
                });
            });
        Some(
            gpui_kit::base::h_flex()
                .id(SharedString::from(format!("nav-edit-{group}")))
                .gap_1()
                .h_8()
                .px_3()
                .ml(px(12. * indent as f32))
                .rounded(px(theme::RADIUS_SELECTION))
                .child(Input::new(&input).appearance(true).bordered(true).h_8())
                .child(ok)
                .child(cancel)
                .when(false, |el| el)
                .into_any_element(),
        )
    }

    /// Middle list: two-row rows (title + meta), Wake 336px.
    fn render_list(
        &mut self,
        dark: bool,
        t: &dyn Fn(i18n::Key) -> &'static str,
        _window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> gpui_kit::Div {
        let fg = theme::foreground(dark);
        let muted = theme::muted_foreground(dark);
        let sel_fg = theme::accent_foreground();
        let notebook_names = self.notebook_names();
        let notes = self.visible_notes();
        let count = notes.len();
        let title = match &self.nav {
            NavSelection::AllNotes => t(i18n::Key::NavAllNotes),
            NavSelection::Starred => t(i18n::Key::NavStarred),
            NavSelection::Trash => t(i18n::Key::NavTrash),
            NavSelection::Notebook(id) => notebook_names
                .get(id)
                .map(|s| s.as_str())
                .unwrap_or(t(i18n::Key::NavNotebooks)),
            NavSelection::Tag(tag) => tag.as_str(),
        };

        // Trash view has no new-note; every other context does (ticket 10).
        let mut header = gpui_kit::base::h_flex()
            .gap_2()
            .child(
                Label::new(format!("{} · {}", title, count))
                    .text_size(px(16.))
                    .text_color(fg),
            )
            .child(gpui_kit::base::h_flex().flex_1());
        if self.nav != NavSelection::Trash {
            header = header.child(
                Button::new("new-note")
                    .icon(IconName::Plus)
                    .ghost()
                    .on_click(cx.listener(|this, _ev, window, cx| this.new_note(window, cx))),
            );
        }

        let mut list = gpui_kit::base::v_flex()
            .w(px(MID_W))
            .h_full()
            .flex_none()
            .bg(theme::material(Material::List, dark))
            .pt(px(TITLEBAR_H))
            .px_3()
            .gap_1()
            .child(header);

        if notes.is_empty() {
            list = list.child(small_label(t(i18n::Key::EmptyNotes), muted));
        } else {
            for n in &notes {
                let meta = browse::note_meta(
                    notebook_names.get(&n.notebook_id.to_string()).map(|s| s.as_str()).unwrap_or(""),
                    &n.tags,
                    n.updated_time,
                );
                let is_sel = self.selected.as_ref() == Some(&n.note_id);
                let star = n.star;
                let id = n.note_id.clone();
                let id2 = n.note_id.clone();
                list = list.child(
                    gpui_kit::base::v_flex()
                        .id(n.note_id.to_string())
                        .px_2()
                        .py_2()
                        .rounded(px(theme::RADIUS_SELECTION))
                        .when(is_sel, |el| el.bg(theme::accent()))
                        .on_click(cx.listener(move |this, _ev, _window, cx| this.select_note(id2.clone(), cx)))
                        .child(
                            gpui_kit::base::h_flex()
                                .child(
                                    Label::new(n.title.clone())
                                        .text_size(px(14.))
                                        .text_color(if is_sel { sel_fg } else { fg }),
                                )
                                .child(gpui_kit::base::h_flex().flex_1())
                                .when(star, |el| {
                                    el.child(
                                        Icon::new(IconName::StarFill)
                                            .xsmall()
                                            .text_color(if is_sel { sel_fg } else { theme::accent() }),
                                    )
                                }),
                        )
                        .child(small_label(meta, if is_sel { sel_fg } else { muted })),
                );
                let _ = (id, star);
            }
        }
        list
    }

    /// Right pane: remainder width, 720px centered reading column, read-only
    /// Markdown via TextView (CommonMark + GFM incl. tables/strikethrough/
    /// fenced code highlight/links/blockquotes; remote images over http).
    /// When `editing` is set for the selected note, renders the source
    /// editor + preview toggle instead (ticket 10).
    fn render_reader(
        &mut self,
        dark: bool,
        t: &dyn Fn(i18n::Key) -> &'static str,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> gpui_kit::Div {
        let fg = theme::foreground(dark);
        let muted = theme::muted_foreground(dark);
        let notebook_names = self.notebook_names();

        let pane = gpui_kit::base::v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .bg(theme::material(Material::Background, dark))
            .pt(px(TITLEBAR_H));

        // Ticket 14: adjudication bar when this note (or any note) has an
        // unresolved conflict. Built before the selection check so it also
        // shows with nothing selected.
        let conflicts = self
            .store()
            .and_then(|s| s.list_conflicts().ok())
            .unwrap_or_default();
        let bar = conflict_bar::bar_state(
            &conflicts,
            self.selected.as_ref().map(|id| id.to_string()).as_deref(),
        )
        .map(|c| {
            let (sid, sid2, sid3) = (
                c.server_note_id.clone(),
                c.server_note_id.clone(),
                c.server_note_id.clone(),
            );
            conflict_bar::render(
                dark,
                self.lang,
                c,
                cx.listener(move |this, _ev, _window, cx| {
                    this.resolve_conflict(ConflictChoice::UseLocal, sid.clone(), cx)
                }),
                cx.listener(move |this, _ev, _window, cx| {
                    this.resolve_conflict(ConflictChoice::UseServer, sid2.clone(), cx)
                }),
                cx.listener(move |this, _ev, _window, cx| {
                    this.resolve_conflict(ConflictChoice::KeepBoth, sid3.clone(), cx)
                }),
            )
        });

        let note = self
            .selected
            .as_ref()
            .and_then(|id| self.store().and_then(|s| s.get_note_any(id).ok().flatten()));
        // Editor mode takes over the pane for the selected note (ticket 10).
        if self
            .editing
            .as_ref()
            .is_some_and(|ed| Some(&ed.note_id) == self.selected.as_ref())
        {
            return self.render_editor(dark, t, window, cx);
        }

        let Some(n) = note else {
            let mut empty = gpui_kit::base::v_flex().w(px(READER_W)).mx_auto().gap_2();
            if let Some(bar) = bar {
                empty = empty.child(bar);
            }
            return pane.child(
                empty
                    .child(small_label(i18n::t(self.lang, i18n::Key::NoSelection), muted))
                    .child(Label::new(t(i18n::Key::NoSelection)).text_size(px(16.)).text_color(fg)),
            );
        };

        let is_trash_view = self.nav == NavSelection::Trash;
        let nb_name = notebook_names
            .get(&*n.notebook_id.to_string())
            .cloned()
            .unwrap_or_else(|| n.notebook_id.to_string());

        // Toolbar: star toggle (live notes) / trash actions (trash view).
        let id = n.note_id.clone();
        let star_btn = if n.star {
            Button::new("star-toggle")
                .icon(IconName::StarFill)
                .ghost()
                .on_click(cx.listener(move |this, _ev, _window, cx| this.toggle_star(id.clone(), cx)))
        } else {
            Button::new("star-toggle")
                .icon(IconName::Star)
                .ghost()
                .on_click(cx.listener(move |this, _ev, _window, cx| this.toggle_star(id.clone(), cx)))
        };

        let mut toolbar = gpui_kit::base::h_flex().gap_2().child(star_btn);
        // Ticket 12: rich-text notes are read-only Markdown renders; the
        // only write path is the explicit convert action. No edit entry
        // exists anywhere for them.
        if !n.is_markdown {
            let cid = n.note_id.clone();
            toolbar = toolbar
                .child(small_label(t(i18n::Key::RichTextReadonly), muted))
                .child(
                    Button::new("convert-md")
                        .label(t(i18n::Key::ConvertToMarkdown))
                        .outline()
                        .on_click(cx.listener(move |this, _ev, _window, cx| {
                            this.convert_selected_to_markdown(cid.clone(), cx)
                        })),
                );
        }
        if self.converted_note.as_ref() == Some(&n.note_id) {
            toolbar =
                toolbar.child(small_label(t(i18n::Key::ConvertedToMarkdown), muted));
        }
        if is_trash_view {
            let rid = n.note_id.clone();
            toolbar = toolbar.child(
                Button::new("restore")
                    .label(t(i18n::Key::Restore))
                    .outline()
                    .on_click(cx.listener(move |this, _ev, _window, cx| this.restore_selected(rid.clone(), cx))),
            );
            let did = n.note_id.clone();
            toolbar = toolbar.child(
                Button::new("hard-delete")
                    .label(t(i18n::Key::HardDelete))
                    .outline()
                    .on_click(cx.listener(move |this, _ev, _window, cx| this.hard_delete_selected(did.clone(), cx))),
            );
            toolbar = toolbar.child(
                Button::new("empty-trash")
                    .label(t(i18n::Key::EmptyTrash))
                    .outline()
                    .on_click(cx.listener(|this, _ev, _window, cx| this.empty_trash(cx))),
            );
        } else if n.is_markdown {
            toolbar = toolbar.child(
                Button::new("edit-note")
                    .label(t(i18n::Key::Edit))
                    .outline()
                    .on_click(cx.listener(|this, _ev, window, cx| this.start_editing(window, cx))),
            );
        }

        // Ticket 15: export actions (live notes only).
        if !is_trash_view {
            toolbar = toolbar
                .child(
                    Button::new("export-html")
                        .label(t(i18n::Key::ExportHtml))
                        .outline()
                        .on_click(cx.listener(|this, _ev, _window, cx| this.export_selected_file(false, cx))),
                )
                .child(
                    Button::new("export-markdown")
                        .label(t(i18n::Key::ExportMarkdown))
                        .outline()
                        .on_click(cx.listener(|this, _ev, _window, cx| this.export_selected_file(true, cx))),
                )
                .child(
                    Button::new("export-pdf")
                        .label(t(i18n::Key::ExportPdf))
                        .outline()
                        .on_click(cx.listener(|this, _ev, _window, cx| this.export_selected_pdf(cx))),
                );
        }
        if let Some(status) = self.export_status.clone() {
            toolbar = toolbar.child(
                Label::new(status)
                    .text_size(px(12.))
                    .text_color(theme::muted_foreground(dark)),
            );
        }

        let src = browse::reader_markdown_src(&n.title, &n.content, n.is_markdown);


        let mut col = gpui_kit::base::v_flex()
            .w(px(READER_W))
            .mx_auto()
            .gap_2()
            .min_h_0();
        if let Some(bar) = bar {
            col = col.child(bar);
        }
        col = col
            .child(
                gpui_kit::base::h_flex()
                    .gap_2()
                    .child(small_label(
                        browse::note_meta(&nb_name, &n.tags, n.updated_time),
                        muted,
                    ))
                    .child(gpui_kit::base::h_flex().flex_1()),
            )
            .child(toolbar)
            // GFM tables + strikethrough ship in TextView's default GFM
            // parse options; `.scrollable` keeps the whole pane from
            // scrolling away under the toolbar.
            .child(
                TextView::markdown("reader", src)
                    .scrollable(true)
                    .on_link_click(|url, _ev, _window, cx| cx.open_url(url)),
            );
        pane.child(col)
    }

    /// Right pane in edit mode (ticket 10): title input + source textarea
    /// with preview toggle, save, history. ⌘S/ctrl-S is handled by an
    /// on_key_down that only matches the exact modifier combo — every text
    /// keystroke (incl. IME composition) passes through untouched.
    fn render_editor(
        &mut self,
        dark: bool,
        t: &dyn Fn(i18n::Key) -> &'static str,
        _window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> gpui_kit::Div {
        let fg = theme::foreground(dark);
        let muted = theme::muted_foreground(dark);
        let note_id = self.editing.as_ref().unwrap().note_id.clone();
        let history = self
            .store()
            .and_then(|s| s.list_history(&note_id).ok())
            .unwrap_or_default();

        let pane = gpui_kit::base::v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .bg(theme::material(Material::Background, dark))
            .pt(px(TITLEBAR_H));

        let Some(ed) = &self.editing else { unreachable!("caller checked") };
        let title_entity = ed.title.clone();
        let content_entity = ed.content.clone();
        let preview = ed.preview;
        let saved_flash = ed.saved_flash;

        // Toolbar: Source/Preview toggle · Save · History · Done.
        let mut toolbar = gpui_kit::base::h_flex().gap_2();
        let (toggle_key, toggle_icon) = if preview {
            (i18n::Key::EditorSource, AssetIcon::Code)
        } else {
            (i18n::Key::EditorPreview, AssetIcon::Eye)
        };
        toolbar = toolbar.child(
            Button::new("toggle-preview")
                .label(t(toggle_key))
                .icon(toggle_icon)
                .outline()
                .on_click(cx.listener(|this, _ev, _window, cx| {
                    if let Some(ed) = &mut this.editing {
                        ed.preview = !ed.preview;
                    }
                    cx.notify();
                })),
        );
        toolbar = toolbar.child(
            Button::new("save-note")
                .label(t(if saved_flash { i18n::Key::Saved } else { i18n::Key::Save }))
                .icon(AssetIcon::Save)
                .primary()
                .on_click(cx.listener(|this, _ev, _window, cx| this.save_editor(cx))),
        );
        let hist_count = history.len();
        toolbar = toolbar.child(
            Button::new("history")
                .label(format!("{} · {}", t(i18n::Key::History), hist_count))
                .icon(AssetIcon::Clock)
                .outline()
                .on_click(cx.listener(|this, _ev, _window, cx| {
                    this.history_open = !this.history_open;
                    this.history_preview = None;
                    cx.notify();
                })),
        );
        toolbar = toolbar.child(gpui_kit::base::h_flex().flex_1());
        toolbar = toolbar.child(
            Button::new("done")
                .label(t(i18n::Key::Done))
                .ghost()
                .on_click(cx.listener(|this, _ev, _window, cx| this.stop_editing(cx))),
        );

        // Body: preview via TextView, source via kit Textarea. The whole
        // column gets the ⌘S key listener; it only acts on the exact combo
        // so IME composition keystrokes flow to the input untouched.
        let body = if preview {
            let src = content_entity.read(cx).value().to_string();
            let title_txt = title_entity.read(cx).value().to_string();
            gpui_kit::base::v_flex()
                .min_h_0()
                .flex_1()
                .child(
                    TextView::markdown("editor-preview", browse::reader_markdown_src(
                        &title_txt,
                        &src,
                        true,
                    ))
                    .scrollable(true)
                    .on_link_click(|url, _ev, _window, cx| cx.open_url(url)),
                )
                .into_any_element()
        } else {
            gpui_kit::base::v_flex()
                .min_h_0()
                .flex_1()
                .gap_2()
                .child(
                    Input::new(&title_entity)
                        .appearance(false)
                        .bordered(false)
                        .h(px(32.)),
                )
                .child(Textarea::new(&content_entity).flex_1().min_h_0().h_full())
                .into_any_element()
        };

        let mut col = gpui_kit::base::v_flex()
            .w(px(READER_W))
            .mx_auto()
            .gap_2()
            .min_h_0()
            .h_full()
            .child(toolbar)
            .child(body)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _window, cx| {
                // IME-safe: only exact cmd/ctrl+S (key_char is None for
                // cmd combos), never intercepts composition keystrokes.
                let m = &ev.keystroke.modifiers;
                let exact = (m.platform || m.control) && !m.alt && !m.shift
                    && ev.keystroke.key.eq_ignore_ascii_case("s");
                if exact {
                    this.save_editor(cx);
                }
            }));

        // History popover (list + preview + restore).
        if self.history_open {
            let mut rows = gpui_kit::base::v_flex().gap_1();
            if let Some(pid) = &self.history_preview {
                if let Some(h) = history.iter().find(|h| &h.history_id == pid) {
                    let src = browse::reader_markdown_src(&h.title, &h.content, true);
                    let back_id = pid.clone();
                    rows = rows
                        .child(small_label(format!("{} · {}", h.title, fmt_time(h.created_time)), muted))
                        .child(
                            TextView::markdown("history-preview", src)
                                .scrollable(true)
                                .on_link_click(|url, _ev, _window, cx| cx.open_url(url)),
                        )
                        .child(
                            Button::new("history-back")
                                .label(t(i18n::Key::History))
                                .outline()
                                .on_click(cx.listener(move |this, _ev, _window, _cx| {
                                    this.history_preview = None;
                                })),
                        );
                    let rid = pid.clone();
                    col = col.child(
                        Button::new("history-restore")
                            .label(t(i18n::Key::RestoreVersion))
                            .icon(AssetIcon::RotateCcw)
                            .primary()
                            .on_click(cx.listener(move |this, _ev, window, cx| {
                                this.restore_history_version(rid.clone(), window, cx)
                            })),
                    );
                }
            } else if history.is_empty() {
                rows = rows.child(small_label(t(i18n::Key::HistoryEmpty), muted));
            } else {
                for h in &history {
                    let hid = h.history_id.clone();
                    let label = format!("{} · {}", h.title, fmt_time(h.created_time));
                    rows = rows.child(
                        Button::new(format!("history-row-{}", h.history_id.to_string()))
                            .label(label)
                            .ghost()
                            .on_click(cx.listener(move |this, _ev, _window, _cx| {
                                this.history_preview = Some(hid.clone());
                            })),
                    );
                }
            }
            col = col.child(rows);
        }

        // Clear the one-frame saved flash after this render.
        if saved_flash {
            if let Some(ed) = &mut self.editing {
                ed.saved_flash = false;
            }
        }
        let _ = fg;
        pane.child(col)
    }
}

/// epoch ms → "YYYY-MM-DD" (display only; store keeps epoch ms).
pub(crate) fn fmt_time(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    // Howard Hinnant civil_from_days.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_time_known_dates() {
        assert_eq!(fmt_time(0), "1970-01-01");
        assert_eq!(fmt_time(1_765_235_200_000), "2025-12-08");
        assert_eq!(fmt_time(86_399_999), "1970-01-01");
        assert_eq!(fmt_time(86_400_000), "1970-01-02");
    }

    #[test]
    fn nav_selection_compares_by_identity() {
        assert_eq!(NavSelection::Notebook("nb1".into()), NavSelection::Notebook("nb1".into()));
        assert_ne!(NavSelection::Notebook("nb1".into()), NavSelection::Tag("nb1".into()));
        assert_eq!(NavSelection::Tag("rust".into()), NavSelection::Tag("rust".into()));
    }
}

// Window-state glue used by main.rs.

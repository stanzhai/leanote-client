//! The main window: login form (right pane) → full-sync progress → Wake
//! three-pane shell (ADR-0003). Ticket 09: selectable nav + filtered list +
//! read-only Markdown reader, star toggle, trash actions.

use crate::browse::{self, DropTarget, NavNode};
use crate::conflict_bar;
use crate::i18n::{self, Lang};
use crate::theme::{self, Material};
use gpui_kit::assets::IconName as AssetIcon;
use gpui_kit::base::Scrollbar;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::combobox::{Combobox, ComboboxEvent, ComboboxState};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::component::input::{
    Backspace, Delete, Editor, EditorState, Enter, Escape, IndentInline, Input, InputEvent,
    InputState, MoveDown, MoveLeft, MoveRight, MoveUp, OutdentInline,
};
use gpui_kit::component::label::Label;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::resizable_panel;
use gpui_kit::component::searchable_list::SearchableVec;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::text::TextView;
use gpui_kit::component::IndexPath;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, IconName, InteractiveElementExt, Sizable, WindowExt as _,
};
use gpui_kit::div;
use gpui_kit::prelude::*;
use gpui_kit::{
    px, rems, AnyElement, KeyDownEvent, MouseButton, SharedString, Task, UniformListScrollHandle,
    Window, WindowControlArea,
};
use leanote_core::sync::{Api, NoProgress, SyncProgress, SyncStats};
use std::sync::{Arc, Mutex};

/// Default pane widths (ADR-0003). Issue 18: user-resizable, persisted;
/// these are the first-run and reset values. Left starts at its drag
/// minimum — the sidebar only needs icon + title + count.
const LEFT_W: f32 = 150.;
const MID_W: f32 = 240.;
/// Issue 18: drag ranges for the left/middle panes.
const LEFT_RANGE: (f32, f32) = (150., 400.);
const MID_RANGE: (f32, f32) = (240., 600.);
/// Issue 18: uniform note-row height — py 4 + title 19 + meta 20 + excerpt
/// 20 + 2× 1px gaps = 65px.
const NOTE_ROW_H: f32 = 65.;
/// Issue 23 #11: 1px divider under each note row (except the last) —
/// `theme::border(dark)`, auto-adapting to light/dark.
const NOTE_ROW_DIVIDER_H: f32 = 1.;
/// Recent list cap — one source of truth for the list query AND the nav
/// badge count (issue: badge must equal what Recent actually shows).
const RECENT_LIMIT: usize = 20;
/// macOS transparent titlebar height, same material as the sidebar.
const TITLEBAR_H: f32 = 44.;

pub enum Phase {
    Login,
    SigningIn,
    /// Progress lines from full_sync, newest last. `done/total` carry the
    /// deterministic content stage (None until it starts or when total=0).
    Syncing {
        lines: Vec<SharedString>,
        done: usize,
        total: usize,
    },
    /// Full sync finished → summary card + "start" button → Shell.
    SyncDone(Box<SyncSummary>),
    /// Sync done → three-pane shell.
    Shell,
    /// Login or sync failed → back to login with an error line.
    Failed {
        error: SharedString,
    },
}

pub(crate) struct SyncSummary {
    notebooks: usize,
    notes: usize,
    contents: usize,
    tags: usize,
    content_failures: usize,
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

#[derive(Clone)]
enum NavEdit {
    /// Move this notebook: picking the target parent (New/Rename moved to
    /// a modal dialog, see `notebook_name_dialog`).
    MoveNotebook { id: leanote_core::Id },
    /// Issue 18: move this note to another notebook — the middle list
    /// swaps to a target picker.
    MoveNote { id: leanote_core::Id },
    /// Feature 3: batch move — same picker, commits to every
    /// multi-selected note at once.
    MoveNoteBatch,
    /// Rename this tag.
    RenameTag {
        old: String,
        name: gpui_kit::Entity<InputState>,
    },
}

/// What the middle list is filtered to (left-nav selection).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavSelection {
    Recent,
    Starred,
    Trash,
    Notebook(String),
    Tag(String),
}

/// Issue 19: nav selection ↔ prefs tag ("recent"/"starred"/"trash"/
/// "notebook"/"tag", id "" for fixed ones).
fn nav_selection_key(nav: &NavSelection) -> (String, String) {
    match nav {
        NavSelection::Recent => ("recent".into(), String::new()),
        NavSelection::Starred => ("starred".into(), String::new()),
        NavSelection::Trash => ("trash".into(), String::new()),
        NavSelection::Notebook(id) => ("notebook".into(), id.clone()),
        NavSelection::Tag(tag) => ("tag".into(), tag.clone()),
    }
}

fn nav_from_key(kind: &str, id: &str) -> Option<NavSelection> {
    match kind {
        "recent" => Some(NavSelection::Recent),
        "starred" => Some(NavSelection::Starred),
        "trash" => Some(NavSelection::Trash),
        "notebook" => Some(NavSelection::Notebook(id.into())),
        "tag" => Some(NavSelection::Tag(id.into())),
        _ => None,
    }
}

pub struct AppShell {
    phase: Phase,
    lang: Lang,
    /// Active account's UserId (rows are scoped per user db); "user1" until
    /// login re-keys it.
    user_id: String,
    /// Review item 2/3: outcome of the last incr sync (startup + SyncNow).
    sync_status: Option<Result<SharedString, SharedString>>,
    /// Local edits not yet pushed (notes/notebooks/tags IsDirty=1) —
    /// recomputed in invalidate_lists(); drives the pending-sync icon.
    pending_sync: bool,
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
    /// Feature 3 multi-select: ids beyond `selected` picked with
    /// ⇧-range/⌘-toggle. Empty = plain single-select. `selected` stays
    /// the anchor-carrying "last clicked" note for the reader pane.
    selected_ids: Vec<leanote_core::Id>,
    /// Where a ⇧ click extends from (last clicked row).
    sel_anchor: Option<leanote_core::Id>,
    /// Issue 19: focus mode — hide left + middle panes (⇧⌘F toggles).
    focus_mode: bool,
    /// Issue 21 需求10: ⌘T full-window markdown preview (reader or
    /// editor source rendered edge-to-edge; ESC/⌘T leaves).
    full_preview: bool,
    /// Focus target while the full-window preview is open: without a
    /// focused element the key dispatch path skips this shell's nodes
    /// entirely, and ESC would stop working.
    preview_focus: gpui_kit::FocusHandle,
    /// UI round 10 #7: permanent focus target on the root so ⌘K/⌘, dispatch
    /// even when no input has focus (gpui routes actions down the focus
    /// chain; an unfocused window falls off it).
    root_focus: gpui_kit::FocusHandle,
    /// Ticket 15: export outcome shown inline in the reader toolbar
    /// (e.g. PDF-fell-back-to-HTML reason). Cleared on selection change.
    export_status: Option<SharedString>,
    /// Ticket 13: ⌘K search overlay (dialog-hosted palette + last rows).
    search: crate::search_overlay::SearchOverlay,
    search_rows: Vec<crate::search_overlay::ResultRow>,
    /// Latest committed query (debounce re-entry takes the newest).
    search_query: String,
    /// Debounced ⌘K search task (issue 17 Q6).
    search_task: Option<Task<()>>,
    // --- Ticket 12 rich-text state ---
    /// Set after a successful Convert to Markdown; cleared when the
    /// selection changes. Rendered as a one-line feedback next to the badge.
    converted_note: Option<leanote_core::Id>,
    // --- Ticket 11 nav management ---
    nav_edit: Option<NavEdit>,
    // --- Ticket 10 editor state ---
    /// Edit mode for `selected` (only entered for live IsMarkdown notes).
    pub(crate) editing: Option<EditState>,
    /// History popover open (only meaningful while editing).
    history_open: bool,
    /// 斜杠面板（ADR-0011）：编辑器内 `/` 触发的 4×4 命令格；None = 关。
    pub(crate) slash_panel: Option<crate::slash_panel::SlashPanel>,
    /// Which history row is previewed (None = list view).
    history_preview: Option<leanote_core::Id>,
    /// List caches — invalidated by invalidate_lists() on any mutation
    /// (select/sync/save/nav edit/...). Avoids re-querying the DB every frame.
    cached_notebook_nodes: Option<Vec<NavNode>>,
    cached_tag_nodes: Option<Vec<NavNode>>,
    cached_notebook_names: Option<std::collections::HashMap<String, String>>,
    cached_visible_notes: Option<Vec<leanote_core::NoteMeta>>,
    /// Scroll handles for the virtualized lists.
    notes_scroll: UniformListScrollHandle,
    notebooks_scroll: UniformListScrollHandle,
    tags_scroll: UniformListScrollHandle,
    // --- Issue 17 vim mode (ADR-0004) ---
    /// Pure key-grammar state machine; side effects execute in the shell.
    vim: crate::vim::VimState,
    /// Vim enabled (toolbar toggle; persisted as prefs.vim_default).
    vim_enabled: bool,
    // --- Notebook sidebar drag-reorder (ticket 11 follow-up) ---
    /// Drop target under the pointer while a notebook row is dragged.
    /// None = no valid target. Lives on the shell: rows are rebuilt every
    /// frame, so nothing can live in the row closures.
    notebook_drag_target: Option<crate::browse::DropTarget>,
}

/// Right-pane edit mode (issues 10+17): title + source editor with syntax
/// highlighting. Buffer lives in the kit's EditorState so IME composition is
/// handled entirely by the kit; `saved_content` is the last persisted text so
/// an unchanged editor never writes to the store (issue 17 Q2/Q8).
pub(crate) struct EditState {
    note_id: leanote_core::Id,
    title: gpui_kit::Entity<InputState>,
    pub(crate) content: gpui_kit::Entity<EditorState>,
    /// Last persisted content — auto-save compares against it.
    saved_content: String,
    /// True when the buffer has changed since last persist (drives autosave).
    dirty: bool,
    /// Show rendered preview instead of source.
    pub(crate) preview: bool,
    /// Set for one frame after a save, to flash "Saved".
    saved_flash: bool,
    /// Issue 23 #6: working copy of tags (status bar chips); Some(≠ store
    /// value) marks dirty so the next autosave flushes them too.
    tags: Vec<String>,
    /// Issue 23 #6: last persisted UpdatedTime (ms epoch), status bar right.
    updated_time: i64,
    /// Issue 23 #6: tag-add input in the status bar (empty text).
    tag_input: gpui_kit::Entity<InputState>,
}

/// Progress sink that forwards full_sync lines into a channel the UI drains.
/// Ticks travel as `@done/total` — note() and tick() share one channel so
/// ordering with the text lines is preserved.
struct ChannelProgress(Arc<Mutex<smol::channel::Sender<String>>>);

impl SyncProgress for ChannelProgress {
    fn note(&self, msg: String) {
        // ponytail: try_send, dropped lines only cost progress granularity
        let _ = self.0.lock().unwrap().try_send(msg);
    }
    fn tick(&self, done: usize, total: usize) {
        let _ = self.0.lock().unwrap().try_send(format!("@{done}/{total}"));
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
        // Signed-in account's user id (None → still "user1" placeholder).
        user_id: Option<String>,
        fake_prefill: Option<(&str, &str, &str)>,
        // Last account (host, email) for login prefill after logout.
        remembered: Option<(String, String)>,
    ) -> Self {
        // Fake prefill wins (dev override); else remembered account.
        let prefill = fake_prefill
            .map(|(h, e, _)| (h.to_string(), e.to_string()))
            .or(remembered);
        let host = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("https://leanote.com");
            if let Some((h, _)) = &prefill {
                s = s.default_value(h);
            }
            s
        });
        let email = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("user@example.com");
            if let Some((_, e)) = &prefill {
                s = s.default_value(e);
            }
            s
        });
        let password = cx.new(|cx| {
            // No placeholder: host+email are prefilled, password is the
            // only thing left to type — a masked-bullets hint adds nothing.
            let mut s = InputState::new(window, cx).masked(true);
            if let Some((_, _, p)) = fake_prefill {
                s = s.default_value(p);
            }
            s
        });
        let prefs = crate::prefs::Prefs::load(&data_dir);
        // Enter in the password field submits.
        let pw = password.clone();
        cx.subscribe(&pw, |this, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.start_login(cx);
            }
        })
        .detach();

        let mut shell = Self {
            // Returning user: straight to the shell (startup incr-sync is
            // ticket 04 territory, not this one).
            phase: if signed_in {
                Phase::Shell
            } else {
                Phase::Login
            },
            lang,
            user_id: user_id.unwrap_or_else(|| "user1".to_string()),
            sync_status: None,
            pending_sync: false,
            vim_enabled: prefs.vim_default,
            notebook_drag_target: None,
            data_dir,
            host,
            email,
            password,
            _task: None,
            nav: prefs
                .nav
                .selection
                .as_ref()
                .and_then(|(k, id)| nav_from_key(k, id))
                .unwrap_or(NavSelection::Recent),
            notebooks_open: prefs.nav.notebooks_open,
            tags_open: prefs.nav.tags_open,
            selected: None,
            selected_ids: Vec::new(),
            sel_anchor: None,
            focus_mode: false,
            full_preview: false,
            preview_focus: cx.focus_handle(),
            root_focus: cx.focus_handle(),
            export_status: None,
            search: crate::search_overlay::SearchOverlay::new(window, cx),
            search_rows: Vec::new(),
            search_query: String::new(),
            search_task: None,
            converted_note: None,
            nav_edit: None,
            editing: None,
            history_open: false,
            slash_panel: None,
            history_preview: None,
            cached_notebook_nodes: None,
            cached_tag_nodes: None,
            cached_notebook_names: None,
            cached_visible_notes: None,
            notes_scroll: UniformListScrollHandle::new(),
            notebooks_scroll: UniformListScrollHandle::new(),
            tags_scroll: UniformListScrollHandle::new(),
            vim: crate::vim::VimState::new(),
        };
        // Issue 20 made the accordions exclusive, which let one session's
        // "I looked at Tags" persist as "Notebooks collapsed" forever.
        // Notebooks are the primary tree: open them every launch; Tags
        // keep whatever the user left (both open is fine — the header
        // scroll area fits them).
        if !shell.notebooks_open {
            shell.notebooks_open = true;
            let mut p = crate::prefs::Prefs::load(&shell.data_dir);
            p.nav.notebooks_open = true;
            p.save(&shell.data_dir);
        }
        // Issue 19 #10: restore the last selected note when it still exists.
        if let Some(id) = prefs.nav.last_note.clone().map(leanote_core::Id) {
            let exists = shell
                .store()
                .and_then(|s| s.get_note_any(&id).ok())
                .flatten()
                .is_some();
            if exists {
                shell.selected = Some(id);
                // Open the editor right away (issue 17): without this the
                // restored note showed no editor column until the user
                // re-clicked the note, so vim keys (and the status line)
                // never appeared after launch.
                shell.start_editing(window, cx);
            }
        }
        // Park focus on the root so app-scope actions (⌘K/⌘S/⌘N/⌘,)
        // dispatch before the user clicks anything: gpui routes actions
        // down the focus chain, and an unfocused window falls off it.
        // A later focus (login input, note click) replaces this cleanly.
        window.focus(&shell.root_focus, cx);
        // Prefilled login (logout → relaunch): host+email filled, jump
        // straight to the password field.
        if matches!(shell.phase, Phase::Login)
            && prefill
                .as_ref()
                .is_some_and(|(h, e)| !h.is_empty() && !e.is_empty())
        {
            shell.password.update(cx, |s, cx| s.focus(window, cx));
        }
        shell
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
            let result: Result<(String, SyncStats), String> = (|| {
                let Ok(store) = leanote_core::Store::open(dir.clone(), "user1".into()) else {
                    return Err("cannot open local store".into());
                };
                let account =
                    Api::login(&host, &email, &password, &store).map_err(|e| e.to_string())?;
                // Re-open the store keyed to the real user so synced rows land
                // under the account that owns them.
                let Ok(store) = leanote_core::Store::open(dir, account.user_id.clone()) else {
                    return Err("cannot open local store".into());
                };
                let api = Api::for_account(&account);
                let uid = account.user_id.clone();
                leanote_core::sync::full_sync(&api, &store, &progress)
                    .map(|s| (uid, s))
                    .map_err(|e| e.to_string())
            })();
            result
        });

        // Await the work on the app thread; stream progress meanwhile.
        // The channel closes when the background task drops its sender, so
        // recv() ends exactly when the work does — one drain, no extra spawn.
        self._task = Some(cx.spawn(async move |this, cx| {
            let mut lines: Vec<SharedString> = Vec::new();
            let mut done_total: Option<(usize, usize)> = None;
            let result = loop {
                // Poll the channel to completion; the sender dies with `work`.
                match rx.recv().await {
                    Ok(msg) => {
                        // `@d/t` = deterministic tick; anything else is a
                        // text line for the log view.
                        let tick = msg.strip_prefix('@').and_then(|s| {
                            let (d, t) = s.split_once('/')?;
                            Some((d.parse::<usize>().ok()?, t.parse::<usize>().ok()?))
                        });
                        match tick {
                            Some((done, total)) => done_total = Some((done, total)),
                            None => {
                                lines.push(msg.into());
                                if lines.len() > 200 {
                                    lines.remove(0); // keep the tail
                                }
                            }
                        }
                        let snapshot = lines.clone();
                        let progress = done_total;
                        let _ = this.update(cx, |shell, cx| {
                            if matches!(shell.phase, Phase::SigningIn) {
                                shell.phase = Phase::Syncing {
                                    lines: snapshot,
                                    done: progress.map_or(0, |p| p.0),
                                    total: progress.map_or(0, |p| p.1),
                                };
                                cx.notify();
                            } else if let Phase::Syncing { lines, done, total } = &mut shell.phase {
                                *lines = snapshot;
                                if let Some((d, t)) = progress {
                                    *done = d;
                                    *total = t;
                                }
                                cx.notify();
                            }
                        });
                    }
                    Err(_) => break work.await,
                }
            };
            let _ = this.update(cx, |shell, cx| {
                match result {
                    Ok((user_id, stats)) => {
                        shell.user_id = user_id;
                        shell.phase = Phase::SyncDone(Box::new(SyncSummary {
                            notebooks: stats.notebooks,
                            notes: stats.notes,
                            contents: stats.contents,
                            tags: stats.tags,
                            content_failures: stats.content_failures.len(),
                        }));
                        shell.invalidate_lists();
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

    /// Review item 2: background incr sync for the signed-in account
    /// (startup + SyncNow). Failure is non-blocking — logged + shown in the
    /// identity area.
    pub(crate) fn sync_now(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let Some(acct) = self.store().and_then(|s| s.active_account().ok().flatten()) else {
            return;
        };
        let dir = self.data_dir.clone();
        let uid = acct.user_id.clone();
        let work = cx.background_spawn(async move {
            (|| {
                let store = leanote_core::Store::open(dir, uid).map_err(|e| e.to_string())?;
                let api = Api::for_account(&acct);
                let stats = leanote_core::sync::incr_sync(&api, &store, &NoProgress)
                    .map_err(|e| e.to_string())?;
                Ok::<_, String>(stats)
            })()
        });
        self._task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |shell, cx| {
                match &result {
                    Ok(_) => {
                        shell.sync_status = Some(Ok("sync ok".into()));
                    }
                    Err(e) => {
                        leanote_core::logging::log("sync", e);
                        shell.sync_status = Some(Err(e.clone().into()));
                    }
                }
                shell.invalidate_lists();
                cx.notify();
            });
        }));
    }

    /// Review item 3: one muted line for the identity area.
    fn sync_status_label(&self) -> SharedString {
        let t = |k: i18n::Key| i18n::t(self.lang, k);
        // Err > Pending > Ok > None: a failure must stay visible even with
        // un-pushed edits queued behind it.
        if let Some(Err(e)) = &self.sync_status {
            return format!("{}: {}", t(i18n::Key::SyncFailed), e).into();
        }
        if self.pending_sync {
            return t(i18n::Key::PendingSync).into();
        }
        match &self.sync_status {
            Some(Ok(_)) => t(i18n::Key::SyncDone).into(),
            _ => t(i18n::Key::NotSynced).into(),
        }
    }

    /// Review item 2: logout — server-side signout (best effort), local
    /// account deactivated, back to the login screen. Bug fix: the settings
    /// dialog (and any other open dialog) must close with the account.
    fn logout(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        if let Some(acct) = self.store().and_then(|s| s.active_account().ok().flatten()) {
            Api::for_account(&acct).logout();
            if let Ok(store) =
                leanote_core::Store::open(self.data_dir.clone(), acct.user_id.clone())
            {
                let _ = store.deactivate_account();
            }
        }
        self.user_id = "user1".to_string();
        self.sync_status = None;
        self.pending_sync = false;
        self.phase = Phase::Login;
        self._task = None;
        self.invalidate_lists();
        window.close_all_dialogs(cx);
        cx.notify();
    }

    /// Issue 24: read the kit Theme mode, not `window.appearance()`. The
    /// window field only updates on AppKit's *deferred* effective-appearance
    /// callback, so pane backgrounds lagged a tick behind kit controls on
    /// every Light/Dark switch (the reported "theme switch is slow").
    /// Theme.mode flips inside `Theme::update`, same frame.
    pub(crate) fn dark(&self, cx: &gpui_kit::App) -> bool {
        cx.theme().mode.is_dark()
    }

    pub(crate) fn lang(&self) -> i18n::Lang {
        self.lang
    }

    fn store(&self) -> Option<leanote_core::Store> {
        // Re-open per read: cheap (WAL) and always fresh after star/trash ops.
        leanote_core::Store::open(self.data_dir.clone(), self.user_id.clone()).ok()
    }

    /// Issue 17: invalidate list caches — call after ANY mutation that can
    /// change notes/notebooks/tags (select, sync, save, nav edit, ...).
    fn invalidate_lists(&mut self) {
        self.cached_tag_nodes = None;
        self.cached_notebook_names = None;
        self.cached_notebook_nodes = None;
        self.cached_visible_notes = None;
        // Any local mutation may have dirtied rows — refresh the
        // pending-sync icon state (3 COUNTs, no row materialization).
        self.pending_sync = self
            .store()
            .and_then(|s| s.has_dirty().ok())
            .unwrap_or(false);
    }

    /// Notes for the current nav selection, in list order (cached).
    /// Issue 19: meta projection — no content blob in the list path.
    fn visible_notes(&mut self) -> &Vec<leanote_core::NoteMeta> {
        if self.cached_visible_notes.is_none() {
            let notes = self
                .store()
                .map(|store| {
                    let res = match &self.nav {
                        NavSelection::Recent => store.list_notes_meta_recent(RECENT_LIMIT),
                        NavSelection::Starred => store.list_notes_filtered_meta(None, true),
                        NavSelection::Trash => store.list_trash_meta(),
                        NavSelection::Notebook(id) => store
                            .list_notes_filtered_meta(Some(&leanote_core::Id(id.clone())), false),
                        NavSelection::Tag(tag) => store.list_notes_by_tag_meta(tag),
                    };
                    res.unwrap_or_default()
                })
                .unwrap_or_default();
            self.cached_visible_notes = Some(notes);
        }
        self.cached_visible_notes.as_ref().unwrap()
    }

    fn notebook_names(&mut self) -> &std::collections::HashMap<String, String> {
        if self.cached_notebook_names.is_none() {
            let names = self
                .store()
                .and_then(|s| s.notebooks().ok())
                .map(|nbs| {
                    nbs.into_iter()
                        .map(|nb| (nb.notebook_id.to_string(), nb.title))
                        .collect::<std::collections::HashMap<_, _>>()
                })
                .unwrap_or_default();
            self.cached_notebook_names = Some(names);
        }
        self.cached_notebook_names.as_ref().unwrap()
    }

    /// Nav-tree rows for the Notebooks group (nesting + Seq + counts, cached).
    fn notebook_nav_nodes(&mut self) -> &Vec<NavNode> {
        if self.cached_notebook_nodes.is_none() {
            let nodes = self
                .store()
                .map(|store| {
                    let (Ok(nbs), Ok(count_rows)) =
                        (store.notebooks(), store.notebook_note_counts())
                    else {
                        return Vec::new();
                    };
                    let counts: std::collections::HashMap<String, i64> = count_rows
                        .into_iter()
                        .map(|(id, c)| (id.to_string(), c))
                        .collect();
                    browse::notebook_nodes(
                        &nbs.iter()
                            .map(|nb| {
                                (
                                    nb.notebook_id.to_string(),
                                    nb.title.clone(),
                                    nb.parent_notebook_id.clone(),
                                    nb.seq,
                                )
                            })
                            .collect::<Vec<_>>(),
                        &counts,
                    )
                })
                .unwrap_or_default();
            self.cached_notebook_nodes = Some(nodes);
        }
        self.cached_notebook_nodes.as_ref().unwrap()
    }

    /// Nav rows for the Tags group (cached): aggregated purely from
    /// notes.Tags — the tags table can lag notes entirely after sync
    /// (issue 20: list was empty while notes carried tags).
    fn tag_nav_nodes(&mut self) -> &Vec<NavNode> {
        if self.cached_tag_nodes.is_none() {
            let nodes = self
                .store()
                .map(|store| {
                    store
                        .tag_note_counts()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(tag, count)| NavNode {
                            count,
                            id: tag.clone(),
                            title: format!("# {tag}"),
                            depth: 0,
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.cached_tag_nodes = Some(nodes);
        }
        self.cached_tag_nodes.as_ref().unwrap()
    }

    fn select_nav(&mut self, nav: NavSelection, cx: &mut gpui_kit::Context<Self>) {
        self.nav = nav.clone();
        self.selected = None; // new filter → no stale reader content
        self.selected_ids.clear(); // feature 3: multi-select is per-view
        self.sel_anchor = None;
        self.close_editor();
        // Issue 19: persist nav selection.
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        p.nav.selection = Some(nav_selection_key(&self.nav));
        p.save(&dir);
        self.invalidate_lists();
        cx.notify();
    }

    fn select_note(
        &mut self,
        id: leanote_core::Id,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        self.apply_selection(id, gpui_kit::Modifiers::default(), window, cx);
    }

    /// Feature 3: plain click = single select; ⇧ = range from anchor
    /// (visible-list order); ⌘/⌃ = toggle. Multi-select keeps the
    /// reader on `selected` and swaps it for the batch overlay.
    fn apply_selection(
        &mut self,
        id: leanote_core::Id,
        mods: gpui_kit::Modifiers,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if mods.shift {
            // Range anchor → clicked row, inclusive, in list order.
            let notes: Vec<leanote_core::Id> = self
                .visible_notes()
                .iter()
                .map(|n| n.note_id.clone())
                .collect();
            let anchor_ix = self
                .sel_anchor
                .as_ref()
                .and_then(|a| notes.iter().position(|n| n == a))
                .unwrap_or(0);
            let hit_ix = notes.iter().position(|n| n == &id).unwrap_or(anchor_ix);
            let (lo, hi) = if anchor_ix <= hit_ix {
                (anchor_ix, hit_ix)
            } else {
                (hit_ix, anchor_ix)
            };
            self.selected_ids = notes[lo..=hi].to_vec();
            self.selected = Some(id);
            self.close_editor();
            cx.notify();
            return;
        }
        if mods.platform || mods.control {
            // Seed with the plain-click selection on the first ⌘-click:
            // the note selected before multi-select started stays in.
            if self.selected_ids.is_empty() {
                if let Some(cur) = &self.selected {
                    self.selected_ids.push(cur.clone());
                }
                if self.sel_anchor.is_none() {
                    self.sel_anchor = self.selected.clone();
                }
            }
            let had = self.selected_ids.iter().any(|n| n == &id);
            if had {
                self.selected_ids.retain(|n| n != &id);
                // `selected` drives the row highlight; keep it pointing
                // at a row that is actually in the selection.
                if self.selected.as_deref() == Some(&id) {
                    self.selected = self.selected_ids.first().cloned();
                }
            } else {
                self.selected_ids.push(id.clone());
                self.sel_anchor = Some(id.clone());
                self.selected = Some(id);
            }
            self.close_editor();
            cx.notify();
            return;
        }
        // Plain click: single select. Recents + last-note only apply here
        // (multi-select is transient, not a reading position).
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        p.push_recent(&id.to_string());
        p.nav.last_note = Some(id.to_string());
        p.save(&dir);
        self.selected_ids.clear();
        self.sel_anchor = Some(id.clone());
        self.selected = Some(id);
        self.converted_note = None;
        self.close_editor();
        self.invalidate_lists();
        // Issue 17 Q2/Q8: a live markdown note opens straight in the editor.
        self.start_editing(window, cx);
        cx.notify();
    }

    fn close_editor(&mut self) {
        self.editing = None;
        self.history_open = false;
        self.history_preview = None;
    }

    /// Enter edit mode for the selected live markdown note. Buffer is seeded
    /// from the store; all editing (incl. IME composition) stays inside the
    /// kit's editor state — this code never mutates the buffer on key events.
    /// InputEvent::Change marks the buffer dirty and schedules the debounced
    /// auto-save (issue 17 Q8).
    fn start_editing(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let Some(note) = self
            .store()
            .and_then(|s| s.get_note_any(&id).ok().flatten())
        else {
            return;
        };
        if !note.is_markdown || note.is_trash || note.is_deleted {
            return;
        }
        let title_ph: SharedString = i18n::t(self.lang, i18n::Key::TitlePlaceholder).into();
        let title = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(note.title.clone())
                .placeholder(title_ph)
        });
        let content = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("markdown")
                .line_number(false)
                .folding(false)
                .soft_wrap(true)
                .default_value(note.content.clone())
        });
        // Buffer changes → dirty + 800ms debounced write-through.
        cx.subscribe(&content, |this, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                if let Some(ed) = &mut this.editing {
                    ed.dirty = true;
                }
                this.schedule_autosave(cx);
            }
        })
        .detach();
        // Empty title = freshly created note: focus lands on the title so
        // the first keystroke names it; existing notes keep focus in the
        // body (vim grammar + typing land in the content).
        let focus_title = note.title.is_empty();
        if focus_title {
            title.update(cx, |state, cx| state.focus(window, cx));
        } else {
            content.update(cx, |state, cx| state.focus(window, cx));
        }
        let tag_input = cx.new(|cx| {
            let ph: SharedString = i18n::t(self.lang, i18n::Key::AddTagPlaceholder).into();
            InputState::new(window, cx).placeholder(ph)
        });
        // Issue 23 #6: Enter in the tag field commits the chip.
        cx.subscribe(&tag_input, |this, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.add_tag(cx);
            }
        })
        .detach();
        self.editing = Some(EditState {
            note_id: id,
            title,
            content,
            saved_content: note.content,
            dirty: false,
            preview: false,
            saved_flash: false,
            tags: note.tags,
            updated_time: note.updated_time,
            tag_input,
        });
        // Issue 17 Q12: every session starts in normal mode.
        self.vim = crate::vim::VimState::new();
        // ADR-0005: normal mode rests on a block caret, so the very
        // first frame already shows one. Native block (vendored kit):
        // collapsed selection + blinking block, nothing selected.
        if self.vim_enabled {
            let editor = self.editing.as_ref().unwrap().content.clone();
            editor.update(cx, |state, cx| {
                state.set_block_cursor(true, cx);
                state.set_selected_range(0..0, cx);
            });
        }
        self.history_open = false;
        self.history_preview = None;
        cx.notify();
    }

    /// Issue 17 Q8: 800ms after the last keystroke, flush to the store (if
    /// the text actually changed). Restarted on every buffer change.
    fn schedule_autosave(&mut self, cx: &mut gpui_kit::Context<Self>) {
        self._task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(800))
                .await;
            let _ = this.update(cx, |shell, cx| shell.autosave_flush(cx));
        }));
    }

    fn autosave_flush(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let Some(ed) = &self.editing else { return };
        if !ed.dirty {
            return;
        }
        let content = ed.content.read(cx).value().to_string();
        if content == ed.saved_content {
            // Q8: no real change → never write, never dirty the sync row.
            if let Some(ed) = &mut self.editing {
                ed.dirty = false;
            }
            return;
        }
        self.save_editor(cx);
    }

    /// ⌘S / autosave: write through save_note_with_history (snapshot of the
    /// pre-change content, IsDirty=1 so sync pushes it). Unchanged text is a
    /// no-op (issue 17 Q8).
    fn save_editor(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let Some(ed) = &self.editing else { return };
        let title = ed.title.read(cx).value().to_string();
        let content = ed.content.read(cx).value().to_string();
        let id = ed.note_id.clone();
        let changed = content != ed.saved_content;
        // Issue 23 #6: tags edited from the status bar dirty the note too.
        if let Some(store) = self.store() {
            let saved_tags = store
                .get_note_any(&id)
                .ok()
                .flatten()
                .map(|n| n.tags)
                .unwrap_or_default();
            let tags_changed = saved_tags != ed.tags;
            if changed || tags_changed {
                let saved = store.save_note_with_history(
                    &id,
                    leanote_core::NoteUpdate {
                        title: Some(title),
                        content: Some(content.clone()),
                        tags: Some(ed.tags.clone()),
                        ..Default::default()
                    },
                );
                // Returned row carries the new UpdatedTime (issue 23 #6).
                if let Some(note) = saved.ok().flatten() {
                    if let Some(ed) = &mut self.editing {
                        ed.updated_time = note.updated_time;
                    }
                }
            }
        }
        if let Some(ed) = &mut self.editing {
            ed.dirty = false;
            ed.saved_content = content;
            ed.saved_flash = true;
        }
        self.invalidate_lists();
        cx.notify();
    }

    /// Issue 23 #6: commit the tag-add input (Enter), then flush. Empty or
    /// duplicate tags are ignored.
    fn add_tag(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let Some(ed) = &mut self.editing else { return };
        let raw = ed.tag_input.read(cx).value().to_string();
        let name = raw.trim().to_string();
        if name.is_empty() || ed.tags.iter().any(|t| t == &name) {
            return;
        }
        ed.tags.push(name);
        ed.dirty = true;
        self.save_editor(cx);
    }

    /// Issue 23 #6: drop a status-bar tag chip.
    fn remove_tag(&mut self, tag: String, cx: &mut gpui_kit::Context<Self>) {
        let Some(ed) = &mut self.editing else { return };
        let before = ed.tags.len();
        ed.tags.retain(|t| *t != tag);
        if ed.tags.len() != before {
            ed.dirty = true;
            self.save_editor(cx);
        }
    }

    /// Leave edit mode without saving.
    fn stop_editing(&mut self, cx: &mut gpui_kit::Context<Self>) {
        self.close_editor();
        cx.notify();
    }

    /// Persist vim_enabled to prefs.json (issue 17 Q12).
    fn save_vim_pref(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        p.vim_default = self.vim_enabled;
        p.save(&dir);
        let _ = cx;
    }

    /// Issue 17 Q7: appearance pref → disk + live theme replay.
    fn set_appearance(&mut self, ix: usize, cx: &mut gpui_kit::Context<Self>) {
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        let target = match ix {
            0 => crate::prefs::AppearancePref::Follow,
            1 => crate::prefs::AppearancePref::Light,
            _ => crate::prefs::AppearancePref::Dark,
        };
        // Issue 23 #2: picking the already-active mode still reloaded the
        // theme tables and repainted — a no-op click stays free now.
        if p.appearance == target {
            return;
        }
        p.appearance = target;
        p.save(&dir);
        // Panes read window.appearance(); drive it from the theme choice so
        // Light/Dark repaint the whole app (issue 19 #26). Applied BEFORE the
        // kit update below so the window's own appearance change (which fires
        // the observer → a second wake replay) lands after ours, not racing it.
        cx.set_window_appearance(match p.appearance {
            crate::prefs::AppearancePref::Follow => None,
            crate::prefs::AppearancePref::Light => Some(gpui_kit::WindowAppearance::Light),
            crate::prefs::AppearancePref::Dark => Some(gpui_kit::WindowAppearance::Dark),
        });
        // Mode switch, then wake overlay — TWO updates. One combined
        // update cannot work: Theme::edit re-applies the mode's registered
        // config AFTER the user closure (to reload mode-scoped tables), so
        // any color edits made inside are wiped. set_accent/main.rs use the
        // same stacked shape. Cost: one frame of kit-default colors before
        // the second update repaints.
        let palette = p.ui_theme.clone();
        let font = p.font;
        let editor_size = p.editor_font_size.clone();
        let zoom = p.zoom;
        let appearance = p.appearance;
        let sys_mode = gpui_kit::component::ThemeMode::from(cx.window_appearance());
        gpui_kit::component::Theme::update(cx, move |theme| {
            // Pick the target mode's registered config; Follow resolves the
            // system appearance (read before the closure — cx moves in).
            let mode = match appearance {
                crate::prefs::AppearancePref::Follow => sys_mode,
                crate::prefs::AppearancePref::Light => gpui_kit::component::ThemeMode::Light,
                crate::prefs::AppearancePref::Dark => gpui_kit::component::ThemeMode::Dark,
            };
            let config = if mode.is_dark() {
                theme.dark_theme.clone()
            } else {
                theme.light_theme.clone()
            };
            theme.apply_config(&config);
        });
        gpui_kit::component::Theme::update(cx, move |theme| {
            crate::apply_wake_theme(theme, &palette, &font, &editor_size, zoom);
        });
        cx.notify();
    }

    /// ADR-0010: theme family pref → disk + live replay. Family swap
    /// retints materials/fg/border and the editor highlight together.
    fn set_ui_theme(&mut self, id: &str, cx: &mut gpui_kit::Context<Self>) {
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        p.ui_theme = id.to_string();
        p.save(&dir);
        theme::set_family(id);
        let (family, font, editor_size, zoom) = (
            p.ui_theme.clone(),
            p.font,
            p.editor_font_size.clone(),
            p.zoom,
        );
        gpui_kit::component::Theme::update(cx, move |theme| {
            crate::apply_wake_theme(theme, &family, &font, &editor_size, zoom)
        });
        cx.notify();
    }

    /// Issue 19: accent color pref → disk + live swap.
    fn set_accent(&mut self, id: &str, cx: &mut gpui_kit::Context<Self>) {
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        p.accent = id.to_string();
        p.save(&dir);
        theme::set_accent(id);
        // Live swap must reach kit-owned surfaces (context-menu hover,
        // ghost-button hover read tokens.accent), not just theme::accent()
        // call sites — replay the wake overlay.
        let (palette, font, editor_size, zoom) = (
            p.ui_theme.clone(),
            p.font,
            p.editor_font_size.clone(),
            p.zoom,
        );
        gpui_kit::component::Theme::update(cx, move |theme| {
            crate::apply_wake_theme(theme, &palette, &font, &editor_size, zoom)
        });
        cx.notify();
    }

    /// Issue 17 Q7: language pref → disk + live switch.
    fn set_lang(&mut self, ix: usize, cx: &mut gpui_kit::Context<Self>) {
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        p.lang = match ix {
            1 => crate::prefs::LangPref::Zh,
            2 => crate::prefs::LangPref::En,
            _ => crate::prefs::LangPref::Follow,
        };
        p.save(&dir);
        self.lang = match p.lang {
            crate::prefs::LangPref::Follow => i18n::system_lang(),
            crate::prefs::LangPref::Zh => Lang::Zh,
            crate::prefs::LangPref::En => Lang::En,
        };
        cx.notify();
    }

    /// Issue 23 #5 / Issue 24: zoom pref → disk + live scale. The kit Root
    /// plugin re-stamps the window's rem size from `theme.font_size` every
    /// frame, so zoom is applied by scaling that font size (one Theme::update
    /// → sync_base + refresh_windows, same frame). `window.set_rem_size` was
    /// dead on arrival — overwritten before the next layout.
    fn set_zoom(&mut self, factor: f32, _window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        p.zoom = factor;
        p.save(&dir);
        let palette = p.ui_theme.clone();
        let font = p.font;
        let editor_size = p.editor_font_size.clone();
        gpui_kit::component::Theme::update(cx, move |theme| {
            crate::apply_wake_theme(theme, &palette, &font, &editor_size, factor)
        });
    }

    /// UI round 10 #6: UI font pref → disk + live swap. WenKai only takes
    /// effect after the ttf is installed (fonts.rs) and the app restarts —
    /// CoreText scans ~/Library/Fonts at process start.
    fn set_font_setting(&mut self, id: &str, cx: &mut gpui_kit::Context<Self>) {
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        p.font = id.to_string();
        p.save(&dir);
        let palette = p.ui_theme.clone();
        let font = p.font;
        let editor_size = p.editor_font_size.clone();
        let zoom = p.zoom;
        gpui_kit::component::Theme::update(cx, move |theme| {
            crate::apply_wake_theme(theme, &palette, &font, &editor_size, zoom)
        });
        cx.notify();
    }

    /// Round 5: nav glass strength → disk + live retint. Only the nav pane
    /// reads the pref (glass_base in render_nav), so a plain notify
    /// repaints it; no Theme::update needed.
    fn set_glass(&mut self, id: &str, cx: &mut gpui_kit::Context<Self>) {
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        p.glass = id.to_string();
        p.save(&dir);
        cx.notify();
    }

    /// UI round 12: editor font size pref (regular/medium/large) → disk +
    /// live editor resize. Zoom still multiplies this on top (set_zoom).
    fn set_editor_font_size(&mut self, ix: usize, cx: &mut gpui_kit::Context<Self>) {
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        p.editor_font_size = match ix {
            1 => "medium".into(),
            2 => "large".into(),
            _ => String::new(),
        };
        p.save(&dir);
        let palette = p.ui_theme.clone();
        let font = p.font;
        let editor_size = p.editor_font_size.clone();
        let zoom = p.zoom;
        gpui_kit::component::Theme::update(cx, move |theme| {
            crate::apply_wake_theme(theme, &palette, &font, &editor_size, zoom)
        });
        cx.notify();
    }

    /// Issue 23 #9: vim pref row (⌘K quick toggle stays).
    fn set_vim_setting(&mut self, on: bool, cx: &mut gpui_kit::Context<Self>) {
        self.vim_enabled = on;
        // Same-session effect: drop mode/pending when off, and sync the
        // open editor's caret shape (ADR-0005) — issue 24: without this the
        // current note kept the old caret/keys until the next note switch.
        if !self.vim_enabled {
            self.vim.reset();
        }
        if let Some(ed) = &self.editing {
            let content = ed.content.clone();
            content.update(cx, |state, cx| state.set_block_cursor(self.vim_enabled, cx));
        }
        self.save_vim_pref(cx);
        cx.notify();
    }

    /// Issue 17 Q11/Q12: one keydown captured on the editor column
    /// (capture phase — before the focused input's listeners and before
    /// AppKit turns the key into text). Returns true when vim consumed
    /// it; the caller must stop_propagation. Bare keys carry no kit
    /// bindings, so this listener is the first and only hook that sees
    /// them. Insert mode passes everything through except `escape`: the
    /// kit's Escape action consumes it while IME text is marked, so only
    /// the plain case reaches here.
    fn vim_key_down(
        &mut self,
        ks: &gpui_kit::Keystroke,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> bool {
        if !self.vim_enabled {
            return false;
        }
        let Some(ed) = self.editing.as_ref() else {
            return false;
        };
        if !ed.content.read(cx).has_selection_focus(window, cx) {
            return false;
        }
        let m = &ks.modifiers;
        if m.platform || m.control || m.alt {
            return false;
        }
        match self.vim.mode {
            crate::vim::VimMode::Insert if ks.key != "escape" => {
                return false; // insert mode: everything flows to the editor
            }
            crate::vim::VimMode::Insert => {
                return self.vim_consume("escape", None, window, cx);
            }
            _ => {}
        }
        // Normal/visual: a shifted letter keeps its lowercase key with
        // shift held (gpui-pre-macos events.rs) — restore the vim name
        // for G/V. Shifted symbols (`$`, `:`) already arrive shifted
        // with the flag cleared.
        let key = if m.shift && ks.key.len() == 1 && ks.key.chars().all(|c| c.is_ascii_lowercase())
        {
            ks.key.to_uppercase()
        } else {
            ks.key.clone()
        };
        let key_char = ks.key_char.as_deref().and_then(|s| s.chars().next());
        self.vim_consume(&key, key_char, window, cx)
    }

    /// Feed one key to the vim state machine and execute it against the
    /// focused editor (crate::vim::exec mutates EditorState directly —
    /// no kit-action round-trips, so no dispatch-reentry guard). Always
    /// consumed: unknown printables are swallowed too, so they never
    /// reach the editor as text.
    fn vim_consume(
        &mut self,
        key: &str,
        key_char: Option<char>,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> bool {
        let Some(ed) = self.editing.as_ref() else {
            return true; // no editor open: still eat the key (vim is on)
        };
        let editor = ed.content.clone();
        let cmd = editor.update(cx, |state, cx| {
            crate::vim::consume(&mut self.vim, state, key, key_char, window, cx)
        });
        match cmd {
            Some(crate::vim::Cmd::Write) => self.save_editor(cx),
            Some(crate::vim::Cmd::Quit) => self.stop_editing(cx),
            None => {}
        }
        // The editor entity notifies itself, but the mode label lives on
        // the shell — without this it repaints only on unrelated events
        // and shows a stale mode.
        cx.notify();
        true
    }

    /// Kit input action (enter/escape/backspace/delete) captured before
    /// the focused input's own handler. These keys carry kit bindings,
    /// so they dispatch as actions before any key listener —
    /// capture_action on the column is the only earlier hook. Non-insert
    /// modes only; insert falls through to the kit handler.
    fn vim_action_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> bool {
        if !self.vim_enabled {
            return false;
        }
        let Some(ed) = self.editing.as_ref() else {
            return false;
        };
        if !ed.content.read(cx).has_selection_focus(window, cx) {
            return false;
        }
        if self.vim.mode == crate::vim::VimMode::Insert {
            return false;
        }
        self.vim_consume(key, None, window, cx)
    }

    /// 斜杠面板打开时的方向键导航（用户需求 2）。方向键是 Move* 动作，
    /// capture_action 阶段到达这里；面板未开时不拦（编辑器正常移动）。
    fn slash_move(&mut self, dir: &str, cx: &mut gpui_kit::Context<Self>) {
        let Some(mut panel) = self.slash_panel.clone() else {
            return;
        };
        let keystroke = gpui_kit::Keystroke {
            key: dir.into(),
            ..Default::default()
        };
        let ev = KeyDownEvent {
            keystroke,
            is_held: false,
            prefer_character_input: false,
        };
        if crate::slash_panel::key(&mut panel, &ev, self.lang) == Some(true) {
            self.slash_panel = Some(panel);
        } else {
            self.slash_panel = None;
        }
        cx.notify();
        cx.stop_propagation();
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
            NavSelection::Notebook(id) => {
                store.notebook_title(id).unwrap_or_else(|_| id.to_string())
            }
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
            _ => self.nav = NavSelection::Recent,
        }
        self.selected = Some(note.note_id.clone());
        self.close_editor();
        self.invalidate_lists();
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
        if store
            .restore_history(&note_id, &history_id)
            .ok()
            .flatten()
            .is_some()
        {
            self.close_editor();
            self.selected = Some(note_id);
            self.invalidate_lists();
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
}

impl AppShell {
    /// Committed query → unified core search → rows, debounced 150ms
    /// (issue 17 Q6). IME-safe: the palette only calls this on
    /// `InputEvent::Change` (committed text).
    pub(crate) fn run_overlay_search(&mut self, query: &str, cx: &mut gpui_kit::Context<Self>) {
        self.search_query = query.to_string();
        self.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(150))
                .await;
            let _ = this.update(cx, |shell, cx| {
                let q = shell.search_query.clone();
                shell.search_rows = if crate::search_overlay::should_search(&q, false) {
                    shell.search_store(&q)
                } else {
                    Vec::new()
                };
                cx.notify();
            });
        }));
    }

    /// ↩ on a row: open the note in the editor (`selected` drives it), then
    /// close the overlay. The middle list keeps its filter/nav untouched.
    /// Issue 17 Q6: with an empty query, section 1 = recents (section 0 is
    /// quick actions, which carry their own actions).
    pub(crate) fn confirm_overlay(
        &mut self,
        index: gpui_kit::component::IndexPath,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if self.search_query.is_empty() && index.section == 1 {
            let recents = self.recent_rows();
            if let Some((id, _)) = recents.get(index.row).cloned() {
                self.select_note(leanote_core::Id(id), window, cx);
            }
        } else if !self.search_query.is_empty() {
            if let Some(row) = self.search_rows.get(index.row) {
                let id = leanote_core::Id(row.note_id.clone());
                self.select_note(id, window, cx);
            }
        }
        window.close_dialog(cx);
    }

    /// Issue 17 Q6: recent note ids (prefs) resolved to titles; dead ids
    /// (deleted notes) are skipped.
    pub(crate) fn recent_rows(&self) -> Vec<(String, String)> {
        let prefs = crate::prefs::Prefs::load(&self.data_dir);
        prefs
            .recents
            .iter()
            .filter_map(|id| {
                let note = self
                    .store()
                    .and_then(|s| s.get_note_any(&leanote_core::Id(id.clone())).ok().flatten())?;
                Some((id.clone(), note.title))
            })
            .collect()
    }
    fn search_store(&self, query: &str) -> Vec<crate::search_overlay::ResultRow> {
        let Some(store) = self.store() else {
            return Vec::new();
        };
        let hits = store
            .search(query, crate::search_overlay::RESULT_LIMIT)
            .unwrap_or_default();
        crate::search_overlay::format_rows(&hits, |id| {
            store.notebook_title(id).unwrap_or_else(|_| id.to_string())
        })
    }

    /// Star is local-only: toggle_star never touches IsDirty (GLOSSARY).
    fn toggle_star(&mut self, id: leanote_core::Id, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.toggle_star(&id);
        }
        self.invalidate_lists();
        cx.notify();
    }

    /// Blog publish (IsBlog): unlike star, this IS a synced field —
    /// set_note2blog marks the note dirty for the next push.
    fn set_note2blog(
        &mut self,
        id: leanote_core::Id,
        is_blog: bool,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Some(store) = self.store() {
            let _ = store.set_note2blog(&id, is_blog);
        }
        self.invalidate_lists();
        cx.notify();
    }

    /// Issue 18: context-menu delete — trash a live note (soft delete,
    /// syncs back to the server like the web UI's delete).
    fn delete_note(&mut self, id: leanote_core::Id, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.trash_note(&id);
        }
        if self.selected.as_ref() == Some(&id) {
            self.selected = None;
        }
        self.invalidate_lists();
        cx.notify();
    }

    /// Issue 18: context-menu move — reassign the note's notebook.
    fn move_note_to(
        &mut self,
        id: leanote_core::Id,
        notebook: String,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Some(store) = self.store() {
            let _ = store.set_notebook(&id, &leanote_core::Id(notebook));
        }
        self.nav_edit = None;
        self.invalidate_lists();
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
        self.invalidate_lists();
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
        self.invalidate_lists();
        cx.notify();
    }

    fn hard_delete_selected(&mut self, id: leanote_core::Id, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.hard_delete_note(&id);
        }
        self.selected = None;
        self.invalidate_lists();
        cx.notify();
    }

    /// Feature 3: batch ops loop the single-note store calls over the
    /// multi-selection. Selection is transient — cleared after a
    /// destructive batch.
    fn batch_star(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let ids = self.selected_ids.clone();
        if let Some(store) = self.store() {
            for id in &ids {
                let _ = store.toggle_star(id);
            }
        }
        self.invalidate_lists();
        cx.notify();
    }

    fn batch_delete(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let ids = self.selected_ids.clone();
        if let Some(store) = self.store() {
            for id in &ids {
                let _ = store.trash_note(id);
            }
        }
        self.selected = None;
        self.selected_ids.clear();
        self.sel_anchor = None;
        self.invalidate_lists();
        cx.notify();
    }

    fn batch_move_to(&mut self, notebook: String, cx: &mut gpui_kit::Context<Self>) {
        let ids = self.selected_ids.clone();
        if let Some(store) = self.store() {
            for id in &ids {
                let _ = store.set_notebook(id, &leanote_core::Id(notebook.clone()));
            }
        }
        self.nav_edit = None;
        self.invalidate_lists();
        cx.notify();
    }

    fn batch_set_note2blog(&mut self, is_blog: bool, cx: &mut gpui_kit::Context<Self>) {
        let ids = self.selected_ids.clone();
        if let Some(store) = self.store() {
            for id in &ids {
                let _ = store.set_note2blog(id, is_blog);
            }
        }
        self.invalidate_lists();
        cx.notify();
    }

    fn empty_trash(&mut self, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.empty_trash();
        }
        self.selected = None;
        self.invalidate_lists();
        cx.notify();
    }

    /// Empty-trash button behind an AlertDialog: empty_trash soft-deletes
    /// every trashed note and flags them dirty — irreversible after sync.
    fn confirm_empty_trash(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        let shell = cx.entity();
        let lang = self.lang;
        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            let shell_ok = shell.clone();
            alert
                .title(i18n::t(lang, i18n::Key::EmptyTrash))
                .description(i18n::t(lang, i18n::Key::EmptyTrashConfirmBody))
                .show_cancel(true)
                .ok_text(i18n::t(lang, i18n::Key::Confirm))
                .cancel_text(i18n::t(lang, i18n::Key::Cancel))
                .on_ok(move |_ev, _window, cx| {
                    let _ = shell_ok.update(cx, |this, cx| this.empty_trash(cx));
                    true
                })
        });
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

    /// One note (`id`) → HTML (`md=false`) or Markdown (`md=true`) file.
    /// Issue 21: id-parameterized so the row context menu exports its own
    /// row, not the selection.
    fn export_note_file(
        &mut self,
        id: leanote_core::Id,
        md: bool,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let Some(n) = self
            .store()
            .and_then(|s| s.get_note_any(&id).ok().flatten())
        else {
            return;
        };
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

    /// One note (`id`) → PDF rendered locally with typst. The server's
    /// exportPdf path is gone (it needs wkhtmltopdf the user's server lacks);
    /// rich-text notes are converted to markdown first.
    fn export_note_pdf(&mut self, id: leanote_core::Id, cx: &mut gpui_kit::Context<Self>) {
        let Some(n) = self
            .store()
            .and_then(|s| s.get_note_any(&id).ok().flatten())
        else {
            return;
        };
        let name = format!("{}.pdf", crate::export::safe_filename(&n.title));
        let title = n.title.clone();
        let src = crate::browse::reader_markdown_src(&n.title, &n.content, n.is_markdown);
        self.export_with_panel(name, cx, move |path| {
            let bytes = crate::pdf::note_to_pdf_title(&title, &src)?;
            std::fs::write(&path, bytes).map_err(|e| e.to_string())
        });
    }

    /// Whole library → one `.leanote` JSON backup (JS export-leanote plugin
    /// shape; notebooks + every live note).
    fn export_backup(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let name = "leanote-backup.leanote".to_string();
        let dir = self.data_dir.clone();
        let user = self.user_id.clone();
        self.export_with_panel(name, cx, move |path| {
            let store = leanote_core::Store::open(dir, user).map_err(|e| e.to_string())?;
            std::fs::write(&path, crate::export::backup_to_leanote_json(&store))
                .map_err(|e| e.to_string())
        });
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
        self.invalidate_lists();
        cx.notify();
    }
}

impl AppShell {
    // ---- Ticket 11: nav management actions (context menus + inline input) ----

    /// Modal notebook-name dialog shared by New and Rename (ticket 11
    /// follow-up: popup instead of the old inline row). `apply` runs on
    /// OK / Enter; Esc and Cancel just close.
    fn notebook_name_dialog(
        &mut self,
        title: i18n::Key,
        initial: Option<String>,
        apply: impl Fn(&mut Self, String, &mut gpui_kit::Context<Self>) + Clone + 'static,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let shell = cx.entity();
        let lang = self.lang;
        let name = cx.new(|cx| {
            let mut s = InputState::new(window, cx)
                .placeholder(i18n::t(lang, i18n::Key::NotebookNamePlaceholder));
            if let Some(v) = &initial {
                s = s.default_value(v.clone());
            }
            s
        });
        let first_focus = std::cell::Cell::new(true);
        let t = move |k: i18n::Key| i18n::t(lang, k);
        window.open_dialog(cx, move |dialog, window, cx| {
            if first_focus.replace(false) {
                name.update(cx, |s, cx| s.focus(window, cx));
            }
            let apply = apply.clone();
            let ok = {
                let shell = shell.clone();
                let name = name.clone();
                move |_ev: &gpui_kit::ClickEvent, _window: &mut Window, cx: &mut gpui_kit::App| {
                    let value = name.read(cx).value().to_string();
                    let value = value.trim().to_string();
                    if !value.is_empty() {
                        shell.update(cx, |this, cx| apply(this, value, cx));
                    }
                    true
                }
            };
            dialog
                .w(px(360.))
                .title(t(title))
                .child(Input::new(&name).appearance(true).bordered(true).h_8())
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|b| b.label(t(i18n::Key::Cancel))))
                        .child(
                            DialogAction::new().child(
                                Button::new("notebook-name-ok")
                                    .label(t(i18n::Key::Confirm))
                                    .primary(),
                            ),
                        ),
                )
                .on_ok(ok)
        });
    }

    /// Regular confirm dialog (Cancel / Log out) for both logout entries.
    fn logout_confirm_dialog(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        let shell = cx.entity();
        let lang = self.lang;
        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            let shell_ok = shell.clone();
            alert
                .title(i18n::t(lang, i18n::Key::LogoutConfirmTitle))
                .description(i18n::t(lang, i18n::Key::LogoutConfirmBody))
                .show_cancel(true)
                .ok_text(i18n::t(lang, i18n::Key::Logout))
                .cancel_text(i18n::t(lang, i18n::Key::Cancel))
                .on_ok(move |_ev, window, cx| {
                    // Runs while the alert layer is topmost: logout pops
                    // every dialog itself, then returning true lets kit
                    // close its (already-empty) layer harmlessly.
                    let _ = shell_ok.update(cx, |this, cx| this.logout(window, cx));
                    true
                })
        });
    }

    fn begin_new_notebook(
        &mut self,
        parent: String,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        self.notebook_name_dialog(
            i18n::Key::NewNotebook,
            None,
            move |this, title, cx| {
                if let Some(store) = this.store() {
                    let _ = store.create_notebook(&title, &parent);
                }
                this.invalidate_lists();
                cx.notify();
            },
            window,
            cx,
        );
    }

    fn begin_rename_notebook(
        &mut self,
        id: leanote_core::Id,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let current = self
            .store()
            .and_then(|s| s.notebook_title(&id).ok())
            .unwrap_or_default();
        self.notebook_name_dialog(
            i18n::Key::RenameNotebook,
            Some(current),
            move |this, title, cx| {
                if let Some(store) = this.store() {
                    let _ = store.rename_notebook(&id, &title);
                }
                this.invalidate_lists();
                cx.notify();
            },
            window,
            cx,
        );
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
            // MoveNotebook / MoveNote(Batch) resolve in their *_to fns.
            Some(NavEdit::MoveNotebook { .. })
            | Some(NavEdit::MoveNote { .. })
            | Some(NavEdit::MoveNoteBatch)
            | None => {}
        }
        self.invalidate_lists();
        cx.notify();
    }

    /// Context-menu "Move to…" target chosen: reparent as the last child.
    /// (Full drag-reorder is the drag-drop ticket's UI; Seq rewriting and
    /// the sibling-group push semantics are identical either way.)
    fn move_notebook_to(
        &mut self,
        id: leanote_core::Id,
        parent: String,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Some(store) = self.store() {
            let _ = store.move_notebook(&id, &parent, usize::MAX, &[]);
        }
        self.nav_edit = None;
        self.invalidate_lists();
        // Same as drop_notebook: reparenting is server-visible, sync now.
        self.sync_now(cx);
        cx.notify();
    }

    /// Delete after an AlertDialog confirm. The body warns when the subtree
    /// holds live notes (they go to trash, per ticket 11).
    fn delete_notebook_nav(
        &mut self,
        id: leanote_core::Id,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let shell = cx.entity();
        let lang = self.lang;
        let store = self.store();
        // JS delete guards run in this order: sub-notebooks first, then
        // notes — only an empty notebook can be deleted.
        let has_subs = store
            .as_ref()
            .and_then(|s| s.has_sub_notebooks(&id).ok())
            .unwrap_or(false);
        let count = store
            .as_ref()
            .and_then(|s| s.notebook_subtree_note_count(&id).ok())
            .unwrap_or(0);
        let id_str = id.to_string();
        // Guard: sub-notebooks must be deleted first (JS Notebook.getSubNotebooks check).
        if has_subs {
            window.open_alert_dialog(cx, move |alert, _window, _cx| {
                alert
                    .title(i18n::t(lang, i18n::Key::DeleteNotebookTitle))
                    .description(i18n::t(lang, i18n::Key::DeleteNotebookHasSubNotebooks))
                    .ok_text(i18n::t(lang, i18n::Key::Confirm))
            });
            return;
        }
        // Guard: notes must be deleted first (JS NotebookService.deleteNotebook check).
        if count > 0 {
            let body = SharedString::from(
                i18n::t(lang, i18n::Key::DeleteNotebookHasNotesBody)
                    .replace("{n}", &count.to_string()),
            );
            window.open_alert_dialog(cx, move |alert, _window, _cx| {
                alert
                    .title(i18n::t(lang, i18n::Key::DeleteNotebookTitle))
                    .description(body.clone())
                    .ok_text(i18n::t(lang, i18n::Key::Confirm))
            });
            return;
        }
        // Empty: confirm and delete.
        let body = SharedString::from(i18n::t(lang, i18n::Key::DeleteNotebookEmptyBody));
        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            let shell_ok = shell.clone();
            let id_ok = id_str.clone();
            alert
                .title(i18n::t(lang, i18n::Key::DeleteNotebookTitle))
                .description(body.clone())
                .show_cancel(true)
                .ok_text(i18n::t(lang, i18n::Key::DeleteNotebook))
                .cancel_text(i18n::t(lang, i18n::Key::Cancel))
                .on_ok(move |_ev, _window, cx| {
                    let _ = shell_ok.update(cx, |this, cx| {
                        let id = leanote_core::Id(id_ok.clone());
                        if let Some(store) = this.store() {
                            let _ = store.delete_notebook(&id);
                        }
                        if this.nav == NavSelection::Notebook(id.to_string()) {
                            this.nav = NavSelection::Recent;
                            this.selected = None;
                        }
                        this.invalidate_lists();
                        cx.notify();
                    });
                    true
                })
        });
    }

    fn delete_tag_nav(&mut self, tag: String, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.delete_tag(&tag);
        }
        if self.nav == NavSelection::Tag(tag) {
            self.nav = NavSelection::Recent;
            self.selected = None;
        }
        self.invalidate_lists();
        cx.notify();
    }
}

/// One note entry for the virtualized middle list. Issue 18: fixed row
/// height so every row (and thus its selection band) is identical —
/// uniform_list is an equal-height model. Title is one truncated line;
/// meta and excerpt likewise (compact rows, see NOTE_ROW_H).
/// Horizontal inset: the row pads itself 5px (selection-band breathing
/// room); the column adds its own padding on top.
/// `shell` (weak) routes the click to select_note — uniform_list closures
/// only get `&mut App`, not the view context.
fn note_list_row(
    shell: &gpui_kit::WeakEntity<AppShell>,
    n: &leanote_core::NoteMeta,
    meta: SharedString,
    desc: SharedString,
    is_sel: bool,
    divided: bool,
    dark: bool,
    fg: gpui_kit::Hsla,
    sel_fg: gpui_kit::Hsla,
    muted: gpui_kit::Hsla,
) -> impl IntoElement + use<> {
    let shell = shell.clone();
    let shell_menu = shell.clone();
    let id = n.note_id.clone();
    let id_menu = n.note_id.clone();
    let row_fg = if is_sel { sel_fg } else { fg };
    let row_muted = if is_sel { sel_fg } else { muted };
    gpui_kit::base::v_flex()
        .id(n.note_id.to_string())
        .h(theme::ui(NOTE_ROW_H))
        .w_full()
        // px(), not py_1_5()/gap_0p5(): those are rem units (0.375/0.125 rem)
        // and uniform_list's measure_item lays the item out as root without
        // the shell's rem context — the resolved padding inflates ~4x
        // (measured 25px/side) and the row grows 75 -> 125px, leaving a
        // blank block under the text. Absolute px pins the row at
        // NOTE_ROW_H.
        .justify_center()
        .gap(px(1.))
        .px(px(5.))
        .rounded(px(theme::RADIUS_SELECTION))
        .when(is_sel, |el| el.bg(theme::accent()))
        .on_click(move |ev: &gpui_kit::ClickEvent, window, cx| {
            let mods = ev.modifiers();
            let _ = shell.update(cx, |this: &mut AppShell, cx| {
                this.apply_selection(id.clone(), mods, window, cx)
            });
        })
        .context_menu({
            let shell = shell_menu;
            let id = id_menu;
            let starred = n.star;
            let is_blog = n.is_blog;
            let lang = crate::i18n::system_lang();
            move |menu, _window, cx| {
                // Feature 3: batch menu when the live multi-selection
                // covers this row; otherwise the per-note menu.
                let batch = shell
                    .read_with(cx, |s, _| {
                        s.selected_ids.len() > 1 && s.selected_ids.contains(&id)
                    })
                    .unwrap_or(false);
                if batch {
                    let count = shell
                        .read_with(cx, |s, _| s.selected_ids.len())
                        .unwrap_or(0);
                    note_row_menu_batch(menu, &shell, lang, count)
                } else {
                    note_row_menu(menu, &shell, &id, lang, starred, is_blog)
                }
            }
        })
        .child(
            // Title block: one truncated line. The star rides in the
            // corner without stealing title width.
            gpui_kit::base::h_flex()
                .gap_1()
                .child(
                    gpui_kit::div().flex_1().min_w_0().truncate().child(
                        Label::new(n.title.clone())
                            .text_size(theme::ui(14.))
                            .line_height(theme::ui(19.))
                            .text_color(row_fg),
                    ),
                )
                .when(n.is_blog, |el| {
                    el.child(
                        Icon::new(IconName::Globe)
                            .xsmall()
                            .flex_none()
                            .text_color(row_muted),
                    )
                })
                .when(n.star, |el| {
                    el.child(
                        Icon::new(IconName::StarFill)
                            .xsmall()
                            .flex_none()
                            .text_color(row_muted),
                    )
                }),
        )
        .child(
            gpui_kit::div()
                .truncate()
                .flex_shrink_0()
                .child(small_label(meta, row_muted)),
        )
        .child(
            // Issue 19: content excerpt — single truncated line.
            // truncate() alone shrinks to 0 height in a column flex
            // (overflow_hidden divs measure 0 under compression) —
            // flex_shrink_0 keeps the line's natural 20px.
            gpui_kit::div()
                .truncate()
                .flex_shrink_0()
                .child(small_label(desc, row_muted)),
        )
        .when(divided && !is_sel, |el| {
            // Issue 23 #11: 1px separator under the row — last row and the
            // selected accent band stay clean.
            el.child(
                gpui_kit::div()
                    .h(theme::ui(NOTE_ROW_DIVIDER_H))
                    .w_full()
                    .bg(theme::border(dark)),
            )
        })
}

/// Issue 18/21: right-click menu on a note row — star toggle + exports
/// (issue 21) and move/delete. Menu actions act on their OWN row: the
/// export helpers take an explicit note id, not the selection.
fn note_row_menu(
    menu: gpui_kit::component::menu::PopupMenu,
    shell: &gpui_kit::WeakEntity<AppShell>,
    id: &leanote_core::Id,
    lang: Lang,
    starred: bool,
    is_blog: bool,
) -> gpui_kit::component::menu::PopupMenu {
    let shell_mv = shell.clone();
    let shell_del = shell.clone();
    let shell_star = shell.clone();
    let shell_blog = shell.clone();
    let shell_html = shell.clone();
    let shell_md = shell.clone();
    let shell_pdf = shell.clone();
    let id_mv = id.clone();
    let id_del = id.clone();
    let id_star = id.clone();
    let id_html = id.clone();
    let id_md = id.clone();
    let id_pdf = id.clone();
    let id_blog = id.clone();
    menu.item(
        PopupMenuItem::new(i18n::t(lang, i18n::Key::MoveNoteTo)).on_click(
            move |_ev, _window, cx| {
                let _ = shell_mv.update(cx, |this, cx| {
                    this.nav_edit = Some(NavEdit::MoveNote { id: id_mv.clone() });
                    cx.notify();
                });
            },
        ),
    )
    .separator()
    .item(
        PopupMenuItem::new(if starred {
            i18n::t(lang, i18n::Key::UnstarNote)
        } else {
            i18n::t(lang, i18n::Key::StarNote)
        })
        .on_click(move |_ev, _window, cx| {
            let _ = shell_star.update(cx, |this, cx| this.toggle_star(id_star.clone(), cx));
        }),
    )
    .item(
        PopupMenuItem::new(if is_blog {
            i18n::t(lang, i18n::Key::UnpublishBlog)
        } else {
            i18n::t(lang, i18n::Key::PublishBlog)
        })
        .on_click(move |_ev, _window, cx| {
            let _ = shell_blog.update(cx, |this, cx| {
                this.set_note2blog(id_blog.clone(), !is_blog, cx)
            });
        }),
    )
    .item(
        PopupMenuItem::new(i18n::t(lang, i18n::Key::ExportHtml)).on_click(
            move |_ev, _window, cx| {
                let _ = shell_html.update(cx, |this, cx| {
                    this.export_note_file(id_html.clone(), false, cx)
                });
            },
        ),
    )
    .item(
        PopupMenuItem::new(i18n::t(lang, i18n::Key::ExportMarkdown)).on_click(
            move |_ev, _window, cx| {
                let _ = shell_md.update(cx, |this, cx| {
                    this.export_note_file(id_md.clone(), true, cx)
                });
            },
        ),
    )
    .item(
        PopupMenuItem::new(i18n::t(lang, i18n::Key::ExportPdf)).on_click(
            move |_ev, _window, cx| {
                let _ = shell_pdf.update(cx, |this, cx| this.export_note_pdf(id_pdf.clone(), cx));
            },
        ),
    )
    .separator()
    .item(
        PopupMenuItem::new(i18n::t(lang, i18n::Key::DeleteNote)).on_click(
            move |_ev, _window, cx| {
                let _ = shell_del.update(cx, |this, cx| this.delete_note(id_del.clone(), cx));
            },
        ),
    )
}

/// Feature 3: batch menu while the multi-selection covers the clicked
/// row. Exports/PDF stay single-note (export_note_* take one id; a zip
/// exporter is out of scope).
fn note_row_menu_batch(
    menu: gpui_kit::component::menu::PopupMenu,
    shell: &gpui_kit::WeakEntity<AppShell>,
    lang: Lang,
    count: usize,
) -> gpui_kit::component::menu::PopupMenu {
    let shell_star = shell.clone();
    let shell_blog = shell.clone();
    let shell_del = shell.clone();
    let shell_mv = shell.clone();
    menu.item(
        PopupMenuItem::new(format!(
            "{} ({})",
            i18n::t(lang, i18n::Key::MoveNoteTo),
            count
        ))
        .on_click(move |_ev, _window, cx| {
            // Reuse the single-note move picker; the commit routes to
            // batch_move_to while a multi-select is live.
            let _ = shell_mv.update(cx, |this, cx| {
                this.nav_edit = Some(NavEdit::MoveNoteBatch);
                cx.notify();
            });
        }),
    )
    .separator()
    .item(
        PopupMenuItem::new(i18n::t(lang, i18n::Key::StarNote)).on_click(move |_ev, _window, cx| {
            let _ = shell_star.update(cx, |this, cx| this.batch_star(cx));
        }),
    )
    .item(
        PopupMenuItem::new(i18n::t(lang, i18n::Key::PublishBlog)).on_click(
            move |_ev, _window, cx| {
                let _ = shell_blog.update(cx, |this, cx| this.batch_set_note2blog(true, cx));
            },
        ),
    )
    .separator()
    .item(
        PopupMenuItem::new(i18n::t(lang, i18n::Key::DeleteNote)).on_click(
            move |_ev, _window, cx| {
                let _ = shell_del.update(cx, |this, cx| this.batch_delete(cx));
            },
        ),
    )
}

impl Render for AppShell {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> impl IntoElement {
        let dark = self.dark(cx);
        let lang = self.lang;
        let t = move |k: i18n::Key| i18n::t(lang, k);
        // Round 3: root stays TRANSPARENT so the NSVisualEffectView blur
        // reaches the nav pane; every other pane paints an opaque surface
        // over it (list: double .bg below, reader/editor: Background).
        // Root painting Material::Base would glass the whole window.
        let base_bg = gpui_kit::transparent_black();

        // Root paints nothing; panes own their backgrounds.
        let root = gpui_kit::base::v_flex()
            .id("shell-root")
            .track_focus(&self.root_focus)
            .size_full()
            .bg(base_bg)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(
                |this, _: &crate::search_overlay::ToggleFocusMode, _window, cx| {
                    // Issue 19: hide left + middle panes for pure reading.
                    this.focus_mode = !this.focus_mode;
                    cx.notify();
                },
            ))
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, _window, cx| {
                // Issue 21 需求10: ESC leaves the full-window preview. A
                // capture_action can't be used: Escape's key binding lives
                // in the Input context, which isn't rendered while the
                // preview is open, so the action never dispatches.
                if this.full_preview
                    && ev.keystroke.key == "escape"
                    && !ev.keystroke.modifiers.platform
                    && !ev.keystroke.modifiers.control
                {
                    this.full_preview = false;
                    cx.notify();
                }
            }))
            .on_action(cx.listener(
                |this, _: &crate::search_overlay::ToggleFullPreview, window, cx| {
                    // Issue 21 需求10: only meaningful with a selected note.
                    if this.selected.is_some() {
                        this.full_preview = !this.full_preview;
                        if this.full_preview {
                            window.focus(&this.preview_focus, cx);
                        } else if let Some(ed) = this.editing.as_ref() {
                            let handle = gpui_kit::Focusable::focus_handle(ed.content.read(cx), cx);
                            window.focus(&handle, cx);
                        }
                        cx.notify();
                    }
                },
            ))
            .on_action(cx.listener(
                |this, _: &crate::search_overlay::ToggleSearch, window, cx| {
                    // Any phase: the dialog layer is window-level.
                    this.toggle_search(window, cx);
                },
            ))
            .on_action(cx.listener(
                |this, _: &crate::search_overlay::OpenSettings, window, cx| {
                    // Issue 20: ⌘, / Ctrl+, (see bind_keys in main.rs).
                    window.close_dialog(cx);
                    this.open_settings(window, cx);
                },
            ))
            .on_action(cx.listener(
                |this, _: &crate::search_overlay::QuickNewNote, window, cx| {
                    window.close_dialog(cx);
                    this.new_note(window, cx);
                },
            ))
            .on_action(cx.listener(
                |this, _: &crate::search_overlay::QuickSyncNow, window, cx| {
                    window.close_dialog(cx);
                    this.sync_now(cx);
                },
            ));
        // Issue 17: no separate titlebar strip. The left nav's 44px header
        // region is the drag area (see render_nav); every pane runs flush
        // to the window top edge.

        // Issue 18: panes are user-resizable; widths persist (prefs).
        // The group wraps every phase's three children in panels so the
        // saved geometry applies from the login frame onward.
        let prefs = crate::prefs::Prefs::load(&self.data_dir);
        let widths = prefs.pane_widths;
        let mut body = gpui_kit::component::h_resizable("main-panes").on_resize({
            let dir = self.data_dir.clone();
            move |state, _window, cx| {
                let sizes = state.read(cx).sizes();
                if sizes.len() < 2 {
                    return;
                }
                let mut p = crate::prefs::Prefs::load(&dir);
                // Issue 19: outer group = [left | rest]; mid is owned by
                // the nested "mid-reader" group.
                p.pane_widths.left = Some(sizes[0].as_f32().round());
                p.save(&dir);
            }
        });

        match &self.phase {
            Phase::Login | Phase::SigningIn | Phase::Failed { .. } => {
                // Not signed in: right pane IS the login form (spec §1).
                // Side panels stay as slots but collapse to zero — keeps
                // the same 3-panel tree (and divider state) as the shell
                // so post-login layout is untouched (same trick as ⌘T
                // full preview).
                let error = match &self.phase {
                    Phase::Failed { error } => Some(error.clone()),
                    _ => None,
                };
                let busy = matches!(self.phase, Phase::SigningIn);
                // Built here: on_click needs cx.listener (Context of Self).
                let sign_in = if busy {
                    Button::new("sign-in")
                        .label(t(i18n::Key::SigningIn))
                        .primary()
                        .disabled(true)
                } else {
                    Button::new("sign-in")
                        .label(t(i18n::Key::SignIn))
                        .primary()
                        .on_click(cx.listener(|this, _ev, _window, cx| this.start_login(cx)))
                };
                body = body
                    .child(
                        resizable_panel()
                            .size(px(widths.left.unwrap_or(LEFT_W)))
                            .size_range(px(LEFT_RANGE.0)..px(LEFT_RANGE.1))
                            .visible(false),
                    )
                    .child(
                        resizable_panel()
                            .size(px(widths.mid.unwrap_or(MID_W)))
                            .size_range(px(MID_RANGE.0)..px(MID_RANGE.1))
                            .visible(false),
                    )
                    .child(resizable_panel().child(render_login(
                        dark,
                        error,
                        sign_in,
                        &t,
                        &self.host,
                        &self.email,
                        &self.password,
                    )));
            }
            Phase::Syncing { lines, done, total } => {
                body = body
                    .child(
                        resizable_panel()
                            .size(px(widths.left.unwrap_or(LEFT_W)))
                            .size_range(px(LEFT_RANGE.0)..px(LEFT_RANGE.1))
                            .visible(false),
                    )
                    .child(
                        resizable_panel()
                            .size(px(widths.mid.unwrap_or(MID_W)))
                            .size_range(px(MID_RANGE.0)..px(MID_RANGE.1))
                            .visible(false),
                    )
                    .child(resizable_panel().child(render_syncing(dark, lines, *done, *total, &t)));
            }
            Phase::SyncDone(summary) => {
                body = body
                    .child(
                        resizable_panel()
                            .size(px(widths.left.unwrap_or(LEFT_W)))
                            .size_range(px(LEFT_RANGE.0)..px(LEFT_RANGE.1))
                            .visible(false),
                    )
                    .child(
                        resizable_panel()
                            .size(px(widths.mid.unwrap_or(MID_W)))
                            .size_range(px(MID_RANGE.0)..px(MID_RANGE.1))
                            .visible(false),
                    )
                    .child(resizable_panel().child(render_sync_done(dark, summary, &t, cx)));
            }
            Phase::Shell { .. } if self.full_preview => {
                // Issue 21 需求10: ⌘T full-window preview — the selected
                // note's markdown (editor buffer while editing) rendered
                // edge-to-edge; ESC/⌘T/✕ leave.
                //
                // Layout bug fix: the preview must keep the SAME 3-panel
                // tree (nav | list | reader) as the normal shell. Swapping
                // in a single-panel body let `sync_panels_count` truncate
                // the group state (3 → 1) and re-expand it (1 → 3) on
                // exit, which redistributed sizes and lost the user's
                // dragged divider positions. `visible(false)` keeps the
                // panel slots (and their recorded sizes) while collapsing
                // them, so leaving the preview restores the exact layout.
                let close = self.selected.is_none();
                if close {
                    self.full_preview = false;
                }
                let src = self
                    .editing
                    .as_ref()
                    .map(|ed| ed.content.read(cx).value().to_string())
                    .or_else(|| {
                        self.selected
                            .clone()
                            .and_then(|id| {
                                self.store()
                                    .and_then(|s| s.get_note_any(&id).ok().flatten())
                            })
                            .map(|n| n.content)
                    });
                let title = self
                    .editing
                    .as_ref()
                    .map(|ed| ed.title.read(cx).value().to_string())
                    .or_else(|| {
                        self.selected
                            .clone()
                            .and_then(|id| {
                                self.store()
                                    .and_then(|s| s.get_note_any(&id).ok().flatten())
                            })
                            .map(|n| n.title)
                    });
                if let (Some(_title), Some(content)) = (title, src) {
                    let content = self
                        .store()
                        .map(|s| browse::resolve_note_images(&content, &s))
                        .unwrap_or(content);
                    let preview_px =
                        px(theme::preview_font_px(&prefs.editor_font_size, prefs.zoom));
                    // Comfortable reading frame (round 32): a capped,
                    // centered 1120px column (mx_auto leaves the rest as
                    // margin — the wider the window, the bigger the gutters)
                    // plus 48px inner padding. A lone note edge-to-edge on a
                    // fullscreen window reads as a wall of text. Top stays
                    // slim (just the close button), bottom gets extra room
                    // past the scroll end.
                    let preview = gpui_kit::base::v_flex()
                        .id("full-preview")
                        .track_focus(&self.preview_focus)
                        .debug_selector(|| "full-preview".into())
                        .size_full()
                        .pt(px(24.))
                        .pb(px(48.))
                        .px(px(48.))
                        .max_w(px(1120.))
                        .mx_auto()
                        .text_size(preview_px)
                        .child(
                            gpui_kit::base::h_flex().justify_end().child(
                                Button::new("full-preview-close")
                                    .icon(IconName::CircleX)
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.full_preview = false;
                                        cx.notify();
                                    })),
                            ),
                        )
                        .child(
                            crate::mermaid::install(
                                TextView::markdown("full-preview", content),
                                dark,
                            )
                            .scrollable(true)
                            .on_link_click(|url, _ev, _window, cx| cx.open_url(url)),
                        );
                    body = body
                        .child(
                            resizable_panel()
                                .size(px(widths.left.unwrap_or(LEFT_W)))
                                .size_range(px(LEFT_RANGE.0)..px(LEFT_RANGE.1))
                                .visible(false),
                        )
                        .child(
                            resizable_panel()
                                .size(px(widths.mid.unwrap_or(MID_W)))
                                .size_range(px(MID_RANGE.0)..px(MID_RANGE.1))
                                .visible(false),
                        )
                        .child(resizable_panel().child(preview));
                } else {
                    self.full_preview = false;
                }
            }
            Phase::Shell { .. } => {
                let dir = self.data_dir.clone();
                let widths2 = widths;
                let inner = gpui_kit::component::h_resizable("mid-reader")
                    .on_resize(move |state, _window, cx| {
                        let sizes = state.read(cx).sizes();
                        if sizes.len() < 2 {
                            return;
                        }
                        let mut p = crate::prefs::Prefs::load(&dir);
                        p.pane_widths.mid = Some(sizes[0].as_f32().round());
                        p.save(&dir);
                    })
                    .when(!self.focus_mode, |el| {
                        el.child(
                            resizable_panel()
                                .size(px(widths2.mid.unwrap_or(MID_W)))
                                .size_range(px(MID_RANGE.0)..px(MID_RANGE.1))
                                .child(self.render_list(dark, &t, window, cx)),
                        )
                    });
                let reader = resizable_panel().child(self.render_reader(dark, &t, window, cx));
                body = if self.focus_mode {
                    body.child(reader)
                } else {
                    body.child(
                        resizable_panel()
                            .size(px(widths.left.unwrap_or(LEFT_W)))
                            .size_range(px(LEFT_RANGE.0)..px(LEFT_RANGE.1))
                            .child(self.render_nav(dark, &t, window, cx)),
                    )
                    .child(resizable_panel().child(inner.child(reader)))
                };
            }
        }

        root.child(body)
    }
}

fn small_label(text: impl Into<SharedString>, color: gpui_kit::Hsla) -> Label {
    Label::new(text).text_size(theme::ui(13.)).text_color(color)
}

// --- Notebook sidebar drag-reorder (ticket 11 follow-up) ---

/// Drag payload: id + title so the ghost renders without store access.
#[derive(Clone, Debug)]
struct NotebookDrag {
    id: String,
    title: String,
}

/// Ghost shown under the pointer while dragging (`on_drag` render view).
struct NotebookDragGhost(SharedString);

impl Render for NotebookDragGhost {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        gpui_kit::base::h_flex()
            .id("notebook-drag-ghost")
            .gap_2()
            .px_2()
            .py_1()
            .rounded(px(6.))
            .bg(theme::accent())
            .shadow_md()
            .child(
                Icon::new(AssetIcon::FolderClosed)
                    .small()
                    .text_color(theme::accent_foreground()),
            )
            .child(
                Label::new(self.0.clone())
                    .text_size(theme::ui(13.))
                    .text_color(theme::accent_foreground()),
            )
    }
}

/// State for the manual window-drag region (mirrors kit TitleBar's TODO
/// workaround for GPUI pre-0.2.3).
struct DragHeaderState {
    should_move: bool,
}

impl Render for DragHeaderState {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        gpui_kit::div()
    }
}

fn drag_header(
    window: &mut Window,
    lang: i18n::Lang,
    cx: &mut gpui_kit::Context<AppShell>,
) -> impl IntoElement + use<> {
    let state = window.use_state(cx, |_, _| DragHeaderState { should_move: false });
    let is_macos = cfg!(target_os = "macos");
    gpui_kit::div()
        .id("nav-drag-header")
        .flex()
        .flex_none()
        .items_center()
        .h(theme::ui(TITLEBAR_H))
        .w_full()
        .pl(px(if is_macos { 80. } else { 0. }))
        .when(is_macos, |this| {
            this.on_double_click(|_, window, _| window.titlebar_double_click())
        })
        .on_mouse_down_out(window.listener_for(&state, |s, _, _, _| s.should_move = false))
        .on_mouse_down(
            MouseButton::Left,
            window.listener_for(&state, |s, _, _, _| s.should_move = true),
        )
        .on_mouse_up(
            MouseButton::Left,
            window.listener_for(&state, |s, _, _, _| s.should_move = false),
        )
        .on_mouse_move(window.listener_for(&state, |s, _, window, _| {
            if s.should_move {
                s.should_move = false;
                window.start_window_move();
            }
        }))
        .window_control_area(WindowControlArea::Drag)
        // Issue 20: no app-name label — the strip stays a 44px drag region
        // with the search button (user ruling Q7).
        // Issue 19: search entry point in the header (opens the ⌘K overlay).
        .child(
            gpui_kit::base::h_flex()
                .flex_1()
                .justify_end()
                .pr_2()
                .child(
                    Button::new("header-search")
                        .icon(AssetIcon::Search)
                        .ghost()
                        .small()
                        .tooltip(i18n::t(lang, i18n::Key::TooltipSearch))
                        .on_click(
                            cx.listener(|this, _ev, window, cx| this.toggle_search(window, cx)),
                        ),
                ),
        )
        // ADR-0011: Windows/Linux 自绘窗口控制按钮（macOS 交通灯由系统
        // 绘制）。每块区域带 WindowControlArea hitbox，平台层据此做
        // NCHITTEST/原生点击；按钮 UI 只是视觉提示，点击不打事件。
        .when(!is_macos, |this| {
            this.child(
                gpui_kit::base::h_flex()
                    .flex_none()
                    .items_center()
                    .child(win_control_btn(
                        "win-min",
                        AssetIcon::Minus,
                        WindowControlArea::Min,
                    ))
                    .child(win_control_btn(
                        "win-max",
                        AssetIcon::Square,
                        WindowControlArea::Max,
                    ))
                    .child(win_control_btn(
                        "win-close",
                        AssetIcon::X,
                        WindowControlArea::Close,
                    )),
            )
        })
}

/// ADR-0011: 一个自绘窗口控制按钮（Windows/Linux）。hitbox 交给平台
/// 层处理，元素本身只提供 44×32 视觉区 + hover 反馈。
fn win_control_btn(
    id: &'static str,
    icon: AssetIcon,
    area: WindowControlArea,
) -> impl IntoElement + use<> {
    gpui_kit::div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(theme::ui(44.))
        .h(theme::ui(32.))
        .hover(|s| s.bg(gpui_kit::rgba(0x00000014)))
        .child(Icon::new(icon))
        .window_control_area(area)
}

/// Issue 17/21: transparent drag strip at the top of the right pane — drag
/// region only, no content. Reader keeps the 28px strip; the editor uses a
/// slim 10px strip (issue 21) so the title sits closer to the top.
fn reader_drag_strip(
    height: f32,
    window: &mut Window,
    cx: &mut gpui_kit::Context<AppShell>,
) -> impl IntoElement + use<> {
    let state = window.use_state(cx, |_, _| DragHeaderState { should_move: false });
    let is_macos = cfg!(target_os = "macos");
    gpui_kit::div()
        .id("reader-drag-strip")
        .flex()
        .flex_none()
        .items_center()
        .h(theme::ui(height))
        .w_full()
        .when(is_macos, |this| {
            this.on_double_click(|_, window, _| window.titlebar_double_click())
        })
        .on_mouse_down_out(window.listener_for(&state, |s, _, _, _| s.should_move = false))
        .on_mouse_down(
            MouseButton::Left,
            window.listener_for(&state, |s, _, _, _| s.should_move = true),
        )
        .on_mouse_up(
            MouseButton::Left,
            window.listener_for(&state, |s, _, _, _| s.should_move = false),
        )
        .on_mouse_move(window.listener_for(&state, |s, _, window, _| {
            if s.should_move {
                s.should_move = false;
                window.start_window_move();
            }
        }))
        .window_control_area(WindowControlArea::Drag)
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
    let title = Label::new(t(i18n::Key::LoginTitle))
        .text_size(theme::ui(22.))
        .text_color(fg);
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

    let mut form = gpui_kit::base::v_flex()
        .gap_3()
        .w(theme::ui(360.))
        .child(title)
        .child(subtitle)
        .child(host_field)
        .child(email_field)
        .child(password_field)
        .child(sign_in);
    if let Some(err) = error {
        form = form.child(
            Label::new(format!(
                "{} / {}: {}",
                t(i18n::Key::LoginFailed),
                t(i18n::Key::SyncFailed),
                err
            ))
            .text_size(theme::ui(13.))
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

fn render_syncing(
    dark: bool,
    lines: &[SharedString],
    done: usize,
    total: usize,
    t: &dyn Fn(i18n::Key) -> &'static str,
) -> impl IntoElement {
    let muted = theme::muted_foreground(dark);
    let rows = lines
        .iter()
        .rev()
        .take(8)
        .map(|l| small_label(l.clone(), muted))
        .collect::<Vec<_>>();
    // Progress bar: indeterminate while chunking indexes (no server-side
    // total), deterministic once the content stage reports done/total.
    let bar = if total > 0 {
        Progress::new("sync-progress").value(done as f32 / total as f32 * 100.)
    } else {
        Progress::new("sync-progress").loading(true)
    };
    let caption = if total > 0 {
        SharedString::from(format!(
            "{}/{} · {}",
            done,
            total,
            t(i18n::Key::SyncingContents)
        ))
    } else {
        SharedString::from(t(i18n::Key::SyncingSubtitle))
    };
    gpui_kit::base::v_flex()
        .flex_1()
        .h_full()
        .bg(theme::material(Material::Background, dark))
        .items_center()
        .justify_center()
        .child(
            gpui_kit::base::v_flex()
                .gap_3()
                .w(theme::ui(360.))
                .child(
                    gpui_kit::base::h_flex()
                        .gap_2()
                        .child(Spinner::new())
                        .child(
                            Label::new(t(i18n::Key::SyncingTitle))
                                .text_size(theme::ui(22.))
                                .text_color(theme::foreground(dark)),
                        ),
                )
                .child(bar)
                .child(small_label(caption, muted))
                .child(gpui_kit::base::v_flex().gap_1().children(rows)),
        )
        .into_element()
}

/// Post-sync summary card: counts + optional content-failure warning +
/// "Start now" button that flips Phase::SyncDone → Shell.
fn render_sync_done(
    dark: bool,
    summary: &SyncSummary,
    t: &dyn Fn(i18n::Key) -> &'static str,
    cx: &mut gpui_kit::Context<AppShell>,
) -> impl IntoElement {
    let muted = theme::muted_foreground(dark);
    let row = |k: i18n::Key, n: usize| {
        gpui_kit::base::h_flex()
            .justify_between()
            .child(small_label(SharedString::from(t(k)), muted))
            .child(Label::new(n.to_string()).text_color(theme::foreground(dark)))
    };
    let warning = if summary.content_failures > 0 {
        Some(small_label(
            SharedString::from(format!(
                "{} {}",
                summary.content_failures,
                t(i18n::Key::SyncSummaryFailures)
            )),
            theme::accent(),
        ))
    } else {
        None
    };
    let start = Button::new("sync-start")
        .label(t(i18n::Key::SyncSummaryStart).to_owned())
        .primary()
        .w_full()
        .on_click(cx.listener(|this, _ev, _window, cx| {
            this.phase = Phase::Shell;
            cx.notify();
        }));
    gpui_kit::base::v_flex()
        .flex_1()
        .h_full()
        .bg(theme::material(Material::Background, dark))
        .items_center()
        .justify_center()
        .child(
            gpui_kit::base::v_flex()
                .gap_3()
                .w(theme::ui(360.))
                .child(
                    Label::new(t(i18n::Key::SyncDone))
                        .text_size(theme::ui(22.))
                        .text_color(theme::foreground(dark)),
                )
                .child(row(i18n::Key::SyncSummaryNotebooks, summary.notebooks))
                .child(row(i18n::Key::SyncSummaryNotes, summary.notes))
                .child(row(i18n::Key::SyncSummaryContents, summary.contents))
                .child(row(i18n::Key::SyncSummaryTags, summary.tags))
                .children(warning)
                .child(start),
        )
        .into_element()
}
/// One clickable nav row; `id` doubles as the element id.
fn nav_row(
    shell: &gpui_kit::WeakEntity<AppShell>,
    dark: bool,
    node: &NavNode,
    kind: &str,
    // Issue 23 #3: AssetIcon carries the full Lucide catalog (Trash);
    // the component IconName subset lacks it.
    icon: Option<AssetIcon>,
    selected: bool,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let fg = theme::foreground(dark);
    let sel_fg = theme::accent_foreground();
    // Issue 19: indent via padding so the pill spans the full row width and
    // the count sits flush right.
    // Issue 20: symmetric highlight margins — 8px both sides, plus 12px
    // per nesting level (user ruling Q2).
    let indent = px(8. + 12. * node.depth as f32);
    let shell = shell.clone();
    let sel: NavSelection = match kind {
        "notebook" => NavSelection::Notebook(node.id.clone()),
        "tag" => NavSelection::Tag(node.id.clone()),
        "starred" => NavSelection::Starred,
        "trash" => NavSelection::Trash,
        "recent" => NavSelection::Recent,
        _ => NavSelection::Recent,
    };
    let mut row = gpui_kit::base::h_flex()
        .id(node.id.clone())
        .w_full()
        .gap_2()
        .pl(indent)
        .pr_2()
        .h_8()
        .rounded(px(theme::RADIUS_SELECTION))
        .when(selected, |el| el.bg(theme::accent()))
        .on_click(move |_ev, _window, cx| {
            let _ = shell.update(cx, |this: &mut AppShell, cx| {
                this.select_nav(sel.clone(), cx)
            });
        });
    if let Some(icon) = icon {
        row = row.child(Icon::new(icon).small().text_color(if selected {
            sel_fg
        } else {
            theme::muted_foreground(dark)
        }));
    }
    row.child(
        Label::new(node.title.clone())
            .text_size(theme::ui(14.))
            .text_color(if selected { sel_fg } else { fg }),
    )
    .child(gpui_kit::base::h_flex().flex_1())
    .child(
        small_label(
            node.count.to_string(),
            if selected {
                sel_fg
            } else {
                theme::muted_foreground(dark)
            },
        )
        .into_any_element(),
    )
}

impl AppShell {
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
                let dir = this.data_dir.clone();
                let mut p = crate::prefs::Prefs::load(&dir);
                // Issue 20: accordions are mutually exclusive — opening
                // one closes the other (both fields persist).
                match group {
                    Group::Notebooks => {
                        this.notebooks_open = !this.notebooks_open;
                        if this.notebooks_open {
                            this.tags_open = false;
                        }
                    }
                    Group::Tags => {
                        this.tags_open = !this.tags_open;
                        if this.tags_open {
                            this.notebooks_open = false;
                        }
                    }
                }
                p.nav.notebooks_open = this.notebooks_open;
                p.nav.tags_open = this.tags_open;
                p.save(&dir);
                cx.notify();
            }))
            .child(Icon::new(IconName::ChevronRight).xsmall().text_color(muted))
            .child(
                Label::new(label)
                    .text_size(theme::ui(11.))
                    .text_color(muted),
            )
    }

    /// Left nav: identity area + All/Starred/Trash + Notebooks/Tags groups.
    fn render_nav(
        &mut self,
        dark: bool,
        t: &dyn Fn(i18n::Key) -> &'static str,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> gpui_kit::Div {
        let (notebooks_open, tags_open) = (self.notebooks_open, self.tags_open);
        let shell = cx.weak_entity();
        let lang = self.lang;

        let all = NavNode {
            id: "all".into(),
            title: t(i18n::Key::NavRecent).into(),
            count: 0,
            depth: 0,
        };
        let mut all_count = 0i64;
        if let Some(store) = self.store() {
            // Badge mirrors the Recent LIST (UpdatedTime desc, capped) —
            // not the raw live-note total.
            all_count = store
                .list_notes_meta_recent(RECENT_LIMIT)
                .map(|n| n.len() as i64)
                .unwrap_or(0);
        }
        let all = NavNode {
            count: all_count,
            ..all
        };

        let mut starred_count = 0i64;
        if let Some(store) = self.store() {
            starred_count = store
                .list_notes_filtered(None, true)
                .map(|n| n.len() as i64)
                .unwrap_or(0);
        }
        let mut trash_count = 0i64;
        if let Some(store) = self.store() {
            trash_count = store.list_trash().map(|n| n.len() as i64).unwrap_or(0);
        }
        let starred = NavNode {
            id: "starred".into(),
            title: t(i18n::Key::NavStarred).into(),
            count: starred_count,
            depth: 0,
        };
        let trash = NavNode {
            id: "trash".into(),
            title: t(i18n::Key::NavTrash).into(),
            count: trash_count,
            depth: 0,
        };

        let all_row = nav_row(
            &shell,
            dark,
            &all,
            "fixed",
            Some(AssetIcon::BookOpen),
            self.nav == NavSelection::Recent,
        );
        let starred_row = nav_row(
            &shell,
            dark,
            &starred,
            "starred",
            Some(AssetIcon::Star),
            self.nav == NavSelection::Starred,
        );
        let trash_row = nav_row(
            &shell,
            dark,
            &trash,
            "trash",
            Some(AssetIcon::Trash), // issue 23 #3: trash-can glyph
            self.nav == NavSelection::Trash,
        );

        // Glass lives ONLY here (user round 3): nav paints Material::Base
        // over the NSVisualEffectView. Root/list/reader paint opaque
        // materials so no blur shows through them. Round 5: the Base alpha
        // follows the user's transparency pref (off/weak/medium/strong).
        let glass = crate::prefs::Prefs::load(&self.data_dir).glass;
        let mut nav = gpui_kit::base::v_flex()
            .w_full()
            .h_full()
            .bg(theme::glass_base(dark, &glass))
            // Tests read this element's bounds to prove the pane layout
            // survives ⌘T full preview (no-op in release).
            .debug_selector(|| "nav-pane".into())
            .px_3()
            .gap_1()
            .child(drag_header(window, lang, cx))
            // Issue 17 Q7: identity moved to the bottom trio (see the nav
            // footer below) — no duplicate under the header.
            .child(all_row)
            .child(starred_row);

        // Ticket 11: right-click menu on the Notebooks group header —
        // "New Notebook" at root level.
        nav = nav.child(
            self.group_header(
                dark,
                t(i18n::Key::NavNotebooks),
                notebooks_open,
                Group::Notebooks,
                cx,
            )
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
            let notebook_nodes = self.notebook_nav_nodes().clone();
            // New/Rename go through the modal dialog now — no inline row.
            let n = notebook_nodes.len();
            let nav_sel = self.nav.clone();
            let shell_ul = shell.clone();
            let scroll = self.notebooks_scroll.clone();
            nav = nav.child(
                gpui_kit::div()
                    .id("notebooks-vlist")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .on_drag_move(cx.listener(
                        |this, ev: &gpui_kit::DragMoveEvent<NotebookDrag>, _window, cx| {
                            // on_drag_move fires for the WHOLE window during
                            // the drag (div.rs:354-356, 366-383 — capture
                            // phase, no hitbox-contains gate), so gate on the
                            // pointer actually being inside the list here.
                            // Outside (tags/trash/footer rows): no target —
                            // the tree must not accept drops from beyond its
                            // bounds, and the stale indicator must clear.
                            let b = &ev.bounds;
                            let inside = ev.event.position.y >= b.origin.y
                                && ev.event.position.y < b.origin.y + b.size.height
                                && ev.event.position.x >= b.origin.x
                                && ev.event.position.x < b.origin.x + b.size.width;
                            if !inside {
                                if this.notebook_drag_target.take().is_some() {
                                    cx.notify();
                                }
                                return;
                            }
                            // Pointer y in list CONTENT space: viewport y
                            // plus scroll_top (the helper returns
                            // scroll_top, positive when scrolled down).
                            // The vlist is taller than the rows when the
                            // tree is short — clamp to the last row's
                            // bottom edge so trailing empty space means
                            // "after the last notebook" (zTree's empty-area
                            // tolerance, -.2/1.2 percent, ztree.all-3.5.js
                            // 2902/2906).
                            // Rows are `h_8()` = rems(2.0), so their pixel
                            // height tracks the window's rem size, not a
                            // constant. The uniform list records the
                            // measured height in its scroll handle
                            // (contents = item_h * count, uniform_list.rs
                            // 359-377) — use that, else ix = y/32 drifts
                            // by a fraction of a row per row and lands
                            // rows early near the bottom.
                            let nodes_n = this.notebook_nav_nodes().len();
                            let n = nodes_n as f32;
                            let row_h = notebooks_row_height(this, nodes_n);
                            let rows_h = n * row_h;
                            let y: f32 = (ev.event.position.y - b.origin.y).into();
                            let scrolled = notebooks_scroll_offset(this);
                            let content_y = (y + scrolled).clamp(0., rows_h - 0.01);
                            this.notebook_drag_target =
                                this.notebook_drop_at(&ev.drag(cx).id, content_y, row_h);
                            cx.notify();
                        },
                    ))
                    .on_drop::<NotebookDrag>(cx.listener(
                        |this, drag: &NotebookDrag, _window, cx| {
                            if let Some(target) = this.notebook_drag_target.clone() {
                                this.drop_notebook(drag, target, cx);
                            }
                        },
                    ))
                    .child(
                        gpui_kit::uniform_list("notebooks-ul", n, move |range, _window, _cx| {
                            range
                                .map(|ix| {
                                    let Some(node) = notebook_nodes.get(ix) else {
                                        return gpui_kit::div().into_any_element();
                                    };
                                    let is_sel = nav_sel == NavSelection::Notebook(node.id.clone());
                                    nav_row(
                                        &shell_ul,
                                        dark,
                                        node,
                                        "notebook",
                                        Some(AssetIcon::FolderClosed),
                                        is_sel,
                                    )
                                    .on_drag(
                                        NotebookDrag {
                                            id: node.id.clone(),
                                            title: node.title.clone(),
                                        },
                                        |drag, _pos, _window, cx| {
                                            cx.new(|_| NotebookDragGhost(drag.title.clone().into()))
                                        },
                                    )
                                    .context_menu({
                                        let shell = shell_ul.clone();
                                        let id2 = node.id.clone();
                                        move |menu, _window, _cx| {
                                            let shell_new = shell.clone();
                                            let shell_ren = shell.clone();
                                            let shell_mv = shell.clone();
                                            let shell_del = shell.clone();
                                            let id_new = id2.clone();
                                            let id_ren = leanote_core::Id(id2.clone());
                                            let id_mv = leanote_core::Id(id2.clone());
                                            let id_del = leanote_core::Id(id2.clone());
                                            menu.item(
                                                PopupMenuItem::new(i18n::t(
                                                    lang,
                                                    i18n::Key::NewNotebook,
                                                ))
                                                .on_click(move |_ev, window, cx| {
                                                    let _ = shell_new.update(cx, |this, cx| {
                                                        this.begin_new_notebook(
                                                            id_new.clone(),
                                                            window,
                                                            cx,
                                                        )
                                                    });
                                                }),
                                            )
                                            .item(
                                                PopupMenuItem::new(i18n::t(
                                                    lang,
                                                    i18n::Key::RenameNotebook,
                                                ))
                                                .on_click(move |_ev, window, cx| {
                                                    let _ = shell_ren.update(cx, |this, cx| {
                                                        this.begin_rename_notebook(
                                                            id_ren.clone(),
                                                            window,
                                                            cx,
                                                        )
                                                    });
                                                }),
                                            )
                                            .item(
                                                PopupMenuItem::new(i18n::t(
                                                    lang,
                                                    i18n::Key::MoveNotebookTo,
                                                ))
                                                .on_click(move |_ev, _window, cx| {
                                                    let _ = shell_mv.update(cx, |this, cx| {
                                                        this.begin_move_notebook(id_mv.clone(), cx)
                                                    });
                                                }),
                                            )
                                            .item(
                                                PopupMenuItem::new(i18n::t(
                                                    lang,
                                                    i18n::Key::DeleteNotebook,
                                                ))
                                                .on_click(move |_ev, window, cx| {
                                                    let _ = shell_del.update(cx, |this, cx| {
                                                        this.delete_notebook_nav(
                                                            id_del.clone(),
                                                            window,
                                                            cx,
                                                        )
                                                    });
                                                }),
                                            )
                                        }
                                    })
                                    .into_any_element()
                                })
                                .collect()
                        })
                        .h_full()
                        .w_full()
                        .track_scroll(&scroll),
                    )
                    .when_some(self.notebook_drag_target.as_ref(), |this, target| {
                        // Overlay is a sibling of the scrolling list, so
                        // it lives in VIEWPORT space: content y minus
                        // scroll_top. The `inner` band highlights the
                        // hovered row instead of drawing the line.
                        let y = target.indicator_y - notebooks_scroll_offset(self);
                        if target.inner {
                            this.child(
                                gpui_kit::div()
                                    .absolute()
                                    .top(px(y))
                                    .left_0()
                                    .right_0()
                                    .h(px(notebooks_row_height(
                                        self,
                                        self.cached_notebook_nodes.as_ref().map_or(0, Vec::len),
                                    )))
                                    .bg(theme::accent().opacity(0.15)),
                            )
                        } else {
                            // 2px line CENTERED on the boundary pixel so it
                            // visually spans "between" the two rows.
                            this.child(
                                gpui_kit::div()
                                    .absolute()
                                    .top(px(y - 1.))
                                    .left_0()
                                    .right_0()
                                    .h(theme::ui(2.))
                                    .bg(theme::accent()),
                            )
                        }
                    })
                    .child(
                        gpui_kit::div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w(theme::ui(12.))
                            .child(
                                Scrollbar::vertical(&scroll)
                                    .viewport_from_layout()
                                    .max_fps(60),
                            ),
                    ),
            );
        }

        nav = nav.child(self.group_header(dark, t(i18n::Key::NavTags), tags_open, Group::Tags, cx));

        if tags_open {
            let tag_nodes = self.tag_nav_nodes().clone();
            let edit_row = self.nav_edit_row(dark, t, "tag", window, cx);
            let edit_row = std::rc::Rc::new(std::cell::RefCell::new(edit_row));
            let nav_sel = self.nav.clone();
            let shell_ul = shell.clone();
            let n = tag_nodes.len() + usize::from(edit_row.borrow().is_some());
            let scroll = self.tags_scroll.clone();
            nav = nav.child(
                gpui_kit::div()
                    .id("tags-vlist")
                    .relative()
                    // Issue 20: the open group owns the leftover height
                    // (exclusive accordion → only one of the two renders).
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        gpui_kit::uniform_list("tags-ul", n, move |range, _window, _cx| {
                            range
                                .map(|ix| {
                                    let Some(node) = tag_nodes.get(ix) else {
                                        return edit_row
                                            .borrow_mut()
                                            .take()
                                            .unwrap_or_else(|| gpui_kit::div().into_any_element());
                                    };
                                    let is_sel = nav_sel == NavSelection::Tag(node.id.clone());
                                    nav_row(&shell_ul, dark, node, "tag", None, is_sel)
                                        .context_menu({
                                            let shell = shell_ul.clone();
                                            let tag = node.id.clone();
                                            move |menu, _window, _cx| {
                                                let shell_ren = shell.clone();
                                                let shell_del = shell.clone();
                                                let tag_ren = tag.clone();
                                                let tag_del = tag.clone();
                                                menu.item(
                                                    PopupMenuItem::new(i18n::t(
                                                        lang,
                                                        i18n::Key::RenameTag,
                                                    ))
                                                    .on_click(move |_ev, window, cx| {
                                                        let _ = shell_ren.update(cx, |this, cx| {
                                                            this.begin_rename_tag(
                                                                tag_ren.clone(),
                                                                window,
                                                                cx,
                                                            )
                                                        });
                                                    }),
                                                )
                                                .item(
                                                    PopupMenuItem::new(i18n::t(
                                                        lang,
                                                        i18n::Key::DeleteTag,
                                                    ))
                                                    .on_click(move |_ev, _window, cx| {
                                                        let _ = shell_del.update(cx, |this, cx| {
                                                            this.delete_tag_nav(tag_del.clone(), cx)
                                                        });
                                                    }),
                                                )
                                            }
                                        })
                                        .into_any_element()
                                })
                                .collect()
                        })
                        .h_full()
                        .w_full()
                        .track_scroll(&scroll),
                    )
                    .child(
                        gpui_kit::div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .w(theme::ui(12.))
                            .child(
                                Scrollbar::vertical(&scroll)
                                    .viewport_from_layout()
                                    .max_fps(60),
                            ),
                    ),
            );
        }

        // Issue 19: trash lives after the tags group, before the footer.
        // Issue 24: spacer only when BOTH groups are collapsed — the open
        // group's vlist owns the leftover height (flex_1), and a permanent
        // spacer split it half/half, pinning trash+footer mid-pane.
        if !notebooks_open && !tags_open {
            nav = nav.child(gpui_kit::div().id("nav-spacer").flex_1().min_h_0());
        }
        nav = nav.child(trash_row);

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
                        .child(
                            Label::new(format!("{}{}", "　".repeat(pdepth), ptitle))
                                .text_size(theme::ui(14.)),
                        ),
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

        // Issue 17 Q7: bottom trio — sync status, settings, account.
        let acct = self.store().and_then(|s| s.active_account().ok().flatten());
        let mut footer = gpui_kit::base::h_flex().gap_1().pt_1().pb_2();
        if let Some(acct) = acct {
            // Sync: icon carries the last outcome; click = sync now.
            // Four states, Err > Pending > Ok > None (label shares the order).
            let icon = match (&self.sync_status, self.pending_sync) {
                (Some(Err(_)), _) => AssetIcon::TriangleAlert,
                (_, true) => AssetIcon::RefreshCw,
                (Some(Ok(_)), _) => AssetIcon::CircleCheck,
                (None, _) => AssetIcon::Cloud,
            };
            let title = self.sync_status_label();
            footer = footer.child(
                Button::new("nav-sync")
                    .icon(icon)
                    .ghost()
                    .tooltip(title)
                    .on_click(cx.listener(|this, _ev, _window, cx| this.sync_now(cx))),
            );
            // Account: username + menu (logout → confirm).
            let name: SharedString = if acct.username.is_empty() {
                acct.email.clone().into()
            } else {
                acct.username.clone().into()
            };
            let lang = self.lang;
            let menu_name = name.clone();
            footer = footer.child(
                gpui_kit::base::h_flex()
                    .flex_1()
                    .justify_end()
                    .gap_1()
                    .when(acct.host.starts_with("http"), |el| {
                        el.child(
                            Button::new("nav-user")
                                .icon(AssetIcon::CircleUser)
                                .label(menu_name)
                                .ghost()
                                .context_menu({
                                    let shell = shell.clone();
                                    move |menu, _window, _cx| {
                                        let shell = shell.clone();
                                        menu.item(
                                            PopupMenuItem::new(i18n::t(lang, i18n::Key::Logout))
                                                .on_click({
                                                    let shell = shell.clone();
                                                    move |_ev, window, cx| {
                                                        let _ = shell.update(cx, |this, cx| {
                                                            this.logout_confirm_dialog(window, cx)
                                                        });
                                                    }
                                                }),
                                        )
                                    }
                                }),
                        )
                    }),
            );
        }
        // Settings: centered dialog (issue 17 Q7).
        footer = footer.child(
            Button::new("nav-settings")
                .icon(AssetIcon::Cog)
                .ghost()
                .tooltip(i18n::t(lang, i18n::Key::TooltipSettings))
                .on_click(cx.listener(|this, _ev, window, cx| this.open_settings(window, cx))),
        );
        nav = nav.child(footer);

        nav
    }

    /// Issue 17 Q7: three-button segmented control (label trio + active index).
    /// `on_pick` receives the picked index; the dialog rebuild re-reads prefs,
    /// so the highlight stays in sync.
    fn segmented<const N: usize>(
        id: &'static str,
        labels: [SharedString; N],
        active: usize,
        on_pick: impl Fn(usize, &mut Window, &mut gpui_kit::App) + 'static,
    ) -> AnyElement {
        let on_pick = std::rc::Rc::new(on_pick);
        let mut row = gpui_kit::base::h_flex().gap_1();
        for (ix, label) in labels.into_iter().enumerate() {
            let on_pick = on_pick.clone();
            row = row.child(
                Button::new(SharedString::from(format!("{id}-{ix}")))
                    .label(label)
                    .when(ix == active, |b| b.primary())
                    .when(ix != active, |b| b.ghost())
                    .xsmall()
                    .on_click(move |_, window, cx| on_pick(ix, window, cx)),
            );
        }
        row.into_any_element()
    }

    /// Issue 17 Q7 / UI round 10: settings dialog — left section nav
    /// (外观/编辑器/数据) + right rows. The build closure reruns every
    /// frame (root.rs re-invokes the stored builder), so `shell.read(cx)`
    /// per call keeps language live without reopening the dialog.
    fn open_settings(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        let shell = cx.entity();
        // ADR-0010: family picker entity must exist BEFORE the dialog
        // closure borrows cx — entity creation can't happen inside it.
        let zh = self.lang == Lang::Zh;
        let items: Vec<SharedString> = theme::FAMILIES
            .iter()
            .map(|f| SharedString::from(if zh { f.name_zh } else { f.name_en }))
            .collect();
        let sel = theme::family_index(&crate::prefs::Prefs::load(&self.data_dir).ui_theme);
        let family_state = cx.new(|cx| {
            ComboboxState::new(
                SearchableVec::new(items),
                vec![IndexPath::default().row(sel)],
                window,
                cx,
            )
            .searchable(false)
        });
        cx.subscribe(&family_state, {
            let zh = zh;
            move |this: &mut AppShell,
                  _: gpui_kit::Entity<ComboboxState<SearchableVec<SharedString>>>,
                  ev: &ComboboxEvent<SearchableVec<SharedString>>,
                  cx| {
                if let ComboboxEvent::Confirm(vals) = ev {
                    let hit = vals.first().and_then(|v| {
                        theme::FAMILIES.iter().position(|f| {
                            SharedString::from(if zh { f.name_zh } else { f.name_en }) == *v
                        })
                    });
                    if let Some(ix) = hit {
                        let id = theme::FAMILIES[ix].id;
                        this.set_ui_theme(id, cx);
                    }
                }
            }
        })
        .detach();
        // Which section is selected, stable across the closure's frames.
        let section: std::rc::Rc<std::cell::Cell<usize>> = std::rc::Rc::default();
        window.open_dialog(cx, move |dialog, _window, cx| {
            let shell_c = shell.clone();
            let t = |k: i18n::Key| i18n::t(shell.read(cx).lang, k);
            let prefs = crate::prefs::Prefs::load(&shell.read(cx).data_dir);
            let acct = shell
                .read(cx)
                .store()
                .and_then(|s| s.active_account().ok().flatten());
            let dark = shell.read(cx).dark(cx);
            let muted = theme::muted_foreground(dark);

            // Row builder.
            let row = |label: gpui_kit::SharedString, control: gpui_kit::AnyElement| {
                gpui_kit::base::h_flex()
                    .justify_between()
                    .gap_3()
                    .py_1()
                    .child(Label::new(label).text_size(theme::ui(13.)))
                    .child(control)
            };

            // Right column for the active section.
            let mut right = gpui_kit::base::v_flex().gap_2().px_1().py_1().w_full();
            match section.get() {
                // ── 外观 / Appearance ──────────────────────────────
                0 => {
                    right = right
                        .child(row(
                            t(i18n::Key::Appearance).into(),
                            Self::segmented(
                                "set-appearance",
                                [
                                    t(i18n::Key::AppearanceFollow).into(),
                                    t(i18n::Key::AppearanceLight).into(),
                                    t(i18n::Key::AppearanceDark).into(),
                                ],
                                match prefs.appearance {
                                    crate::prefs::AppearancePref::Follow => 0,
                                    crate::prefs::AppearancePref::Light => 1,
                                    crate::prefs::AppearancePref::Dark => 2,
                                },
                                {
                                    let shell = shell_c.clone();
                                    move |ix, _window, cx| {
                                        shell.update(cx, |this, cx| {
                                            this.set_appearance(ix, cx);
                                        });
                                    }
                                },
                            ),
                        ))
                        // ADR-0010: theme family — UI-wide, so it lives in
                        // Appearance, not Editor. One pref retints everything.
                        .child(row(
                            t(i18n::Key::UiTheme).into(),
                            // kit's Combobox renders `.size_full()` on its
                            // root; a fixed-width wrapper keeps the trigger
                            // right-aligned like every other row control
                            // instead of stretching across the row.
                            gpui_kit::div()
                                .w(theme::ui(94.))
                                .child(
                                    Combobox::new(&family_state)
                                        .with_size(gpui_kit::component::Size::Small)
                                        .menu_width(gpui_kit::px(150.)),
                                )
                                .into_any_element(),
                        ))
                        .child(row(t(i18n::Key::AccentColor).into(), {
                            let mut swatches = gpui_kit::base::h_flex().gap_1();
                            for (id, hex) in theme::ACCENTS {
                                let shell = shell_c.clone();
                                let id = *id;
                                let selected = prefs.accent == id;
                                let mut sw = gpui_kit::div()
                                    .id(SharedString::from(format!("accent-{id}")))
                                    .size(px(18.))
                                    .rounded_full()
                                    .bg(gpui_kit::rgb(*hex))
                                    .when(selected, |el| {
                                        el.border_1().border_color(theme::foreground(dark))
                                    });
                                sw = sw.on_click(move |_, _window, cx| {
                                    let _ = shell.update(cx, |this, cx| {
                                        this.set_accent(id, cx);
                                    });
                                });
                                swatches = swatches.child(sw);
                            }
                            swatches.into_any_element()
                        }))
                        // Round 5: nav transparency strength, 4 flat steps.
                        .child(row(
                            t(i18n::Key::GlassEffect).into(),
                            Self::segmented(
                                "set-glass",
                                [
                                    t(i18n::Key::GlassOff).into(),
                                    t(i18n::Key::GlassWeak).into(),
                                    t(i18n::Key::GlassMedium).into(),
                                    t(i18n::Key::GlassStrong).into(),
                                ],
                                match prefs.glass.as_str() {
                                    "off" => 0,
                                    "weak" => 1,
                                    "strong" => 3,
                                    _ => 2,
                                },
                                {
                                    let shell = shell_c.clone();
                                    move |ix, _window, cx| {
                                        let id = ["off", "weak", "medium", "strong"][ix];
                                        shell.update(cx, |this, cx| this.set_glass(id, cx));
                                    }
                                },
                            ),
                        ))
                        // Interface zoom (issue 23 #5 / UI round 10: 3 steps).
                        .child(row(t(i18n::Key::InterfaceZoom).into(), {
                            let mut zooms = gpui_kit::base::h_flex().gap_1();
                            for factor in [1.0f32, 1.1, 1.2] {
                                let shell = shell_c.clone();
                                let label = format!("{}%", (factor * 100.).round() as u32);
                                let active = (prefs.zoom - factor).abs() < 0.01;
                                zooms = zooms.child(
                                    Button::new(SharedString::from(format!("set-zoom-{factor}")))
                                        .label(label)
                                        .when(active, |b| b.primary())
                                        .when(!active, |b| b.ghost())
                                        .xsmall()
                                        .on_click(move |_, window, cx| {
                                            let _ = shell.update(cx, |this, cx| {
                                                this.set_zoom(factor, window, cx)
                                            });
                                        }),
                                );
                            }
                            zooms.into_any_element()
                        }))
                        // UI font (UI round 10 #6): default / LXGW WenKai.
                        .child(row(
                            t(i18n::Key::UIFont).into(),
                            Self::segmented(
                                "set-font",
                                [
                                    t(i18n::Key::FontDefault).into(),
                                    t(i18n::Key::FontWenKai).into(),
                                ],
                                if prefs.font == "wenkai-mono" { 1 } else { 0 },
                                {
                                    let shell = shell_c.clone();
                                    move |ix, _window, cx| {
                                        shell.update(cx, |this, cx| {
                                            this.set_font_setting(
                                                if ix == 1 { "wenkai-mono" } else { "" },
                                                cx,
                                            );
                                        });
                                    }
                                },
                            ),
                        ))
                        // Font hint shows only while it still matters:
                        // downloading (state 0, file absent), just installed
                        // this run (state 1 + file written this session), or
                        // failed (state 2). Silent when WenKai isn't in use
                        // or was installed by an earlier run.
                        .when(
                            prefs.font == "wenkai-mono"
                                && crate::fonts::INSTALL_STATE
                                    .load(std::sync::atomic::Ordering::Relaxed)
                                    != 1
                                || crate::fonts::INSTALL_STATE
                                    .load(std::sync::atomic::Ordering::Relaxed)
                                    == 2,
                            |el| {
                                el.child(small_label(
                                    match crate::fonts::INSTALL_STATE
                                        .load(std::sync::atomic::Ordering::Relaxed)
                                    {
                                        2 => t(i18n::Key::FontInstallFailed),
                                        1 => t(i18n::Key::FontRestartHint),
                                        _ => t(i18n::Key::FontDownloading),
                                    },
                                    muted,
                                ))
                            },
                        )
                        // Language
                        .child(row(
                            t(i18n::Key::LanguageSetting).into(),
                            Self::segmented(
                                "set-lang",
                                [
                                    t(i18n::Key::AppearanceFollow).into(),
                                    "中文".into(),
                                    "English".into(),
                                ],
                                match prefs.lang {
                                    crate::prefs::LangPref::Follow => 0,
                                    crate::prefs::LangPref::Zh => 1,
                                    crate::prefs::LangPref::En => 2,
                                },
                                {
                                    let shell = shell_c.clone();
                                    move |ix, _window, cx| {
                                        shell.update(cx, |this, cx| {
                                            this.set_lang(ix, cx);
                                        });
                                    }
                                },
                            ),
                        ))
                }
                // ── 编辑器 / Editor ────────────────────────────────
                1 => {
                    right = right
                        // Vim toggle (issue 23 #9)
                        .child(row(
                            t(i18n::Key::VimMode).into(),
                            Switch::new("set-vim")
                                .checked(prefs.vim_default)
                                .on_click({
                                    let shell = shell_c.clone();
                                    move |checked, _window, cx| {
                                        let _ = shell.update(cx, |this, cx| {
                                            this.set_vim_setting(*checked, cx)
                                        });
                                    }
                                })
                                .into_any_element(),
                        ))
                        // Editor font size (UI round 12): regular/medium/large;
                        // the appearance zoom scales this on top.
                        .child(row(
                            t(i18n::Key::EditorFontSize).into(),
                            Self::segmented(
                                "set-editor-size",
                                [
                                    t(i18n::Key::FontSizeRegular).into(),
                                    t(i18n::Key::FontSizeMedium).into(),
                                    t(i18n::Key::FontSizeLarge).into(),
                                ],
                                match prefs.editor_font_size.as_str() {
                                    "" => 0,
                                    "medium" => 1,
                                    _ => 2,
                                },
                                {
                                    let shell = shell_c.clone();
                                    move |ix, _window, cx| {
                                        shell.update(cx, |this, cx| {
                                            this.set_editor_font_size(ix, cx);
                                        });
                                    }
                                },
                            ),
                        ))
                }
                // ── 数据 / Data ────────────────────────────────────
                _ => {
                    right = right
                        .child(small_label(
                            format!(
                                "{}: {}",
                                t(i18n::Key::AccountInfo),
                                acct.as_ref().map(|a| a.email.as_str()).unwrap_or("-")
                            ),
                            muted,
                        ))
                        .child(small_label(
                            format!(
                                "{}: {}",
                                t(i18n::Key::ServerLabel),
                                acct.as_ref().map(|a| a.host.as_str()).unwrap_or("-")
                            ),
                            muted,
                        ))
                        .child(
                            Button::new("set-export")
                                .label(t(i18n::Key::ExportBackupButton).to_owned())
                                .outline()
                                .on_click({
                                    let shell = shell_c.clone();
                                    move |_ev, _window, cx| {
                                        let _ = shell.update(cx, |this, cx| this.export_backup(cx));
                                    }
                                }),
                        )
                        .when_some(acct.clone(), |el, _acct| {
                            el.child(
                                Button::new("settings-logout")
                                    .label(t(i18n::Key::Logout).to_owned())
                                    .outline()
                                    .on_click({
                                        let shell = shell_c.clone();
                                        move |_ev, window, cx| {
                                            let _ = shell.update(cx, |this, cx| {
                                                this.logout_confirm_dialog(window, cx)
                                            });
                                        }
                                    }),
                            )
                        });
                }
            }

            // Left nav: three section buttons.
            let mut nav = gpui_kit::base::v_flex().gap_1().w(theme::ui(110.));
            for (ix, label) in [
                i18n::Key::SectionAppearance,
                i18n::Key::SectionEditor,
                i18n::Key::SectionData,
            ]
            .into_iter()
            .enumerate()
            {
                let section = section.clone();
                let active = section.get() == ix;
                nav = nav.child(
                    Button::new(SharedString::from(format!("set-section-{ix}")))
                        .label(t(label).to_owned())
                        .when(active, |b| b.primary())
                        .when(!active, |b| b.ghost())
                        .xsmall()
                        .w_full()
                        .on_click(move |_, _window, _cx| {
                            section.set(ix);
                        }),
                );
            }

            let content = gpui_kit::base::h_flex()
                .gap_4()
                .items_start()
                .child(nav)
                .child(right);

            // Round 3: kit paints the dialog panel from tokens.background,
            // which apply_wake_theme forces transparent for the window
            // glass. Re-opaque it here or settings floats on the blur.
            dialog
                .w(px(640.))
                .title(t(i18n::Key::SettingsTitle))
                .bg(theme::material(Material::Popover, dark))
                .child(content)
        });
    }

    /// Issue 19: history as a centered dialog (was an inline block under the
    /// editor). Clicking a row previews; Restore writes it back.
    fn open_history(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        let shell = cx.entity();
        let lang = self.lang;
        let note_id = self.editing.as_ref().map(|e| e.note_id.clone());
        let history = self
            .store()
            .and_then(|s| note_id.map(|id| s.list_history(&id).ok()))
            .flatten()
            .unwrap_or_default();
        let t = move |k: i18n::Key| i18n::t(lang, k);
        window.open_dialog(cx, move |dialog, _window, cx| {
            let shell_c = shell.clone();
            let dark = shell.read(cx).dark(cx);
            let muted = theme::muted_foreground(dark);
            let mut rows = gpui_kit::base::v_flex().gap_1();
            if history.is_empty() {
                rows = rows.child(small_label(t(i18n::Key::HistoryEmpty), muted));
            }
            for h in &history {
                rows = rows.child(
                    Button::new(format!("history-row-{}", h.history_id.to_string()))
                        .label(format!("{} · {}", h.title, fmt_time(h.created_time)))
                        .ghost()
                        .on_click({
                            let shell = shell_c.clone();
                            let content = h.content.clone();
                            let title = h.title.clone();
                            let hid = h.history_id.clone();
                            move |_ev, window, cx| {
                                window.close_dialog(cx);
                                shell.update(cx, |this, cx| {
                                    this.preview_history(
                                        hid.clone(),
                                        title.clone(),
                                        content.clone(),
                                        window,
                                        cx,
                                    )
                                });
                            }
                        }),
                );
            }
            dialog.w(px(440.)).title(t(i18n::Key::History)).child(rows)
        });
    }

    /// Issue 19: history preview + restore, second dialog step.
    fn preview_history(
        &mut self,
        history_id: leanote_core::Id,
        title: String,
        content: String,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let lang = self.lang;
        let dark = self.dark(cx);
        let t = move |k: i18n::Key| i18n::t(lang, k);
        let shell = cx.entity();
        let prefs = crate::prefs::Prefs::load(&self.data_dir);
        let _preview_px = px(theme::preview_font_px(&prefs.editor_font_size, prefs.zoom));
        window.open_dialog(cx, move |dialog, _window, _cx| {
            let shell_c = shell.clone();
            let src = browse::reader_markdown_src(&title, &content, true);
            dialog
                .w(px(560.))
                .title(t(i18n::Key::History))
                .child(
                    crate::mermaid::install(TextView::markdown("history-preview", src), dark)
                        .scrollable(true)
                        .on_link_click(|url, _ev, _window, cx| cx.open_url(url)),
                )
                .child(
                    Button::new("history-restore")
                        .label(t(i18n::Key::RestoreVersion))
                        .outline()
                        .on_click({
                            let shell = shell_c.clone();
                            let hid = history_id.clone();
                            move |_ev, window, cx| {
                                window.close_dialog(cx);
                                shell.update(cx, |this, cx| {
                                    this.restore_history_version(hid.clone(), window, cx)
                                });
                            }
                        }),
                )
        });
    }

    /// Issue 19: Markdown cheat-sheet dialog.
    pub(crate) fn open_md_help(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        let dark = self.dark(cx);
        let lang = self.lang;
        let t = move |k: i18n::Key| i18n::t(lang, k);
        let prefs = crate::prefs::Prefs::load(&self.data_dir);
        let preview_px = px(theme::preview_font_px(&prefs.editor_font_size, prefs.zoom));
        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog
                .w(px(440.))
                .title(t(i18n::Key::MarkdownHelpTitle))
                .child(div().text_size(preview_px).child(crate::mermaid::install(
                    TextView::markdown(
                        "md-help",
                        browse::reader_markdown_src("", i18n::markdown_help_body(lang), true),
                    ),
                    dark,
                )))
        });
    }

    /// Issue 19: attachments dialog — scan the note content for
    /// `getAttach?fileId=` refs (upload is issue 20); Download fetches via
    /// `Api::get_attach` and writes via the OS save dialog.
    fn open_attachments(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        let shell = cx.entity();
        let lang = self.lang;
        let content = self
            .editing
            .as_ref()
            .map(|ed| ed.content.read(cx).value().to_string())
            .unwrap_or_default();
        let mut ids: Vec<String> = Vec::new();
        let rest = content.as_str();
        while let Some(pos) = rest.find("getAttach?fileId=") {
            let after = &rest[pos + "getAttach?fileId=".len()..];
            let id: String = after
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            if !id.is_empty() && !ids.contains(&id) {
                ids.push(id);
            }
        }
        let t = move |k: i18n::Key| i18n::t(lang, k);
        window.open_dialog(cx, move |dialog, _window, cx| {
            let dark = shell.read(cx).dark(cx);
            let muted = theme::muted_foreground(dark);
            let mut list = gpui_kit::base::v_flex().gap_1();
            if ids.is_empty() {
                list = list.child(small_label(t(i18n::Key::AttachmentsEmpty), muted));
            }
            for id in &ids {
                let shell = shell.clone();
                let id = id.clone();
                list = list.child(
                    gpui_kit::base::h_flex()
                        .justify_between()
                        .gap_2()
                        .child(
                            Label::new(format!("{id}.bin"))
                                .text_size(theme::ui(13.))
                                .font_family("Menlo"),
                        )
                        .child(
                            Button::new(format!("attach-dl-{id}"))
                                .label(t(i18n::Key::Download))
                                .outline()
                                .small()
                                .on_click(move |_ev, window, cx| {
                                    shell.update(cx, |this, cx| {
                                        this.download_attachment(id.clone(), window, cx)
                                    });
                                }),
                        ),
                );
            }
            dialog
                .w(px(440.))
                .title(t(i18n::Key::Attachments))
                .child(list)
        });
    }

    /// Issue 19: fetch one attachment and write it through the OS save
    /// dialog (same path shape as `export_selected_file`).
    fn download_attachment(
        &mut self,
        file_id: String,
        _window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let Some(store) = self.store() else { return };
        let Ok(Some(acct)) = store.active_account() else {
            return;
        };
        let api = Api::for_account(&acct);
        let failed = i18n::t(self.lang, i18n::Key::DownloadFailed).to_string();
        self.export_with_panel(format!("{file_id}.bin"), cx, move |path| {
            let (_, bytes) = api.get_attach(&file_id).map_err(|_| failed.clone())?;
            std::fs::write(&path, bytes).map_err(|e| e.to_string())
        });
    }

    /// Issue 19: wrap the selection (or insert at cursor) with `prefix` /
    /// `suffix` markdown markup. Uses the public selection API: select the
    /// target range, then `replace()` swaps it (undo-able, cursor at end).
    pub(crate) fn insert_around(
        &mut self,
        prefix: &str,
        suffix: &str,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let Some(ed) = &self.editing else { return };
        if ed.preview {
            return;
        }
        let editor = ed.content.clone();
        let prefix = prefix.to_string();
        let suffix = suffix.to_string();
        editor.update(cx, |state, cx| {
            let range = state.selected_range();
            let (start, end) = (range.start.min(range.end), range.start.max(range.end));
            let selected = state.value().as_ref()[start..end].to_string();
            state.set_selected_range(start..end, cx);
            state.replace(format!("{prefix}{selected}{suffix}"), window, cx);
            // Leave the original text selected so repeated clicks toggle.
            let s = start + prefix.len();
            state.set_selected_range(s..(s + selected.len()), cx);
        });
        self.schedule_autosave(cx);
    }

    /// 斜杠面板「图片」项：打开文件选择器并附加到笔记
    /// （原 md-toolbar 图标按钮逻辑，面板取代工具栏后迁到这里）。
    pub(crate) fn pick_images(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        let rx = cx.prompt_for_paths(gpui_kit::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                _ = this.update_in(cx, |this, window, cx| {
                    this.attach_images(paths, window, cx);
                });
            }
        })
        .detach();
    }
    /// Issue 21 需求9: store each image locally (24-hex id) and insert
    /// `![name](leanote://file/getImage?fileId=<id>)` at the cursor.
    fn attach_images(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let Some(ed) = self.editing.as_ref() else {
            return;
        };
        if ed.preview {
            return;
        }
        let editor = ed.content.clone();
        let mut md = String::new();
        for p in paths {
            let Ok(bytes) = std::fs::read(&p) else {
                continue;
            };
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "image".into());
            let mime = match name
                .rsplit('.')
                .next()
                .map(|e| e.to_ascii_lowercase())
                .as_deref()
            {
                Some("jpg" | "jpeg") => "image/jpeg".to_string(),
                Some(ext @ ("png" | "gif" | "webp" | "bmp" | "svg")) => {
                    format!("image/{ext}")
                }
                _ => "image/png".to_string(),
            };
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let id = format!("{nanos:024x}");
            let img = leanote_core::ImageAttachment {
                local_id: leanote_core::Id(id.clone()),
                server_id: String::new(),
                file_name: name.clone(),
                mime: mime.clone(),
                size: bytes.len() as i64,
                bytes,
                created_time: 0,
            };
            let stored = self.store().map(|s| s.insert_image(&img));
            if !matches!(stored, Some(Ok(()))) {
                continue;
            }
            md.push_str(&format!("![{name}](leanote://file/getImage?fileId={id})\n"));
        }
        if md.is_empty() {
            return;
        }
        editor.update(cx, |state, cx| {
            let cursor = state.selected_range().end;
            state.set_selected_range(cursor..cursor, cx);
            state.insert(md, window, cx);
        });
        self.schedule_autosave(cx);
    }

    /// Issue 21: editor content has focus and vim is off or in insert
    /// mode — the "type into the buffer" state where input assists run.
    fn editor_insert_focused(&self, window: &Window, cx: &gpui_kit::Context<Self>) -> bool {
        let Some(ed) = &self.editing else {
            return false;
        };
        if ed.preview || !ed.content.read(cx).has_selection_focus(window, cx) {
            return false;
        }
        !self.vim_enabled || self.vim.mode == crate::vim::VimMode::Insert
    }

    /// Issue 21 需求4: Enter on a list line continues the marker
    /// (insert mode / vim off — the vim capture runs first).
    fn enter_list_continue(
        &mut self,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> bool {
        let Some(ed) = &self.editing else {
            return false;
        };
        if ed.preview {
            return false;
        }
        let editor = ed.content.clone();
        let hit = editor.update(cx, |state, cx| {
            crate::markdown_input::enter_continue_list(state, window, cx)
        });
        if hit {
            self.schedule_autosave(cx);
        }
        hit
    }

    /// Issue 21 需求5: bare-key pair closing / fence completion in the
    /// editor (insert mode / vim off). Returns true when consumed.
    fn md_pair_key(
        &mut self,
        key_char: char,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> bool {
        let Some(ed) = &self.editing else {
            return false;
        };
        if ed.preview {
            return false;
        }
        let editor = ed.content.clone();
        let hit = editor.update(cx, |state, cx| {
            crate::markdown_input::close_pair_on_key(state, key_char, window, cx)
        });
        if hit {
            self.schedule_autosave(cx);
        }
        hit
    }

    /// 光标前一字符是否为 `c`（斜杠面板触发防误触用）。
    fn char_before_caret_is(&self, c: char, cx: &mut gpui_kit::App) -> bool {
        let Some(ed) = &self.editing else {
            return false;
        };
        let editor = ed.content.clone();
        editor.update(cx, |state, _| {
            let range = state.selected_range();
            let caret = range.start.min(range.end);
            caret > 0 && state.value().as_ref().chars().nth(caret - 1) == Some(c)
        })
    }
    /// Issue 21 需求5: Tab/Shift-Tab on a list line indents the line
    /// start; other lines keep the kit default. Returns true when
    /// consumed.
    fn md_indent(
        &mut self,
        outdent: bool,
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> bool {
        let Some(ed) = &self.editing else {
            return false;
        };
        if ed.preview {
            return false;
        }
        let editor = ed.content.clone();
        let hit = editor.update(cx, |state, cx| {
            crate::markdown_input::indent_list_line(state, outdent, window, cx)
        });
        if hit {
            self.schedule_autosave(cx);
        }
        hit
    }

    /// Flat (id, title, depth) of every valid move target: root + all
    /// notebooks except the moved one and its descendants (no cycles).
    fn notebook_move_targets(&mut self, moved: &leanote_core::Id) -> Vec<(String, String, usize)> {
        let banned = self.notebook_banned_set(&moved.to_string());
        let mut out = vec![(
            String::new(),
            i18n::t(self.lang, i18n::Key::RootLevel).to_string(),
            0usize,
        )];
        let nodes = self.notebook_nav_nodes().clone();
        for node in nodes {
            if !banned.contains(&node.id) {
                out.push((node.id.clone(), node.title.clone(), node.depth as usize + 1));
            }
        }
        out
    }

    /// Sidebar drag: drop target for the pointer position over the notebook
    /// list (content-relative y). Bands and sibling-index derivation live in
    /// [`crate::browse::drop_target`]. None when the pointer is off the
    /// rows or the target is the dragged notebook or one of its descendants
    /// (cycle guard).
    fn notebook_drop_at(&mut self, dragged: &str, y: f32, row_h: f32) -> Option<DropTarget> {
        let nodes = self.notebook_nav_nodes().clone();
        let ix = (y / row_h).floor() as isize;
        if ix < 0 || ix as usize >= nodes.len() {
            return None;
        }
        let target =
            browse::drop_target(&nodes, ix as usize, y - row_h * ix as f32, row_h, dragged)?;
        // Cycle guard: never drop into the dragged notebook or any of its
        // descendants (banned set). For the inner band `target.parent` IS
        // the hovered row, so one check covers both.
        let banned = self.notebook_banned_set(dragged);
        if banned.contains(&target.parent) {
            return None;
        }
        Some(target)
    }

    /// The dragged notebook plus all its descendants (invalid drop-inside
    /// targets — no cycles). Same walk as [`Self::notebook_move_targets`].
    fn notebook_banned_set(&self, dragged: &str) -> std::collections::HashSet<String> {
        let mut banned = std::collections::HashSet::new();
        banned.insert(dragged.to_string());
        if let Some(store) = self.store() {
            if let Ok(nbs) = store.notebooks() {
                let by_id: std::collections::HashMap<String, String> = nbs
                    .iter()
                    .map(|nb| (nb.notebook_id.to_string(), nb.parent_notebook_id.clone()))
                    .collect();
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
            }
        }
        banned
    }

    /// Commit a sidebar drag: rewrite the whole target sibling group (and
    /// the old one), JS dragNotebooks semantics, via `move_notebook`.
    fn drop_notebook(
        &mut self,
        drag: &NotebookDrag,
        target: DropTarget,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Some(store) = self.store() {
            let _ = store.move_notebook(
                &leanote_core::Id(drag.id.clone()),
                &target.parent,
                target.index,
                &[],
            );
        }
        self.notebook_drag_target = None;
        self.invalidate_lists();
        // Reorder is server-visible state (Seq/ParentNotebookId push) — sync
        // right away instead of waiting for the manual cloud button.
        self.sync_now(cx);
        cx.notify();
    }

    /// Input + OK/Cancel row for the in-flight tag rename (notebook
    /// New/Rename use the modal dialog now).
    fn nav_edit_row(
        &mut self,
        _dark: bool,
        t: &dyn Fn(i18n::Key) -> &'static str,
        group: &str,
        _window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> Option<AnyElement> {
        // borrow dance: take the input entity handle out temporarily
        let edit = self.nav_edit.clone()?;
        let input = match &edit {
            NavEdit::RenameTag { name, .. } => name.clone(),
            _ => return None,
        };
        let indent = 1;
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
        let notebook_names = self.notebook_names().clone();
        let notes = self.visible_notes().clone();
        let title: std::borrow::Cow<str> = match &self.nav {
            NavSelection::Recent => t(i18n::Key::NavRecent).into(),
            NavSelection::Starred => t(i18n::Key::NavStarred).into(),
            NavSelection::Trash => t(i18n::Key::NavTrash).into(),
            NavSelection::Notebook(id) => notebook_names
                .get(id)
                .map(|s| s.clone())
                .unwrap_or_else(|| t(i18n::Key::NavNotebooks).to_string())
                .into(),
            NavSelection::Tag(tag) => tag.as_str().into(),
        };
        // Mirrors the left nav row: same icon + 14px label + muted icon
        // color (user round: header should read as the selected nav row).
        let icon = match &self.nav {
            NavSelection::Recent => Some(AssetIcon::BookOpen),
            NavSelection::Starred => Some(AssetIcon::Star),
            NavSelection::Trash => Some(AssetIcon::Trash),
            NavSelection::Notebook(_) => Some(AssetIcon::FolderClosed),
            NavSelection::Tag(_) => Some(AssetIcon::Tag),
        };

        // Issue: new-note only in a notebook context; Recent/Starred/Tag
        // have no add button (⌘K quick action still works anywhere, falling
        // back to the first notebook).
        let mut header = gpui_kit::base::h_flex()
            .gap_2()
            .pl(px(4.))
            .when_some(icon, |el, icon| {
                el.child(Icon::new(icon).small().text_color(muted))
            })
            .child(
                Label::new(title.clone())
                    .text_size(theme::ui(14.))
                    .text_color(fg),
            )
            .child(gpui_kit::base::h_flex().flex_1());
        match &self.nav {
            NavSelection::Notebook(_) => {
                header = header.child(
                    Button::new("new-note")
                        .icon(IconName::Plus)
                        .ghost()
                        .tooltip(t(i18n::Key::TooltipNewNote))
                        .on_click(cx.listener(|this, _ev, window, cx| this.new_note(window, cx))),
                );
            }
            // Trash gets a destructive "empty" action instead (confirmed
            // via AlertDialog — empty_trash is irreversible after sync).
            NavSelection::Trash => {
                header = header.child(
                    Button::new("empty-trash")
                        .icon(AssetIcon::Trash)
                        .tooltip(t(i18n::Key::TooltipEmptyTrash))
                        .on_click(cx.listener(|this, _ev, window, cx| {
                            this.confirm_empty_trash(window, cx)
                        })),
                );
            }
            NavSelection::Recent | NavSelection::Starred | NavSelection::Tag(_) => {}
        }

        let mut list = gpui_kit::base::v_flex()
            .w_full()
            .h_full()
            // Round 5: no glass outside the nav — the list column is a
            // solid surface. (Round 3 stacked translucent List over an
            // opaque fill; same pixels, one fill fewer now.)
            .bg(theme::material(Material::Background, dark))
            // Issue 17: list column runs flush to the window top (drag
            // region is the left nav's 44px header only).
            .px_3()
            .gap_1()
            // UI round: 10px from the window top — only the left nav yields
            // to the traffic lights; this column needs no titlebar clearance.
            .pt(px(10.))
            .child(header);

        if notes.is_empty() {
            list = list.child(
                gpui_kit::base::v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .child(small_label(t(i18n::Key::EmptyNotes), muted)),
            );
        } else {
            // Issue 17: virtualized — only visible rows render.
            let sel_id = self.selected.clone();
            let multi: std::collections::HashSet<leanote_core::Id> =
                self.selected_ids.iter().cloned().collect();
            let shell = cx.weak_entity();
            let notes_for_list = notes.clone();
            let scroll = self.notes_scroll.clone();
            let list_view = gpui_kit::div()
                .id("notes-vlist")
                .relative()
                .flex_1()
                .min_h_0()
                .child(
                    gpui_kit::uniform_list(
                        "notes-ul",
                        notes_for_list.len(),
                        move |range, _window, _cx| {
                            range
                                .map(|ix| {
                                    let n = &notes_for_list[ix];
                                    let meta = browse::note_meta(
                                        notebook_names
                                            .get(&n.notebook_id.to_string())
                                            .map(|s| s.as_str())
                                            .unwrap_or(""),
                                        n.updated_time,
                                    );
                                    // Issue 19: content excerpt (store-side
                                    // desc), single truncated line.
                                    let desc: SharedString = n.desc.clone().into();
                                    let is_sel = sel_id.as_ref() == Some(&n.note_id)
                                        || multi.contains(&n.note_id);
                                    let divided = ix + 1 < notes_for_list.len();
                                    note_list_row(
                                        &shell,
                                        n,
                                        meta.into(),
                                        desc,
                                        is_sel,
                                        divided,
                                        dark,
                                        fg,
                                        sel_fg,
                                        muted,
                                    )
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                    .h_full()
                    .w_full()
                    .track_scroll(&scroll),
                )
                .child(
                    gpui_kit::div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .w(theme::ui(12.))
                        .child(
                            Scrollbar::vertical(&scroll)
                                .viewport_from_layout()
                                .max_fps(60),
                        ),
                );
            list = list.child(list_view);
        }

        // Issue 18: "move note" picker replaces the note rows while
        // choosing (mirrors the nav's notebook-move picker). Feature 3:
        // MoveNoteBatch shares it, committing to the whole selection.
        #[derive(Clone)]
        enum MoveTarget {
            One(leanote_core::Id),
            Batch(Vec<leanote_core::Id>),
        }
        let target = match &self.nav_edit {
            Some(NavEdit::MoveNote { id }) => Some(MoveTarget::One(id.clone())),
            Some(NavEdit::MoveNoteBatch) if !self.selected_ids.is_empty() => {
                Some(MoveTarget::Batch(self.selected_ids.clone()))
            }
            _ => None,
        };
        if let Some(target) = target {
            let count = match &target {
                MoveTarget::One(_) => 1,
                MoveTarget::Batch(v) => v.len(),
            };
            let shell = cx.weak_entity();
            let mut picker = gpui_kit::base::v_flex().flex_1().min_h_0().gap_1().py_2();
            let head = if count > 1 {
                format!("{} ({})", t(i18n::Key::MoveNoteTo), count)
            } else {
                format!("{} · {}", t(i18n::Key::MoveNoteTo), title)
            };
            picker = picker.child(Label::new(head).text_size(theme::ui(16.)).text_color(fg));
            let nodes = self.notebook_nav_nodes().clone();
            for node in &nodes {
                let shell2 = shell.clone();
                let pid = node.id.clone();
                let tgt = target.clone();
                picker = picker.child(
                    gpui_kit::base::h_flex()
                        .id(format!("note-mv-{pid}"))
                        .h_8()
                        .px_3()
                        .ml(px(12. * node.depth as f32))
                        .rounded(px(theme::RADIUS_SELECTION))
                        .on_click(move |_, _, cx| {
                            let _ = shell2.update(cx, |this, cx| match &tgt {
                                MoveTarget::One(id) => {
                                    this.move_note_to(id.clone(), pid.to_string(), cx)
                                }
                                MoveTarget::Batch(_) => this.batch_move_to(pid.to_string(), cx),
                            });
                        })
                        .child(Label::new(node.title.clone()).text_size(theme::ui(14.))),
                );
            }
            let shell3 = shell.clone();
            picker = picker.child(
                Button::new("note-mv-cancel")
                    .label(t(i18n::Key::Cancel).to_string())
                    .on_click(move |_, _, cx| {
                        let _ = shell3.update(cx, |this, cx| {
                            this.nav_edit = None;
                            cx.notify();
                        });
                    }),
            );
            return gpui_kit::base::v_flex()
                .w_full()
                .h_full()
                // Opaque: no glass outside the nav (round 5).
                .bg(theme::material(Material::Background, dark))
                .px_3()
                .gap_1()
                .pt(px(10.))
                .child(picker);
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
        let prefs = crate::prefs::Prefs::load(&self.data_dir);
        let muted = theme::muted_foreground(dark);
        let notebook_names = self.notebook_names().clone();

        let pane = gpui_kit::base::v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .bg(theme::material(Material::Background, dark))
            // Issue 17: top strip is the right-pane drag region.
            .child(reader_drag_strip(28., window, cx));

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
        // Feature 3: multi-select replaces the reader with a batch
        // summary — no single note to read, and it makes the modal
        // state loud.
        if self.selected_ids.len() > 1 {
            let count = self.selected_ids.len();
            let mut mask = gpui_kit::base::v_flex();
            if let Some(bar) = bar {
                mask = mask.child(bar);
            }
            return pane.child(
                mask.flex_1().child(
                    gpui_kit::base::v_flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .child(
                            Label::new(format!("{} {}", count, t(i18n::Key::MultiSelected)))
                                .text_size(theme::ui(16.))
                                .text_color(muted),
                        )
                        .child(small_label(t(i18n::Key::MultiSelectHint), muted)),
                ),
            );
        }

        if self
            .editing
            .as_ref()
            .is_some_and(|ed| Some(&ed.note_id) == self.selected.as_ref())
        {
            return self.render_editor(dark, t, window, cx);
        }

        let Some(n) = note else {
            let mut empty = gpui_kit::base::v_flex();
            if let Some(bar) = bar {
                empty = empty.child(bar);
            }
            // Centered icon + one-line hint; wording flips when the
            // current list itself is empty (no note to select).
            let list_empty = self.visible_notes().is_empty();
            let hint = if list_empty {
                i18n::t(self.lang, i18n::Key::EmptyReaderHint)
            } else {
                i18n::t(self.lang, i18n::Key::NoSelection)
            };
            return pane.child(
                empty.flex_1().child(
                    gpui_kit::base::v_flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .child(Icon::new(AssetIcon::FileText).large().text_color(muted))
                        .child(Label::new(hint).text_size(theme::ui(14.)).text_color(muted)),
                ),
            );
        };

        let is_trash_view = self.nav == NavSelection::Trash;
        let nb_name = notebook_names
            .get(&*n.notebook_id.to_string())
            .cloned()
            .unwrap_or_else(|| n.notebook_id.to_string());

        // Issue 21: star + exports moved to the row context menu; the
        // toolbar keeps convert (rich text), trash actions and the
        // export-status label.
        let mut toolbar = gpui_kit::base::h_flex().gap_2();
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
            toolbar = toolbar.child(small_label(t(i18n::Key::ConvertedToMarkdown), muted));
        }
        if is_trash_view {
            let rid = n.note_id.clone();
            toolbar = toolbar.child(
                Button::new("restore")
                    .label(t(i18n::Key::Restore))
                    .outline()
                    .on_click(cx.listener(move |this, _ev, _window, cx| {
                        this.restore_selected(rid.clone(), cx)
                    })),
            );
            let did = n.note_id.clone();
            toolbar = toolbar.child(
                Button::new("hard-delete")
                    .label(t(i18n::Key::HardDelete))
                    .outline()
                    .on_click(cx.listener(move |this, _ev, _window, cx| {
                        this.hard_delete_selected(did.clone(), cx)
                    })),
            );
            toolbar = toolbar.child(
                Button::new("empty-trash")
                    .label(t(i18n::Key::EmptyTrash))
                    .outline()
                    .on_click(cx.listener(|this, _ev, _window, cx| this.empty_trash(cx))),
            );
        }

        if let Some(status) = self.export_status.clone() {
            toolbar = toolbar.child(
                Label::new(status)
                    .text_size(theme::ui(12.))
                    .text_color(theme::muted_foreground(dark)),
            );
        }

        let content = self
            .store()
            .map(|s| browse::resolve_note_images(&n.content, &s))
            .unwrap_or_else(|| n.content.clone());
        let src = browse::reader_markdown_src(&n.title, &content, n.is_markdown);

        let preview_px = px(theme::preview_font_px(&prefs.editor_font_size, prefs.zoom));
        let mut col = gpui_kit::base::v_flex()
            .px(px(5.))
            .gap_2()
            .min_h_0()
            .text_size(preview_px);
        col = col
            .child(
                gpui_kit::base::h_flex()
                    .gap_2()
                    .child(small_label(
                        browse::note_meta(&nb_name, n.updated_time),
                        muted,
                    ))
                    .child(gpui_kit::base::h_flex().flex_1()),
            )
            .child(toolbar)
            // GFM tables + strikethrough ship in TextView's default GFM
            // parse options; `.scrollable` keeps the whole pane from
            // scrolling away under the toolbar.
            .child(
                crate::mermaid::install(TextView::markdown("reader", src), dark)
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
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> gpui_kit::Div {
        let fg = theme::foreground(dark);
        let muted = theme::muted_foreground(dark);

        let pane = gpui_kit::base::v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .bg(theme::material(Material::Background, dark))
            // Issue 21: slim 10px drag strip above the title row (the
            // reader keeps its 28px strip); the caller nests this pane
            .child(reader_drag_strip(10., window, cx));

        let Some(ed) = &self.editing else {
            unreachable!("caller checked")
        };
        let title_entity = ed.title.clone();
        let content_entity = ed.content.clone();
        let preview = ed.preview;
        // issue 23 #10 工具栏已由斜杠面板（ADR-0011）取代。
        let saved_flash = ed.saved_flash;

        // Issue 19: title row = title input + ghost icon buttons; save runs
        // via autosave/⌘S with a transient "Saved" flash, so no Save button.
        let (toggle_key, toggle_icon) = if preview {
            (i18n::Key::EditorSource, AssetIcon::Code)
        } else {
            (i18n::Key::EditorPreview, AssetIcon::Eye)
        };
        let icon_btn = |id: &'static str,
                        icon: AssetIcon,
                        tip: &'static str,
                        on_click: Box<
            dyn Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App),
        >| {
            Button::new(id)
                .icon(icon)
                .ghost()
                .small()
                .tooltip(tip)
                .on_click(on_click)
        };
        let title_row = gpui_kit::base::h_flex()
            .gap_2()
            .child(
                // Issue 21 / UI round 10: title = 1.25rem — scales with the
                // interface zoom. 32px row at 100% (28px text + descender
                // room); the fixed 20px/36px pair ignored zoom.
                Input::new(&title_entity)
                    .appearance(false)
                    .bordered(false)
                    .text_size(rems(1.25))
                    .h(theme::ui(32.))
                    .flex_1()
                    .min_w_0(),
            )
            .when(saved_flash, |el| {
                el.child(small_label(t(i18n::Key::Saved), muted))
            })
            .child(
                Button::new("toggle-preview")
                    .icon(toggle_icon)
                    .ghost()
                    .small()
                    .tooltip(t(toggle_key))
                    .on_click(cx.listener(|this, _ev, _window, cx| {
                        if let Some(ed) = &mut this.editing {
                            ed.preview = !ed.preview;
                        }
                        cx.notify();
                    })),
            )
            .child(icon_btn(
                "open-attachments",
                AssetIcon::Paperclip,
                t(i18n::Key::Attachments),
                Box::new(cx.listener(|this, _ev, window, cx| this.open_attachments(window, cx))),
            ))
            .child(icon_btn(
                "open-history",
                AssetIcon::Clock,
                t(i18n::Key::History),
                Box::new(cx.listener(|this, _ev, window, cx| this.open_history(window, cx))),
            ));
        // Issue 21: no Done and no OS-fullscreen button — quitting edit is
        // selecting another note; fullscreen is the ⌘T in-app preview.

        // Body: preview via TextView, source via kit Editor. Title input
        // lives in the title row; only the content scrolls.
        let body = if preview {
            let src = content_entity.read(cx).value().to_string();
            let title_txt = title_entity.read(cx).value().to_string();
            let prefs = crate::prefs::Prefs::load(&self.data_dir);
            gpui_kit::base::v_flex()
                .min_h_0()
                .flex_1()
                // Issue 23 #7: 10px side margins (editor-internal preview;
                // the ⌘T full-window preview keeps its 32px frame).
                .px(px(10.))
                .text_size(px(theme::preview_font_px(
                    &prefs.editor_font_size,
                    prefs.zoom,
                )))
                .child(
                    crate::mermaid::install(
                        TextView::markdown(
                            "editor-preview",
                            browse::reader_markdown_src(
                                &title_txt,
                                &self
                                    .store()
                                    .map(|s| browse::resolve_note_images(&src, &s))
                                    .unwrap_or_else(|| src.clone()),
                                true,
                            ),
                        ),
                        dark,
                    )
                    .scrollable(true)
                    .on_link_click(|url, _ev, _window, cx| cx.open_url(url)),
                )
                .into_any_element()
        } else {
            gpui_kit::base::v_flex()
                .min_h_0()
                .flex_1()
                .child(Editor::new(&content_entity).flex_1().min_h_0().h_full())
                .into_any_element()
        };

        // Status bar (tags left w/ icon, updated time right w/ icon) sits
        // directly under the title row — issue: move out of the bottom.
        // Edit mode only; preview keeps the clean look.
        let status_bar = if !preview {
            let ed = self.editing.as_ref().unwrap();
            Some(
                gpui_kit::base::h_flex()
                    .w_full()
                    .h_7()
                    .gap_2()
                    .px_2()
                    .mb_0()
                    // UI round 10 #2: no bottom rule — the tag chips and
                    // clock already read as a row; the border made the
                    // editor look boxed-in.
                    .child(
                        gpui_kit::base::h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(Icon::new(AssetIcon::Tag).xsmall().text_color(muted))
                            .children(ed.tags.iter().cloned().map(|tag| {
                                let shell_t = cx.entity().downgrade();
                                let tag_id: SharedString = format!("tag-x-{tag}").into();
                                gpui_kit::base::h_flex()
                                    .gap_1()
                                    .px_2()
                                    .rounded(px(theme::RADIUS_SELECTION))
                                    .bg(theme::material(Material::List, dark))
                                    .child(small_label(SharedString::from(tag.clone()), muted))
                                    .child(
                                        Button::new(tag_id.clone())
                                            .icon(AssetIcon::X)
                                            .ghost()
                                            .xsmall()
                                            .on_click(move |_, _window, cx| {
                                                let _ = shell_t.update(cx, |this, cx| {
                                                    this.remove_tag(tag.clone(), cx)
                                                });
                                            }),
                                    )
                            }))
                            .child(
                                Input::new(&ed.tag_input)
                                    .xsmall()
                                    .w(theme::ui(120.))
                                    .cleanable(false),
                            ),
                    )
                    .child(
                        gpui_kit::base::h_flex()
                            .gap_1()
                            .flex_none()
                            .child(Icon::new(AssetIcon::Clock).xsmall().text_color(muted))
                            .child(
                                small_label(SharedString::from(fmt_time(ed.updated_time)), muted)
                                    .text_size(theme::ui(12.)),
                            ),
                    ),
            )
        } else {
            None
        };

        let col = gpui_kit::base::v_flex()
            .flex_1()
            .min_w_0()
            .px(px(5.))
            .gap_2()
            .min_h_0()
            // UI round 10 #4: 1rem base for the editor/preview column so
            // its intrinsic text scales with the interface zoom.
            .text_size(rems(1.0))
            // Editor column ends 5px above the window bottom (issue #2).
            .pb(px(5.))
            .h_full()
            .child(title_row)
            .when_some(self.slash_panel.as_ref(), |el, p| {
                // Issue 37: 格子点击执行、hover 同步选中 —— 处理闭包在
                // slash_panel::render 内经 shell 实体回写。
                el.children(crate::slash_panel::render(
                    &content_entity,
                    p,
                    self.dark(cx),
                    self.lang,
                    cx.entity(),
                ))
            })
            .when_some(status_bar, |el, bar| el.child(bar))
            .child(body)
            .on_drop::<gpui_kit::ExternalPaths>(cx.listener(
                |this, paths: &gpui_kit::ExternalPaths, window, cx| {
                    // Issue 21 需求9: dropped image files attach to the note.
                    let imgs: Vec<std::path::PathBuf> = paths
                        .paths()
                        .iter()
                        .filter(|p| {
                            p.extension()
                                .and_then(|e| e.to_str())
                                .map(|e| {
                                    matches!(
                                        e.to_ascii_lowercase().as_str(),
                                        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg"
                                    )
                                })
                                .unwrap_or(false)
                        })
                        .cloned()
                        .collect();
                    if !imgs.is_empty() {
                        this.attach_images(imgs, window, cx);
                    }
                },
            ))
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                // 斜杠面板打开时优先消费按键（ADR-0011）。必须在
                // ⌘S/vim/配对处理之前。Enter/Tab/方向键在 kit 层是
                // 动作（Enter/IndentInline/Move*），不走 key_down ——
                // 由各自 capture_action 的面板分支执行。
                if this.slash_panel.is_some() {
                    let lang = this.lang;
                    let mut panel = this.slash_panel.clone().unwrap();
                    // Esc 关闭时把触发串 `/`+query 写回编辑器（用户
                    // 需求 3）：触发时本键已被吃掉，不回填则 `/` 无法
                    // 正常输入。backspace 到头/过滤空关闭不回填。
                    let restore = ev.keystroke.key == "escape";
                    match crate::slash_panel::key(&mut panel, ev, lang) {
                        None => {
                            this.slash_panel = None;
                            if restore {
                                let text = format!("/{}", panel.query);
                                if let Some(ed) = &this.editing {
                                    if !ed.preview {
                                        let editor = ed.content.clone();
                                        editor.update(cx, |state, cx| {
                                            let range = state.selected_range();
                                            let caret = range.start.min(range.end);
                                            state.set_selected_range(caret..caret, cx);
                                            state.replace(&text, window, cx);
                                            let end = caret + text.chars().count();
                                            state.set_selected_range(end..end, cx);
                                        });
                                    }
                                }
                            }
                        }
                        Some(false) => {}
                        Some(true) => {
                            this.slash_panel = Some(panel);
                        }
                    }
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                // ⌘S / Ctrl+S save + push (IME-safe: key_char is None for
                // cmd combos, so composition is never touched). Save marks
                // the row IsDirty=1; sync_now pushes it.
                let m = &ev.keystroke.modifiers;
                if (m.platform || m.control)
                    && !m.alt
                    && !m.shift
                    && ev.keystroke.key.eq_ignore_ascii_case("s")
                {
                    this.save_editor(cx);
                    this.sync_now(cx);
                    cx.stop_propagation();
                    return;
                }
                // Tab in the title input → focus the markdown body (issue
                // #3). Intercepted before the input's own IndentInline.
                if ev.keystroke.key == "tab" && !m.platform && !m.control && !m.alt {
                    let in_title = this
                        .editing
                        .as_ref()
                        .is_some_and(|ed| ed.title.read(cx).has_selection_focus(window, cx));
                    if in_title && !this.editing.as_ref().is_some_and(|ed| ed.preview) {
                        if let Some(ed) = &this.editing {
                            ed.content.update(cx, |state, cx| state.focus(window, cx));
                        }
                        cx.stop_propagation();
                        return;
                    }
                }
                // Vim grammar: bare keys carry no kit bindings, so this
                // capture listener is the first hook that sees them —
                // ahead of the input's own handlers and of AppKit
                // turning the key into text. Kit-bound keys (enter/
                // escape/backspace/delete) dispatch as actions and are
                // intercepted by the capture_action hooks below instead.
                if this.vim_key_down(&ev.keystroke, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                // Issue 21 需求5: markdown pair closing / fence — insert
                // mode only (vim normal/visual consumed above), IME-safe
                // (key_char present, no modifiers), kit defaults for the
                // pairs are disabled at startup.
                // 斜杠面板触发（ADR-0011）：插入态键入 `/` 且前一字符
                // 不是 `/`（避免路径/URL）→ 打开面板，吃掉本键。
                let is_slash = ev
                    .keystroke
                    .key_char
                    .as_deref()
                    .and_then(|s| s.chars().next())
                    == Some('/');
                if is_slash
                    && this.slash_panel.is_none()
                    && this.editor_insert_focused(window, cx)
                    && !this.char_before_caret_is('/', cx)
                {
                    this.slash_panel = Some(crate::slash_panel::SlashPanel::default());
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                let m2 = &ev.keystroke.modifiers;
                if !m2.platform && !m2.control && !m2.alt && !m2.shift {
                    if let Some(ch) = ev
                        .keystroke
                        .key_char
                        .as_deref()
                        .and_then(|s| s.chars().next())
                    {
                        if matches!(ch, '*' | '~' | '`' | '[' | '(')
                            && this.editor_insert_focused(window, cx)
                            && this.md_pair_key(ch, window, cx)
                        {
                            cx.stop_propagation();
                        }
                    }
                }
            }))
            // 方向键在 kit 层是 Move* 动作（key_down 看不到）——斜杠
            // 面板打开时截下来做网格导航，否则落到编辑器移动光标
            // （用户需求 2）。
            .capture_action::<MoveUp>(cx.listener(move |this, _: &MoveUp, _, cx| {
                this.slash_move("up", cx);
            }))
            .capture_action::<MoveDown>(cx.listener(move |this, _: &MoveDown, _, cx| {
                this.slash_move("down", cx);
            }))
            .capture_action::<MoveLeft>(cx.listener(move |this, _: &MoveLeft, _, cx| {
                this.slash_move("left", cx);
            }))
            .capture_action::<MoveRight>(cx.listener(move |this, _: &MoveRight, _, cx| {
                this.slash_move("right", cx);
            }))
            .capture_action::<Enter>(cx.listener(move |this, _: &Enter, window, cx| {
                // Slash panel first: Enter is an action (never reaches
                // capture_key_down), so without this branch the selected
                // command never runs (user report 1).
                if this.slash_panel.is_some() {
                    let selected = this.slash_panel.as_ref().unwrap().selected;
                    crate::slash_panel::execute(this, selected, window, cx);
                    this.slash_panel = None;
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                // Normal-mode enter / cmdline execute first; insert mode
                // (or vim off) falls to the list continuation, then to
                // the kit's plain enter.
                if this.vim_action_key("enter", window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if this.enter_list_continue(window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action::<Escape>(cx.listener(move |this, _: &Escape, window, cx| {
                // Normal/visual escape (leave visual). Insert esc is
                // handled in vim_key_down via the fallback path.
                if this.vim_action_key("escape", window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action::<Backspace>(cx.listener(move |this, _: &Backspace, window, cx| {
                // Normal backspace moves left; cmdline backspace edits.
                if this.vim_action_key("backspace", window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action::<Delete>(cx.listener(move |this, _: &Delete, window, cx| {
                // Normal delete = x (delete char under cursor).
                if this.vim_action_key("delete", window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action::<IndentInline>(cx.listener(
                move |this, _: &IndentInline, window, cx| {
                    // Slash panel first — Tab is an action like Enter
                    // (user report 1).
                    if this.slash_panel.is_some() {
                        let selected = this.slash_panel.as_ref().unwrap().selected;
                        crate::slash_panel::execute(this, selected, window, cx);
                        this.slash_panel = None;
                        cx.notify();
                        cx.stop_propagation();
                        return;
                    }
                    // Issue 21 需求5: list lines indent at line start; other
                    // lines keep the kit default (2-space at cursor).
                    if this.editor_insert_focused(window, cx) && this.md_indent(false, window, cx) {
                        cx.stop_propagation();
                    }
                },
            ))
            .capture_action::<OutdentInline>(cx.listener(
                move |this, _: &OutdentInline, window, cx| {
                    if this.editor_insert_focused(window, cx) && this.md_indent(true, window, cx) {
                        cx.stop_propagation();
                    }
                },
            ));
        // Issue 21: vim status line removed — the block caret is the
        // mode indicator (ADR-0005); :w/:q still work.

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

/// Measured pixel height of one notebook row: the uniform list records
/// `contents = item_h * count` in its scroll handle at layout time
/// (uniform_list.rs 359-377). Rows are `h_8()` = rems(2.0), so this is
/// 32px only when the rem size is exactly 16 — trust the measurement, not
/// the constant.
fn notebooks_row_height(shell: &AppShell, node_count: usize) -> f32 {
    let state = shell.notebooks_scroll.0.borrow();
    match &state.last_item_size {
        Some(s) => {
            let n = node_count as f32;
            let h: f32 = s.contents.height.into();
            let measured = h / n;
            // Layout may lag a frame behind a list change; fall back
            // rather than divide garbage into the drop math.
            if n > 0. && (1.0..1000.0).contains(&measured) {
                return measured;
            }
            32.
        }
        None => 32.,
    }
}

/// Notebooks list scroll offset in px, positive when scrolled down. Rows
/// live in content space, so pointer y must lose this.
fn notebooks_scroll_offset(shell: &AppShell) -> f32 {
    let state = shell.notebooks_scroll.0.borrow();
    let off: f32 = state.base_handle.offset().y.into();
    -off
}

/// epoch ms → "YYYY-MM-DD" (display only; store keeps epoch ms).
pub(crate) fn fmt_time(ms: i64) -> String {
    let (y, m, d) = crate::export::civil_from_days(ms.div_euclid(86_400_000));
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
        assert_eq!(
            NavSelection::Notebook("nb1".into()),
            NavSelection::Notebook("nb1".into())
        );
        assert_ne!(
            NavSelection::Notebook("nb1".into()),
            NavSelection::Tag("nb1".into())
        );
        assert_eq!(
            NavSelection::Tag("rust".into()),
            NavSelection::Tag("rust".into())
        );
    }

    /// End-to-end vim pipeline check: real AppShell view rendered in a test
    /// window, keystrokes dispatched through the full gpui pipeline (bindings
    /// → actions → key listeners), asserting against the real editor buffer.

    #[gpui_kit::test]
    fn vim_pipeline_moves_cursor_and_inserts(cx: &mut gpui_kit::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("leanote-vimtest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = leanote_core::Store::open(dir.clone(), "user1".to_string()).unwrap();
        let note = store
            .add_note(leanote_core::NewNote {
                title: "t".into(),
                content: "hello world".into(),
                tags: vec![],
                notebook_title: "nb".into(),
                is_markdown: true,
            })
            .unwrap();

        cx.update(gpui_kit::init);
        let mut shell_entity: Option<gpui_kit::Entity<AppShell>> = None;
        let window = cx
            .update(|cx| {
                cx.open_window(gpui_kit::WindowOptions::default(), |window, cx| {
                    let shell = cx.new(|cx| {
                        AppShell::new(
                            window,
                            cx,
                            dir.clone(),
                            crate::i18n::Lang::En,
                            true,
                            Some("user1".into()),
                            None,
                            None,
                        )
                    });
                    shell_entity = Some(shell.clone());
                    shell
                })
            })
            .unwrap();
        let shell_entity = shell_entity.unwrap();
        let note_id = note.note_id.clone();

        // Select the note → start_editing → editor column exists + focused.
        window
            .update(cx, |shell, window, cx| {
                shell.select_note(note_id, window, cx);
            })
            .unwrap();
        cx.run_until_parked();
        // The focused editor's blink cursor is an endless timer loop;
        // without this, run_until_parked spins forever on fake clocks.
        cx.background_executor.allow_parking();

        let mut cx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
        // Lay out the real root so the column's capture hooks exist in the
        // rendered dispatch tree.
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let content = shell_entity.read_with(&cx, |shell, _| {
            shell.editing.as_ref().unwrap().content.clone()
        });
        cx.update(|window, cx| {
            assert!(
                content.read(cx).has_selection_focus(window, cx),
                "editor must hold focus after select_note"
            );
            assert_eq!(content.read(cx).value(), "hello world");
        });
        // ADR-0005: a fresh editor session starts in normal mode with the
        // native block caret — collapsed selection + block flag set.
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).selected_range(),
                0..0,
                "normal mode must not select anything (native block caret)"
            );
            assert!(
                content.read(cx).block_cursor(),
                "normal mode must paint the block caret"
            );
        });
        // Normal-mode "l" moves right without typing — driven through
        // the REAL dispatch path (dispatch_keystroke → dispatch_event →
        // capture listeners), so the capture wiring itself is under
        // test. Without the capture listener, "l" would type into the
        // buffer.
        cx.simulate_keystrokes("l");
        cx.update(|_, cx| {
            let range = content.read(cx).selected_range();
            assert_eq!(range, 1..1, "vim l moves the collapsed caret");
        });
        // "^" jumps to the first non-blank char ("hello world": col 0),
        // driven through the real dispatch chain (shifted symbol).
        cx.simulate_keystrokes("^");
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).selected_range(),
                0..0,
                "^ must land on the first non-blank char"
            );
        });
        cx.simulate_keystrokes("l"); // restore caret at col 1
        cx.update(|_, cx| {
            assert_eq!(content.read(cx).selected_range(), 1..1);
        });
        // "i" + typing inserts at cursor; escape returns to normal.
        cx.simulate_keystrokes("i");
        cx.update(|_, cx| {
            assert!(
                !content.read(cx).block_cursor(),
                "insert mode must use the thin caret"
            );
        });
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).selected_range(),
                2..2,
                "esc from insert must return the collapsed normal caret"
            );
            assert!(content.read(cx).block_cursor());
        });
        cx.simulate_keystrokes("x");
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "hXllo world",
                "i→type→esc→x pipeline"
            );
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slash panel end-to-end: `/` opens the panel (key never lands in the
    /// buffer), filter narrows, Enter executes at the caret — no phantom
    /// deletion before it (user report 3).
    #[gpui_kit::test]
    fn slash_panel_inserts_at_caret(cx: &mut gpui_kit::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("leanote-slashtest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = leanote_core::Store::open(dir.clone(), "user1".to_string()).unwrap();
        let note = store
            .add_note(leanote_core::NewNote {
                title: "t".into(),
                content: "line one\nline two".into(),
                tags: vec![],
                notebook_title: "nb".into(),
                is_markdown: true,
            })
            .unwrap();

        cx.update(gpui_kit::init);
        let mut shell_entity: Option<gpui_kit::Entity<AppShell>> = None;
        let window = cx
            .update(|cx| {
                cx.open_window(gpui_kit::WindowOptions::default(), |window, cx| {
                    let shell = cx.new(|cx| {
                        AppShell::new(
                            window,
                            cx,
                            dir.clone(),
                            crate::i18n::Lang::En,
                            true,
                            Some("user1".into()),
                            None,
                            None,
                        )
                    });
                    shell_entity = Some(shell.clone());
                    shell
                })
            })
            .unwrap();
        let shell_entity = shell_entity.unwrap();
        let note_id = note.note_id.clone();
        window
            .update(cx, |shell, window, cx| {
                shell.select_note(note_id, window, cx);
                if shell.vim_enabled {
                    shell.vim_enabled = false;
                    shell.vim.reset();
                }
            })
            .unwrap();
        cx.run_until_parked();
        cx.background_executor.allow_parking();
        let mut cx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let content = shell_entity.read_with(&cx, |shell, _| {
            shell.editing.as_ref().unwrap().content.clone()
        });

        // Caret on line 2, mid-line; the `/` + filter never enter the buffer.
        cx.update(|_, cx| {
            content.update(cx, |s, cx| s.set_selected_range(14..14, cx));
        });
        cx.simulate_keystrokes("/");
        cx.update(|_, cx| {
            assert!(
                shell_entity.read(cx).slash_panel.is_some(),
                "`/` opens the panel"
            );
            assert_eq!(content.read(cx).value(), "line one\nline two");
        });
        // Filter to the heading command, then run it.
        cx.simulate_keystrokes("h e a d enter");
        cx.update(|_, cx| {
            let shell = shell_entity.read(cx);
            assert!(shell.slash_panel.is_none(), "panel closes on execute");
            drop(shell);
            assert_eq!(
                content.read(cx).value(),
                "line one\nline # two",
                "command inserts at the caret without deleting before it"
            );
            let r = content.read(cx).selected_range();
            assert_eq!(r, 16..16, "caret after the inserted prefix");
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Execution-level coverage for the bugs the direct-execution rewrite
    /// fixed: dd leaves no blank line, yy+p pastes a line below, $ lands
    /// on the last char of the current line (not the next), visual
    /// motions extend the selection.
    #[gpui_kit::test]
    fn vim_exec_line_ops_dollar_visual(cx: &mut gpui_kit::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("leanote-vimexec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = leanote_core::Store::open(dir.clone(), "user1".to_string()).unwrap();
        let note = store
            .add_note(leanote_core::NewNote {
                title: "t".into(),
                content: "one\ntwo\nthree".into(),
                tags: vec![],
                notebook_title: "nb".into(),
                is_markdown: true,
            })
            .unwrap();

        cx.update(gpui_kit::init);
        let mut shell_entity: Option<gpui_kit::Entity<AppShell>> = None;
        let window = cx
            .update(|cx| {
                cx.open_window(gpui_kit::WindowOptions::default(), |window, cx| {
                    let shell = cx.new(|cx| {
                        AppShell::new(
                            window,
                            cx,
                            dir.clone(),
                            crate::i18n::Lang::En,
                            true,
                            Some("user1".into()),
                            None,
                            None,
                        )
                    });
                    shell_entity = Some(shell.clone());
                    shell
                })
            })
            .unwrap();
        let shell_entity = shell_entity.unwrap();
        let note_id = note.note_id.clone();
        window
            .update(cx, |shell, window, cx| {
                shell.select_note(note_id, window, cx);
            })
            .unwrap();
        cx.run_until_parked();
        cx.background_executor.allow_parking();
        let mut cx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let content = shell_entity.read_with(&cx, |shell, _| {
            shell.editing.as_ref().unwrap().content.clone()
        });

        // dd on the middle line eats its newline (no blank line left).
        let key = |s: &str| gpui_kit::Keystroke::parse(s).unwrap();
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("j"), window, cx); // to line 2
                shell.vim_key_down(&key("d"), window, cx);
                shell.vim_key_down(&key("d"), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "one\nthree",
                "dd must remove the whole line and its newline"
            );
        });

        // yy on line 1 then p pastes it below line 1.
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                // After dd the caret sits on "three"; move to line 1 so
                // yy grabs "one".
                shell.vim_key_down(&key("k"), window, cx);
                shell.vim_key_down(&key("y"), window, cx);
                shell.vim_key_down(&key("y"), window, cx);
                shell.vim_key_down(&key("p"), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "one\none\nthree",
                "yy then p must insert the line below"
            );
        });

        // $ stays on the current line's last char ("one" line: caret at
        // the 'e', offset 2 — not at the line-end boundary 3).
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("k"), window, cx); // up to line 1
                shell.vim_key_down(&key("$"), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).selected_range(),
                2..2,
                "$ must rest collapsed on the line's last char"
            );
        });

        // v + ll from the caret on "e" extends over the newline; yank
        // captures "e\no" (vim selects across the line break).
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("v"), window, cx);
                shell.vim_key_down(&key("l"), window, cx);
                shell.vim_key_down(&key("l"), window, cx);
                shell.vim_key_down(&key("y"), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "one\none\nthree",
                "visual yank must not change the text"
            );
            let shell = shell_entity.read(cx);
            assert_eq!(
                shell.vim.register.as_deref(),
                Some("e\no"),
                "visual yank captures the extended selection"
            );
        });

        // Empty-line block caret (ADR-0005): the selection covers the
        // `\n` itself — the kit paints a selected newline as a
        // one-space cell, which is the block caret there. Replacing the
        // buffer with a leading indented line + empty line.
        cx.update(|window, cx| {
            content.update(cx, |state, cx| state.replace_all("  ab\n\ncd", window, cx))
        });
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                // 0 → line start ("0"), then ^: col 2.
                shell.vim_key_down(&key("0"), window, cx);
                shell.vim_key_down(&key("^"), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).selected_range(),
                2..2,
                "^ must land on the first non-blank char of an indented line"
            );
            assert!(content.read(cx).block_cursor());
        });
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("j"), window, cx); // to the empty line
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).selected_range(),
                5..5,
                "empty-line caret is collapsed; the block is painted by the kit"
            );
            assert!(content.read(cx).block_cursor());
        });
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("x"), window, cx); // no-op on an empty line
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "  ab\n\ncd",
                "x on an empty line must not join lines"
            );
            assert_eq!(
                content.read(cx).selected_range(),
                5..5,
                "x on an empty line keeps the collapsed caret"
            );
            assert!(content.read(cx).block_cursor());
        });

        // D: delete to line end (caret col 2 → rest "").
        cx.update(|window, cx| {
            content.update(cx, |state, cx| {
                state.replace_all("para1\n\npara2 line\n", window, cx)
            })
        });
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("l"), window, cx); // caret on 'a' (offset 1)
                shell.vim_key_down(&key("l"), window, cx); // caret on 'r' (offset 2)
                shell.vim_key_down(&key("D"), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "pa\n\npara2 line\n",
                "D deletes from the caret to the line end"
            );
            assert_eq!(content.read(cx).selected_range(), 2..2);
        });

        // { / }: paragraph jumps between blank-line boundaries.
        cx.update(|window, cx| {
            content.update(cx, |state, cx| {
                state.replace_all("aa\nbb\n\ncc\ndd\n\nee", window, cx)
            });
        });
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("g"), window, cx); // g then g: doc start
                shell.vim_key_down(&key("g"), window, cx);
                shell.vim_key_down(&key("}"), window, cx); // → blank line (row 2)
                shell.vim_key_down(&key("}"), window, cx); // → blank line (row 5)
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            // "aa\nbb\n\ncc\ndd\n\nee": blank rows are 2 and 5.
            assert_eq!(
                content.read(cx).selected_range(),
                13..13,
                "}} lands on the second blank line's start"
            );
        });
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("{"), window, cx); // → blank row 2
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).selected_range(),
                6..6,
                "{{ returns to the previous blank line's start"
            );
        });

        // d}: operator + paragraph motion deletes to the boundary.
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("d"), window, cx);
                shell.vim_key_down(&key("}"), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "aa\nbb\n\nee",
                "d}} deletes caret..next blank line start"
            );
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Round 34: `V` linewise visual — select whole lines, j/k extend by
    /// line, `d` eats lines (no blank residue), `y`+`p` pastes lines.
    #[gpui_kit::test]
    fn vim_visual_line_selects_edits(cx: &mut gpui_kit::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("leanote-vline-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = leanote_core::Store::open(dir.clone(), "user1".to_string()).unwrap();
        let note = store
            .add_note(leanote_core::NewNote {
                title: "t".into(),
                content: "one\ntwo\nthree\nfour".into(),
                tags: vec![],
                notebook_title: "nb".into(),
                is_markdown: true,
            })
            .unwrap();

        cx.update(gpui_kit::init);
        let mut shell_entity: Option<gpui_kit::Entity<AppShell>> = None;
        let window = cx
            .update(|cx| {
                cx.open_window(gpui_kit::WindowOptions::default(), |window, cx| {
                    let shell = cx.new(|cx| {
                        AppShell::new(
                            window,
                            cx,
                            dir.clone(),
                            crate::i18n::Lang::En,
                            true,
                            Some("user1".into()),
                            None,
                            None,
                        )
                    });
                    shell_entity = Some(shell.clone());
                    shell
                })
            })
            .unwrap();
        let shell_entity = shell_entity.unwrap();
        let note_id = note.note_id.clone();
        window
            .update(cx, |shell, window, cx| {
                shell.select_note(note_id, window, cx);
            })
            .unwrap();
        cx.run_until_parked();
        cx.background_executor.allow_parking();
        let mut cx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let content = shell_entity.read_with(&cx, |shell, _| {
            shell.editing.as_ref().unwrap().content.clone()
        });
        let key = |s: &str| gpui_kit::Keystroke::parse(s).unwrap();

        // V on line 1 selects the whole line (0..4 incl. `\n`).
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("V"), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).selected_range(),
                0..4,
                "V must select the whole current line"
            );
        });

        // j extends by whole lines: rows 0..=1 → "one\ntwo\n" = 0..8.
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("j"), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).selected_range(),
                0..8,
                "V+j must extend by whole lines"
            );
        });

        // y yanks the two lines linewise; p pastes them below row 1.
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("y"), window, cx);
                shell.vim_key_down(&key("p"), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "one\ntwo\none\ntwo\nthree\nfour",
                "V yank+p must paste whole lines"
            );
        });

        // From row 2, jj lands on row 4 ("three"); V selects it, j extends
        // to row 5, d removes rows 4..=5 — no blank residue.
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.vim_key_down(&key("j"), window, cx);
                shell.vim_key_down(&key("j"), window, cx);
                shell.vim_key_down(&key("V"), window, cx);
                shell.vim_key_down(&key("j"), window, cx);
                shell.vim_key_down(&key("d"), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "one\ntwo\none\ntwo",
                "V d must remove whole lines cleanly"
            );
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue 21 需求4/5: Enter list continuation and pair closing,
    /// driven through the real dispatch chain (capture hooks in the
    /// rendered editor column).
    #[gpui_kit::test]
    fn markdown_input_enter_pairs(cx: &mut gpui_kit::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("leanote-mdtest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = leanote_core::Store::open(dir.clone(), "user1".to_string()).unwrap();
        let note = store
            .add_note(leanote_core::NewNote {
                title: "t".into(),
                content: "1. one\n- [ ] task".into(),
                tags: vec![],
                notebook_title: "nb".into(),
                is_markdown: true,
            })
            .unwrap();

        cx.update(gpui_kit::init);
        let mut shell_entity: Option<gpui_kit::Entity<AppShell>> = None;
        let window = cx
            .update(|cx| {
                cx.open_window(gpui_kit::WindowOptions::default(), |window, cx| {
                    let shell = cx.new(|cx| {
                        AppShell::new(
                            window,
                            cx,
                            dir.clone(),
                            crate::i18n::Lang::En,
                            true, // signed in: render the shell, not auth
                            Some("user1".into()),
                            None,
                            None,
                        )
                    });
                    shell_entity = Some(shell.clone());
                    shell
                })
            })
            .unwrap();
        let shell_entity = shell_entity.unwrap();
        let note_id = note.note_id.clone();
        window
            .update(cx, |shell, window, cx| {
                shell.select_note(note_id, window, cx);
                // prefs default has vim on; turn it off for this test so
                // insert assists are active from the first keystroke.
                if shell.vim_enabled {
                    shell.vim_enabled = false;
                    shell.vim.reset();
                }
            })
            .unwrap();
        cx.run_until_parked();
        cx.background_executor.allow_parking();
        let mut cx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let content = shell_entity.read_with(&cx, |shell, _| {
            shell.editing.as_ref().unwrap().content.clone()
        });

        // End of line 1 ("1. one") → Enter continues to "2. ".
        cx.update(|_, cx| {
            content.update(cx, |s, cx| s.set_selected_range(6..6, cx));
        });
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "1. one\n2. \n- [ ] task",
                "ordered list continues with the next number"
            );
        });
        // Empty item Enter drops the marker (ends the list).
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "1. one\n\n- [ ] task",
                "empty item enter clears the prefix"
            );
        });
        // Move to the end of the task line; Enter continues "- [ ]".
        cx.update(|_, cx| {
            content.update(cx, |s, cx| s.set_selected_range(18..18, cx));
        });
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "1. one\n\n- [ ] task\n- [ ] ",
                "task checkbox continues unchecked"
            );
        });
        // Pair closing: `[` inserts `[]` with the caret between.
        cx.simulate_keystrokes("[");
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "1. one\n\n- [ ] task\n- [ ] []",
                "open bracket closes itself"
            );
            let r = content.read(cx).selected_range();
            assert_eq!(r, r.start..r.start, "collapsed caret between the pair");
        });

        // Tab on a list line indents the line start by two spaces.
        cx.update(|_, cx| {
            content.update(cx, |s, cx| s.set_selected_range(0..0, cx));
        });
        cx.simulate_keystrokes("tab");
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).value(),
                "  1. one\n\n- [ ] task\n- [ ] []",
                "tab indents the list line start"
            );
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue 21 需求10: ⌘T toggles the full-window preview state.
    #[gpui_kit::test]
    fn full_preview_toggles(cx: &mut gpui_kit::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("leanote-fptest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A distinctive non-default left width so a redistributed layout is
        // detectable: the bug reset the pane split after a full preview.
        {
            let mut p = crate::prefs::Prefs::load(&dir);
            p.pane_widths.left = Some(333.);
            p.pane_widths.mid = Some(444.);
            p.save(&dir);
        }
        let store = leanote_core::Store::open(dir.clone(), "user1".to_string()).unwrap();
        let note = store
            .add_note(leanote_core::NewNote {
                title: "t".into(),
                content: "# hi".into(),
                tags: vec![],
                notebook_title: "nb".into(),
                is_markdown: true,
            })
            .unwrap();

        cx.update(gpui_kit::init);
        // Same binding the real app registers in main.rs.
        cx.update(|cx| {
            cx.bind_keys([gpui_kit::KeyBinding::new(
                "secondary-t",
                crate::search_overlay::ToggleFullPreview,
                None,
            )]);
        });
        let mut shell_entity: Option<gpui_kit::Entity<AppShell>> = None;
        let window = cx
            .update(|cx| {
                cx.open_window(gpui_kit::WindowOptions::default(), |window, cx| {
                    let shell = cx.new(|cx| {
                        AppShell::new(
                            window,
                            cx,
                            dir.clone(),
                            crate::i18n::Lang::En,
                            true,
                            Some("user1".into()),
                            None,
                            None,
                        )
                    });
                    shell_entity = Some(shell.clone());
                    shell
                })
            })
            .unwrap();
        let shell_entity = shell_entity.unwrap();
        let note_id = note.note_id.clone();
        window
            .update(cx, |shell, window, cx| {
                shell.select_note(note_id, window, cx);
            })
            .unwrap();
        cx.run_until_parked();
        cx.background_executor.allow_parking();
        let mut cx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        // The nav pane's width must survive a full-preview round trip: the
        // bug was the group collapsing to one panel and redistributing.
        let nav_before = cx
            .debug_bounds("nav-pane")
            .expect("nav pane rendered")
            .size
            .width;

        cx.simulate_keystrokes("secondary-t");
        cx.update(|_, cx| {
            assert!(
                shell_entity.read(cx).full_preview,
                "⌘T must open the full-window preview"
            );
        });

        // No manual redraw: dispatch_key_event redraws on notify-dirty, and
        // a manual draw+clear here would wipe the dispatch tree's listeners.
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| {
            assert!(
                !shell_entity.read(cx).full_preview,
                "ESC must leave the full-window preview"
            );
        });
        // The kit state kept all three panel slots through the preview, so
        // the divider returns to its dragged position, not a redistributed
        // equal split.
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let nav_after = cx
            .debug_bounds("nav-pane")
            .expect("nav pane re-rendered")
            .size
            .width;
        assert_eq!(nav_before, nav_after, "nav pane width must survive ⌘T");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Round 32: the ⌘T preview keeps generous side margins and a capped,
    /// centered column even on a very wide window — the text must not run
    /// edge-to-edge.
    #[gpui_kit::test]
    fn full_preview_keeps_reading_margins(cx: &mut gpui_kit::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("leanote-fpmargins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        {
            let store = leanote_core::Store::open(dir.clone(), "user1".to_string()).unwrap();
            store
                .add_note(leanote_core::NewNote {
                    title: "t".into(),
                    content: "# hi".into(),
                    tags: vec![],
                    notebook_title: "nb".into(),
                    is_markdown: true,
                })
                .unwrap();
        }
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            cx.bind_keys([gpui_kit::KeyBinding::new(
                "secondary-t",
                crate::search_overlay::ToggleFullPreview,
                None,
            )]);
        });
        let mut shell_entity: Option<gpui_kit::Entity<AppShell>> = None;
        let window = cx
            .update(|cx| {
                cx.open_window(gpui_kit::WindowOptions::default(), |window, cx| {
                    let shell = cx.new(|cx| {
                        AppShell::new(
                            window,
                            cx,
                            dir.clone(),
                            crate::i18n::Lang::En,
                            true,
                            Some("user1".into()),
                            None,
                            None,
                        )
                    });
                    shell_entity = Some(shell.clone());
                    shell
                })
            })
            .unwrap();
        window
            .update(cx, |shell, window, cx| {
                let id = shell.visible_notes()[0].note_id.clone();
                shell.select_note(id, window, cx);
            })
            .unwrap();
        cx.run_until_parked();
        cx.background_executor.allow_parking();
        let mut cx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
        // A wide "fullscreen" window: 1920 logical px.
        cx.simulate_resize(gpui_kit::size(gpui_kit::px(1920.), gpui_kit::px(1080.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_keystrokes("secondary-t");
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let frame = cx.debug_bounds("full-preview").expect("preview rendered");
        let w = frame.size.width;
        assert!(
            w <= gpui_kit::px(1120.),
            "preview column must be capped, got {w:?}"
        );
        let window_w = gpui_kit::px(1920.);
        let left = frame.origin.x;
        let right = window_w - (frame.origin.x + w);
        assert!(
            left >= gpui_kit::px(48.) && right >= gpui_kit::px(48.),
            "side margins must stay generous, left={left:?} right={right:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Ticket 11 follow-up: New Notebook opens a modal dialog; Enter
    /// confirms; the new notebook lands FIRST in its sibling group.
    #[gpui_kit::test]
    fn notebook_dialog_creates_prepended(cx: &mut gpui_kit::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("leanote-nbdialog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        {
            let store = leanote_core::Store::open(dir.clone(), "user1".to_string()).unwrap();
            let _ = store.create_notebook("Existing", "").unwrap();
        }
        cx.update(gpui_kit::init);
        let (window, shell_entity) = cx
            .update(|cx| {
                gpui_kit::open_window(gpui_kit::WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| {
                        AppShell::new(
                            window,
                            cx,
                            dir.clone(),
                            crate::i18n::Lang::En,
                            true,
                            Some("user1".into()),
                            None,
                            None,
                        )
                    })
                })
            })
            .unwrap();
        cx.run_until_parked();
        cx.background_executor.allow_parking();
        let mut cx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));

        // open the dialog (root-level "New Notebook" action)
        cx.update(|window, cx| {
            shell_entity.update(cx, |shell, cx| {
                shell.begin_new_notebook(String::new(), window, cx)
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        // type a name and confirm with Enter
        cx.simulate_input("Fresh");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        let first = shell_entity
            .update(&mut cx, |shell, _cx| {
                shell
                    .store()
                    .and_then(|s| s.notebooks().ok())
                    .and_then(|nbs| nbs.first().cloned())
            })
            .unwrap();
        assert_eq!(first.title, "Fresh", "dialog Enter creates the notebook");
        assert_eq!(first.seq, -1, "new notebook prepends (min sibling Seq - 1)");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fix 2: a local edit flips `pending_sync`; invalidate_lists recomputes
    /// it from IsDirty rows; logout resets it.
    #[gpui_kit::test]
    fn pending_sync_tracks_dirty_rows(cx: &mut gpui_kit::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("leanote-pending-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        {
            let store = leanote_core::Store::open(dir.clone(), "user1".to_string()).unwrap();
            let _ = store
                .add_note(leanote_core::NewNote {
                    title: "dirty".into(),
                    content: "x".into(),
                    tags: vec![],
                    notebook_title: "nb".into(),
                    is_markdown: true,
                })
                .unwrap();
            // add_note writes IsDirty=1 (local-only row, never pushed).
        }
        cx.update(gpui_kit::init);
        let (window, shell_entity) = cx
            .update(|cx| {
                gpui_kit::open_window(gpui_kit::WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| {
                        AppShell::new(
                            window,
                            cx,
                            dir.clone(),
                            crate::i18n::Lang::En,
                            true,
                            Some("user1".into()),
                            None,
                            None,
                        )
                    })
                })
            })
            .unwrap();
        cx.run_until_parked();
        let _ = window;

        shell_entity.update(cx, |shell, _cx| {
            shell.invalidate_lists();
            assert!(shell.pending_sync, "IsDirty=1 rows must set pending_sync");
            // Label follows: pending wins over the unset sync_status.
            assert!(
                shell
                    .sync_status_label()
                    .to_string()
                    .contains("waiting to sync"),
                "label must show the pending hint, got {}",
                shell.sync_status_label()
            );
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fix 1: ⌘S / ⌘N dispatch app-scope actions on the root (same
    /// bindings main.rs registers). ⌘N must reach new_note.
    #[gpui_kit::test]
    fn cmd_n_creates_note(cx: &mut gpui_kit::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("leanote-cmdn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        {
            let store = leanote_core::Store::open(dir.clone(), "user1".to_string()).unwrap();
            let _ = store.create_notebook("nb", "").unwrap();
        }
        cx.update(gpui_kit::init);
        // Same bindings the real app registers in main.rs.
        cx.update(|cx| {
            cx.bind_keys([
                gpui_kit::KeyBinding::new("secondary-n", crate::search_overlay::QuickNewNote, None),
                gpui_kit::KeyBinding::new("secondary-s", crate::search_overlay::QuickSyncNow, None),
                // The old "cmd-" spelling parsed to key="" and never fired.
                gpui_kit::KeyBinding::new("secondary-,", crate::search_overlay::OpenSettings, None),
            ]);
        });
        let (window, shell_entity) = cx
            .update(|cx| {
                gpui_kit::open_window(gpui_kit::WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| {
                        AppShell::new(
                            window,
                            cx,
                            dir.clone(),
                            crate::i18n::Lang::En,
                            true,
                            Some("user1".into()),
                            None,
                            None,
                        )
                    })
                })
            })
            .unwrap();
        cx.run_until_parked();
        cx.background_executor.allow_parking();
        let mut cx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let before = shell_entity
            .update(&mut cx, |shell, _cx| shell.visible_notes().clone())
            .len();
        // "secondary" is the portable spelling: cmd on macOS, ctrl on Linux.
        cx.simulate_keystrokes("secondary-n");
        cx.run_until_parked();
        let after = shell_entity
            .update(&mut cx, |shell, _cx| shell.visible_notes().clone())
            .len();
        assert_eq!(before + 1, after, "⌘N must create a note");

        // ⌘, opens settings — a dialog on the window.
        cx.simulate_keystrokes("secondary-,");
        cx.run_until_parked();
        assert!(
            cx.update(|window, cx| window.has_active_dialog(cx)),
            "⌘, must open the settings dialog"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// Window-state glue used by main.rs.

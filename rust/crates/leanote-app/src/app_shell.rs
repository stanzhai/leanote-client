//! The main window: login form (right pane) → full-sync progress → Wake
//! three-pane shell (ADR-0003). Ticket 09: selectable nav + filtered list +
//! read-only Markdown reader, star toggle, trash actions.

use crate::browse::{self, NavNode};
use crate::conflict_bar;
use crate::i18n::{self, Lang};
use crate::theme::{self, Material};
use gpui_kit::assets::IconName as AssetIcon;
use gpui_kit::base::Scrollbar;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{
    Backspace, Delete, Editor, EditorState, Enter, Escape, Input, InputEvent, InputState,
};
use gpui_kit::component::label::Label;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::resizable_panel;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, IconName, InteractiveElementExt, Sizable, WindowExt as _,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    px, AnyElement, KeyDownEvent, MouseButton, SharedString, Task, UniformListScrollHandle, Window,
    WindowControlArea,
};
use leanote_core::sync::{Api, NoProgress, SyncProgress, SyncStats};
use std::sync::{Arc, Mutex};

/// Default pane widths (ADR-0003). Issue 18: user-resizable, persisted;
/// these are the first-run and reset values.
const LEFT_W: f32 = 224.;
const MID_W: f32 = 336.;
/// Issue 18: drag ranges for the left/middle panes.
const LEFT_RANGE: (f32, f32) = (180., 400.);
const MID_RANGE: (f32, f32) = (240., 600.);
/// Issue 18: uniform note-row height — 2-line title (19px each) + meta line
/// (16px) + py_2 padding (16px) + gap_1 (4px).
const NOTE_ROW_H: f32 = 96.;
/// macOS transparent titlebar height, same material as the sidebar.
const TITLEBAR_H: f32 = 44.;

pub enum Phase {
    Login,
    SigningIn,
    /// Progress lines from full_sync, newest last.
    Syncing {
        lines: Vec<SharedString>,
    },
    /// Sync done → three-pane shell.
    Shell,
    /// Login or sync failed → back to login with an error line.
    Failed {
        error: SharedString,
    },
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
    /// Create a notebook under this parent ("" = root).
    NewNotebook {
        parent: String,
        name: gpui_kit::Entity<InputState>,
    },
    /// Rename this notebook.
    RenameNotebook {
        id: leanote_core::Id,
        name: gpui_kit::Entity<InputState>,
    },
    /// Move this notebook: picking the target parent.
    MoveNotebook { id: leanote_core::Id },
    /// Issue 18: move this note to another notebook — the middle list
    /// swaps to a target picker.
    MoveNote { id: leanote_core::Id },
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
    /// Issue 19: focus mode — hide left + middle panes (⇧⌘F toggles).
    focus_mode: bool,
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
    editing: Option<EditState>,
    /// History popover open (only meaningful while editing).
    history_open: bool,
    /// Which history row is previewed (None = list view).
    history_preview: Option<leanote_core::Id>,
    // --- Issue 19 middle-column search box ---
    /// Filter input entity for the note list (None until first render).
    list_filter: Option<gpui_kit::Entity<InputState>>,
    /// Committed filter text (title + desc substring match).
    list_query: String,
    /// Debounced filter task.
    list_filter_task: Option<Task<()>>,
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
}

/// Right-pane edit mode (issues 10+17): title + source editor with syntax
/// highlighting. Buffer lives in the kit's EditorState so IME composition is
/// handled entirely by the kit; `saved_content` is the last persisted text so
/// an unchanged editor never writes to the store (issue 17 Q2/Q8).
struct EditState {
    note_id: leanote_core::Id,
    title: gpui_kit::Entity<InputState>,
    content: gpui_kit::Entity<EditorState>,
    /// Last persisted content — auto-save compares against it.
    saved_content: String,
    /// True when the buffer has changed since last persist (drives autosave).
    dirty: bool,
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
        // Signed-in account's user id (None → still "user1" placeholder).
        user_id: Option<String>,
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
            let mut s = InputState::new(window, cx)
                .placeholder("••••••••")
                .masked(true);
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
            vim_enabled: prefs.vim_default,
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
            focus_mode: false,
            export_status: None,
            search: crate::search_overlay::SearchOverlay::new(window, cx),
            search_rows: Vec::new(),
            search_query: String::new(),
            search_task: None,
            list_filter: None,
            list_query: String::new(),
            list_filter_task: None,
            converted_note: None,
            nav_edit: None,
            editing: None,
            history_open: false,
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
                    Ok((user_id, _stats)) => {
                        shell.user_id = user_id;
                        shell.phase = Phase::Shell;
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
        match &self.sync_status {
            None => t(i18n::Key::NotSynced).to_string().into(),
            Some(Ok(_)) => t(i18n::Key::SyncDone).to_string().into(),
            Some(Err(e)) => format!("{}: {}", t(i18n::Key::SyncFailed), e).into(),
        }
    }

    /// Review item 2: logout — server-side signout (best effort), local
    /// account deactivated, back to the login screen.
    fn logout(&mut self, cx: &mut gpui_kit::Context<Self>) {
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
        self.phase = Phase::Login;
        self._task = None;
        self.invalidate_lists();
        cx.notify();
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
        leanote_core::Store::open(self.data_dir.clone(), self.user_id.clone()).ok()
    }

    /// Issue 17: invalidate list caches — call after ANY mutation that can
    /// change notes/notebooks/tags (select, sync, save, nav edit, ...).
    fn invalidate_lists(&mut self) {
        self.cached_tag_nodes = None;
        self.cached_notebook_names = None;
        self.cached_visible_notes = None;
    }

    /// Notes for the current nav selection, in list order (cached).
    /// Issue 19: meta projection — no content blob in the list path.
    fn visible_notes(&mut self) -> &Vec<leanote_core::NoteMeta> {
        if self.cached_visible_notes.is_none() {
            let notes = self
                .store()
                .map(|store| {
                    let res = match &self.nav {
                        NavSelection::Recent => store.list_notes_meta_recent(20),
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

    /// Nav rows for the Tags group (cached): tag name → live note count via
    /// list_notes_by_tag (the tags table's Count stays 1 after sync).
    fn tag_nav_nodes(&mut self) -> &Vec<NavNode> {
        if self.cached_tag_nodes.is_none() {
            let nodes = self
                .store()
                .map(|store| {
                    let Ok(tags) = store.tags() else {
                        return Vec::new();
                    };
                    // Issue 19: one aggregate query instead of N+1.
                    let counts: std::collections::HashMap<String, i64> = store
                        .tag_note_counts()
                        .unwrap_or_default()
                        .into_iter()
                        .collect();
                    tags.into_iter()
                        .filter(|t| !t.is_deleted)
                        .map(|t| NavNode {
                            count: counts.get(&t.tag).copied().unwrap_or(0),
                            id: t.tag.clone(),
                            title: format!("# {}", t.tag),
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
        // Issue 17 Q6: local recents (prefs).
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        p.push_recent(&id.to_string());
        // Issue 19: remember the last open note.
        p.nav.last_note = Some(id.to_string());
        p.save(&dir);

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
        let title = cx.new(|cx| InputState::new(window, cx).default_value(note.title.clone()));
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
        // Body owns focus: vim grammar and typing land in the content, not
        // the title input.
        content.update(cx, |state, cx| state.focus(window, cx));
        self.editing = Some(EditState {
            note_id: id,
            title,
            content,
            saved_content: note.content,
            dirty: false,
            preview: false,
            saved_flash: false,
        });
        // Issue 17 Q12: every session starts in normal mode.
        self.vim = crate::vim::VimState::new();
        // ADR-0005: normal mode rests on a block caret, so the very
        // first frame already shows one.
        if self.vim_enabled {
            let editor = self.editing.as_ref().unwrap().content.clone();
            editor.update(cx, |state, cx| {
                state.set_selected_range(0..1, cx);
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
        if changed {
            if let Some(store) = self.store() {
                let _ = store.save_note_with_history(
                    &id,
                    leanote_core::NoteUpdate {
                        title: Some(title),
                        content: Some(content.clone()),
                        ..Default::default()
                    },
                );
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
        p.appearance = match ix {
            0 => crate::prefs::AppearancePref::Follow,
            1 => crate::prefs::AppearancePref::Light,
            _ => crate::prefs::AppearancePref::Dark,
        };
        p.save(&dir);
        // Panes read window.appearance(); drive it from the theme choice so
        // Light/Dark repaint the whole app (issue 19 #26).
        cx.set_window_appearance(match p.appearance {
            crate::prefs::AppearancePref::Follow => None,
            crate::prefs::AppearancePref::Light => Some(gpui_kit::WindowAppearance::Light),
            crate::prefs::AppearancePref::Dark => Some(gpui_kit::WindowAppearance::Dark),
        });
        match p.appearance {
            crate::prefs::AppearancePref::Follow => {
                gpui_kit::component::Theme::sync_system_appearance(None, cx)
            }
            crate::prefs::AppearancePref::Light => {
                gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Light, None, cx);
            }
            crate::prefs::AppearancePref::Dark => {
                gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
            }
        }
        let palette = p.editor_palette.clone();
        gpui_kit::component::Theme::update(cx, move |theme| {
            crate::apply_wake_theme(theme, &palette)
        });
        cx.notify();
    }

    /// Issue 17 Q7: editor palette pref → disk + live replay.
    fn set_editor_palette(&mut self, ix: usize, cx: &mut gpui_kit::Context<Self>) {
        let dir = self.data_dir.clone();
        let mut p = crate::prefs::Prefs::load(&dir);
        p.editor_palette = match ix {
            1 => "warm".into(),
            2 => "solarized".into(),
            _ => String::new(),
        };
        p.save(&dir);
        let palette = p.editor_palette.clone();
        gpui_kit::component::Theme::update(cx, move |theme| {
            crate::apply_wake_theme(theme, &palette)
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

    /// Issue 19: middle-column search box debounce (150ms, newest wins).
    fn queue_list_filter(&mut self, query: String, cx: &mut gpui_kit::Context<Self>) {
        self.list_query = query;
        self.list_filter_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(150))
                .await;
            let _ = this.update(cx, |_shell, cx| cx.notify());
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

    fn empty_trash(&mut self, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.empty_trash();
        }
        self.selected = None;
        self.invalidate_lists();
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
        let Some(n) = self.selected_note() else {
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

    /// Selected note → PDF via the server's exportPdf endpoint (src/api.js
    /// authority). Any failure — not synced, no account, transport, server
    /// error — falls back to a local HTML export with the reason shown.
    fn export_selected_pdf(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let Some(n) = self.selected_note() else {
            return;
        };
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
                    std::fs::write(path.with_extension("html"), crate::export::note_to_html(&n))
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
        let user = self.user_id.clone();
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
        self.invalidate_lists();
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
            // MoveNotebook / MoveNote resolve directly in their *_to fns.
            Some(NavEdit::MoveNotebook { .. }) | Some(NavEdit::MoveNote { .. }) | None => {}
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
        cx.notify();
    }

    fn delete_notebook_nav(&mut self, id: leanote_core::Id, cx: &mut gpui_kit::Context<Self>) {
        if let Some(store) = self.store() {
            let _ = store.delete_notebook(&id);
        }
        if self.nav == NavSelection::Notebook(id.to_string()) {
            self.nav = NavSelection::Recent;
            self.selected = None;
        }
        self.invalidate_lists();
        cx.notify();
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
/// uniform_list is an equal-height model. Title wraps up to 2 lines then
/// ellipsizes; meta is one truncated line.
/// `shell` (weak) routes the click to select_note — uniform_list closures
/// only get `&mut App`, not the view context.
fn note_list_row(
    shell: &gpui_kit::WeakEntity<AppShell>,
    n: &leanote_core::NoteMeta,
    meta: SharedString,
    desc: SharedString,
    is_sel: bool,
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
        .h(px(NOTE_ROW_H))
        .w_full()
        .px_2()
        .py_2()
        .gap_1()
        .justify_between()
        .rounded(px(theme::RADIUS_SELECTION))
        .when(is_sel, |el| el.bg(theme::accent()))
        .on_click(move |_ev, window, cx| {
            let _ = shell.update(cx, |this: &mut AppShell, cx| {
                this.select_note(id.clone(), window, cx)
            });
        })
        .context_menu({
            let shell = shell_menu;
            let id = id_menu;
            let lang = crate::i18n::system_lang();
            move |menu, _window, _cx| note_row_menu(menu, &shell, &id, lang)
        })
        .child(
            // Title block: 2 wrapped lines max, then ellipsis. The star
            // rides in the corner without stealing title width.
            gpui_kit::base::h_flex()
                .gap_1()
                .child(
                    gpui_kit::div().flex_1().min_w_0().line_clamp(2).child(
                        Label::new(n.title.clone())
                            .text_size(px(14.))
                            .line_height(px(19.))
                            .text_color(row_fg),
                    ),
                )
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
                .child(small_label(meta, row_muted)),
        )
        .child(
            // Issue 19: content excerpt — single truncated line.
            gpui_kit::div()
                .truncate()
                .child(small_label(desc, row_muted)),
        )
}

/// Issue 18: right-click menu on a note row — trash (live notes) or
/// restore/delete-forever (trash view), plus move-to-notebook.
fn note_row_menu(
    menu: gpui_kit::component::menu::PopupMenu,
    shell: &gpui_kit::WeakEntity<AppShell>,
    id: &leanote_core::Id,
    lang: Lang,
) -> gpui_kit::component::menu::PopupMenu {
    let shell_mv = shell.clone();
    let shell_del = shell.clone();
    let id_mv = id.clone();
    let id_del = id.clone();
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
    .item(
        PopupMenuItem::new(i18n::t(lang, i18n::Key::DeleteNote)).on_click(
            move |_ev, _window, cx| {
                let _ = shell_del.update(cx, |this, cx| this.delete_note(id_del.clone(), cx));
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
            .on_action(cx.listener(
                |this, _: &crate::search_overlay::ToggleFocusMode, _window, cx| {
                    // Issue 19: hide left + middle panes for pure reading.
                    this.focus_mode = !this.focus_mode;
                    cx.notify();
                },
            ))
            .on_action(cx.listener(
                |this, _: &crate::search_overlay::ToggleSearch, window, cx| {
                    // Any phase: the dialog layer is window-level.
                    this.toggle_search(window, cx);
                },
            ))
            .on_action(cx.listener(
                |this, _: &crate::search_overlay::QuickNewNote, window, cx| {
                    window.close_dialog(cx);
                    this.new_note(window, cx);
                },
            ))
            .on_action(cx.listener(
                |this, _: &crate::search_overlay::QuickExportBackup, window, cx| {
                    window.close_dialog(cx);
                    this.export_backup(cx);
                },
            ))
            .on_action(cx.listener(
                |this, _: &crate::search_overlay::QuickToggleVim, window, cx| {
                    window.close_dialog(cx);
                    this.vim_enabled = !this.vim_enabled;
                    // Off mid-session: drop mode/pending/register so the
                    // next enable starts clean (issue 17 Q12).
                    if !this.vim_enabled {
                        this.vim.reset();
                    }
                    this.save_vim_pref(cx);
                    cx.notify();
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
        let widths = crate::prefs::Prefs::load(&self.data_dir).pane_widths;
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
                            .child(gpui_kit::base::v_flex().h_full().w_full().bg(base_bg)),
                    )
                    .child(
                        resizable_panel()
                            .size(px(widths.mid.unwrap_or(MID_W)))
                            .size_range(px(MID_RANGE.0)..px(MID_RANGE.1))
                            .child(gpui_kit::base::v_flex().h_full().w_full().bg(list_bg)),
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
            Phase::Syncing { lines } => {
                body = body
                    .child(
                        resizable_panel()
                            .size(px(widths.left.unwrap_or(LEFT_W)))
                            .size_range(px(LEFT_RANGE.0)..px(LEFT_RANGE.1))
                            .child(gpui_kit::base::v_flex().h_full().w_full().bg(base_bg)),
                    )
                    .child(
                        resizable_panel()
                            .size(px(widths.mid.unwrap_or(MID_W)))
                            .size_range(px(MID_RANGE.0)..px(MID_RANGE.1))
                            .child(gpui_kit::base::v_flex().h_full().w_full().bg(list_bg)),
                    )
                    .child(resizable_panel().child(render_syncing(dark, lines, &t)));
            }
            Phase::Shell { .. } => {
                // Issue 19: two nesting levels — outer [left | rest] keeps
                // the nav width, inner [list | reader] resizes the split
                // between the middle and right panes.
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
    Label::new(text).text_size(px(13.)).text_color(color)
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
    title: &str,
    fg: gpui_kit::Hsla,
    window: &mut Window,
    cx: &mut gpui_kit::Context<AppShell>,
) -> impl IntoElement + use<> {
    let state = window.use_state(cx, |_, _| DragHeaderState { should_move: false });
    let is_macos = cfg!(target_os = "macos");
    gpui_kit::div()
        .id("nav-drag-header")
        .flex()
        .flex_none()
        .items_center()
        .h(px(TITLEBAR_H))
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
        .child(Label::new(title).text_size(px(16.)).text_color(fg))
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
                        .on_click(
                            cx.listener(|this, _ev, window, cx| this.toggle_search(window, cx)),
                        ),
                ),
        )
}

/// Issue 17: transparent 28px strip at the top of the right pane — drag
/// region only, no content.
fn reader_drag_strip(
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
        .h(px(28.))
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
        .text_size(px(22.))
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

    let register = gpui_kit::component::link::Link::new("register")
        .href("https://leanote.com/register")
        .child(small_label(t(i18n::Key::Register), muted))
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
            Label::new(format!(
                "{} / {}: {}",
                t(i18n::Key::LoginFailed),
                t(i18n::Key::SyncFailed),
                err
            ))
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

fn render_syncing(
    dark: bool,
    lines: &[SharedString],
    t: &dyn Fn(i18n::Key) -> &'static str,
) -> impl IntoElement {
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
                        .child(
                            Label::new(t(i18n::Key::SyncingTitle))
                                .text_size(px(22.))
                                .text_color(theme::foreground(dark)),
                        ),
                )
                .child(small_label(
                    if lines.is_empty() {
                        t(i18n::Key::SyncingSubtitle)
                    } else {
                        t(i18n::Key::SyncDone)
                    },
                    muted,
                ))
                .child(gpui_kit::base::v_flex().gap_1().children(rows)),
        )
        .into_element()
}
/// One clickable nav row; `id` doubles as the element id.
fn nav_row(
    shell: &gpui_kit::WeakEntity<AppShell>,
    dark: bool,
    node: &NavNode,
    kind: &str,
    icon: Option<IconName>,
    selected: bool,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let fg = theme::foreground(dark);
    let sel_fg = theme::accent_foreground();
    // Issue 19: indent via padding so the pill spans the full row width and
    // the count sits flush right.
    let indent = px(12. * node.depth as f32);
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
        .pr_3()
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
            .text_size(px(14.))
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
                match group {
                    Group::Notebooks => {
                        this.notebooks_open = !this.notebooks_open;
                        p.nav.notebooks_open = this.notebooks_open;
                    }
                    Group::Tags => {
                        this.tags_open = !this.tags_open;
                        p.nav.tags_open = this.tags_open;
                    }
                }
                p.save(&dir);
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

        let all = NavNode {
            id: "all".into(),
            title: t(i18n::Key::NavRecent).into(),
            count: 0,
            depth: 0,
        };
        let mut all_count = 0i64;
        if let Some(store) = self.store() {
            all_count = store
                .list_notes_filtered(None, false)
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
            Some(IconName::BookOpen),
            self.nav == NavSelection::Recent,
        );
        let starred_row = nav_row(
            &shell,
            dark,
            &starred,
            "starred",
            Some(IconName::Star),
            self.nav == NavSelection::Starred,
        );
        let trash_row = nav_row(
            &shell,
            dark,
            &trash,
            "trash",
            Some(IconName::Delete),
            self.nav == NavSelection::Trash,
        );

        let mut nav = gpui_kit::base::v_flex()
            .w_full()
            .h_full()
            .bg(theme::material(Material::Base, dark))
            // Issue 17: 44px header below is the window-drag region (see
            // the header child below); no global titlebar strip anymore.
            .px_3()
            .gap_1()
            .child(drag_header(t(i18n::Key::AppName), fg, window, cx))
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
            // Ticket 11: inline editor row when creating/renaming.
            if let Some(NavEdit::NewNotebook { parent, .. }) = &self.nav_edit {
                // render at the parent's nesting depth; root edit on top
                let depth = if parent.is_empty() {
                    0
                } else {
                    notebook_nodes
                        .iter()
                        .find(|n| n.id == *parent)
                        .map(|n| n.depth + 1)
                        .unwrap_or(0)
                };
                let _ = depth;
                // row itself is drawn in the loop below via nav_edit_row
            }
            let edit_row = self.nav_edit_row(dark, t, "notebook", window, cx);
            let n = notebook_nodes.len() + usize::from(edit_row.is_some());
            let edit_row = std::rc::Rc::new(std::cell::RefCell::new(edit_row));
            let nav_sel = self.nav.clone();
            let shell_ul = shell.clone();
            let scroll = self.notebooks_scroll.clone();
            nav = nav.child(
                gpui_kit::div()
                    .id("notebooks-vlist")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        gpui_kit::uniform_list("notebooks-ul", n, move |range, _window, _cx| {
                            // Rows are rebuilt per call: measure_item fires
                            // twice per frame (layout + prepaint), so a
                            // take-once buffer would leave later renders empty.
                            range
                                .map(|ix| {
                                    let Some(node) = notebook_nodes.get(ix) else {
                                        return edit_row
                                            .borrow_mut()
                                            .take()
                                            .unwrap_or_else(|| gpui_kit::div().into_any_element());
                                    };
                                    let is_sel = nav_sel == NavSelection::Notebook(node.id.clone());
                                    nav_row(
                                        &shell_ul,
                                        dark,
                                        node,
                                        "notebook",
                                        Some(IconName::FolderClosed),
                                        is_sel,
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
                                                .on_click(move |_ev, _window, cx| {
                                                    let _ = shell_del.update(cx, |this, cx| {
                                                        this.delete_notebook_nav(id_del.clone(), cx)
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
                            .w(px(12.))
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
                    // Notebooks own the leftover height; tags take natural
                    // height up to a cap and scroll inside.
                    .max_h(px(300.))
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
                            .w(px(12.))
                            .child(
                                Scrollbar::vertical(&scroll)
                                    .viewport_from_layout()
                                    .max_fps(60),
                            ),
                    ),
            );
        }

        // Issue 19: trash lives after the tags group, before the footer.
        nav = nav.child(trash_row);
        // Push the footer to the bottom when both groups are collapsed.
        nav = nav.child(gpui_kit::div().flex_1().min_h_0());

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
                                .text_size(px(14.)),
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
            let (icon, title) = match &self.sync_status {
                Some(Ok(_)) => (AssetIcon::CircleCheck, self.sync_status_label()),
                Some(Err(_)) => (AssetIcon::TriangleAlert, self.sync_status_label()),
                None => (AssetIcon::Cloud, self.sync_status_label()),
            };
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
                                                .on_click(move |_ev, window, cx| {
                                                    let shell = shell.clone();
                                                    window.open_dialog(cx, move |d, _w, _cx| {
                                                        d.w(px(360.)).child(
                                                            Alert::warning(
                                                                "logout-confirm",
                                                                i18n::t(
                                                                    lang,
                                                                    i18n::Key::LogoutConfirmBody,
                                                                ),
                                                            )
                                                            .title(i18n::t(
                                                                lang,
                                                                i18n::Key::LogoutConfirmTitle,
                                                            ))
                                                            .on_close({
                                                                let shell = shell.clone();
                                                                move |_e, window, cx| {
                                                                    let _ = shell
                                                                        .update(cx, |this, cx| {
                                                                            this.logout(cx)
                                                                        });
                                                                    window.close_dialog(cx);
                                                                }
                                                            }),
                                                        )
                                                    });
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
                .on_click(cx.listener(|this, _ev, window, cx| this.open_settings(window, cx))),
        );
        nav = nav.child(gpui_kit::base::v_flex().flex_1()).child(footer);

        nav
    }

    /// Issue 17 Q7: three-button segmented control (label trio + active index).
    /// `on_pick` receives the picked index; the dialog rebuild re-reads prefs,
    /// so the highlight stays in sync.
    fn segmented(
        id: &'static str,
        labels: [SharedString; 3],
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

    /// Issue 17 Q7: centered settings dialog — appearance, editor palette,
    /// language, vim default, account + logout.
    fn open_settings(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        let shell = cx.entity();
        let lang = self.lang;
        let t = move |k: i18n::Key| i18n::t(lang, k);
        window.open_dialog(cx, move |dialog, window, cx| {
            let shell_c = shell.clone();
            let t = t;
            let prefs = crate::prefs::Prefs::load(&shell.read(cx).data_dir);
            let acct = shell
                .read(cx)
                .store()
                .and_then(|s| s.active_account().ok().flatten());
            let dark = shell.read(cx).dark(window);
            let muted = theme::muted_foreground(dark);

            // Row builders.
            let row = |label: gpui_kit::SharedString, control: gpui_kit::AnyElement| {
                gpui_kit::base::h_flex()
                    .justify_between()
                    .gap_3()
                    .py_1()
                    .child(Label::new(label).text_size(px(13.)))
                    .child(control)
            };

            let content = gpui_kit::base::v_flex()
                .gap_2()
                .px_1()
                .py_1()
                .w_full()
                // Appearance
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
                // Editor palette
                .child(row(
                    t(i18n::Key::EditorPalette).into(),
                    Self::segmented(
                        "set-palette",
                        [
                            t(i18n::Key::PaletteDefault).into(),
                            t(i18n::Key::PaletteWarm).into(),
                            t(i18n::Key::PaletteSolarized).into(),
                        ],
                        match prefs.editor_palette.as_str() {
                            "" => 0,
                            "warm" => 1,
                            _ => 2,
                        },
                        {
                            let shell = shell_c.clone();
                            move |ix, _window, cx| {
                                shell.update(cx, |this, cx| {
                                    this.set_editor_palette(ix, cx);
                                });
                            }
                        },
                    ),
                ))
                // Accent color
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
                // Account + logout
                .when_some(acct, |el, acct| {
                    el.child(
                        gpui_kit::base::v_flex()
                            .gap_1()
                            .pt_2()
                            .mt_2()
                            .border_t_1()
                            .border_color(theme::border(dark))
                            .child(small_label(
                                format!("{}: {}", t(i18n::Key::AccountInfo), acct.email),
                                muted,
                            ))
                            .child(small_label(
                                format!("{}: {}", t(i18n::Key::ServerLabel), acct.host),
                                muted,
                            ))
                            .child(
                                Button::new("settings-logout")
                                    .label(t(i18n::Key::Logout).to_owned())
                                    .outline()
                                    .on_click({
                                        let shell = shell_c.clone();
                                        move |_ev, window, cx| {
                                            let shell = shell.clone();
                                            window.open_dialog(cx, move |d, _w, _cx| {
                                                d.w(px(360.)).child(
                                                    Alert::warning(
                                                        "logout-confirm",
                                                        i18n::t(lang, i18n::Key::LogoutConfirmBody),
                                                    )
                                                    .title(i18n::t(
                                                        lang,
                                                        i18n::Key::LogoutConfirmTitle,
                                                    ))
                                                    .on_close({
                                                        let shell = shell.clone();
                                                        move |_e, window, cx| {
                                                            let _ = shell.update(cx, |this, cx| {
                                                                this.logout(cx)
                                                            });
                                                            // Pop the confirm, then the settings
                                                            // dialog beneath it.
                                                            window.close_dialog(cx);
                                                            window.close_dialog(cx);
                                                        }
                                                    }),
                                                )
                                            });
                                        }
                                    }),
                            ),
                    )
                });

            dialog
                .w(px(440.))
                .title(t(i18n::Key::SettingsTitle))
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
        window.open_dialog(cx, move |dialog, window, cx| {
            let shell_c = shell.clone();
            let dark = shell.read(cx).dark(window);
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
        let t = move |k: i18n::Key| i18n::t(lang, k);
        let shell = cx.entity();
        window.open_dialog(cx, move |dialog, _window, _cx| {
            let shell_c = shell.clone();
            let src = browse::reader_markdown_src(&title, &content, true);
            dialog
                .w(px(560.))
                .title(t(i18n::Key::History))
                .child(
                    TextView::markdown("history-preview", src)
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
    fn open_md_help(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) {
        let lang = self.lang;
        let t = move |k: i18n::Key| i18n::t(lang, k);
        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog
                .w(px(440.))
                .title(t(i18n::Key::MarkdownHelpTitle))
                .child(TextView::markdown(
                    "md-help",
                    browse::reader_markdown_src("", i18n::markdown_help_body(lang), true),
                ))
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
        let mut rest = content.as_str();
        while let Some(pos) = rest.find("getAttach?fileId=") {
            let after = &rest[pos + "getAttach?fileId=".len()..];
            let id: String = after
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            if !id.is_empty() && !ids.contains(&id) {
                ids.push(id);
            }
            rest = after;
        }
        let t = move |k: i18n::Key| i18n::t(lang, k);
        window.open_dialog(cx, move |dialog, window, cx| {
            let dark = shell.read(cx).dark(window);
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
                                .text_size(px(13.))
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
    fn insert_around(
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

    /// Flat (id, title, depth) of every valid move target: root + all
    /// notebooks except the moved one and its descendants (no cycles).
    fn notebook_move_targets(&mut self, moved: &leanote_core::Id) -> Vec<(String, String, usize)> {
        let Some(store) = self.store() else {
            return Vec::new();
        };
        let Ok(nbs) = store.notebooks() else {
            return Vec::new();
        };
        let by_id: std::collections::HashMap<String, String> = nbs
            .iter()
            .map(|nb| (nb.notebook_id.to_string(), nb.parent_notebook_id.clone()))
            .collect();
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
        let edit = self.nav_edit.clone()?;
        let (input, indent) = match &edit {
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
            NavEdit::RenameNotebook { name, .. } if group == "notebook" => (name.clone(), 1),
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
        window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> gpui_kit::Div {
        let fg = theme::foreground(dark);
        let muted = theme::muted_foreground(dark);
        let sel_fg = theme::accent_foreground();
        let notebook_names = self.notebook_names().clone();
        let all_notes = self.visible_notes().clone();
        // Issue 19: search box filters by title+desc substring.
        let q = self.list_query.to_lowercase();
        let notes: Vec<_> = if q.is_empty() {
            all_notes
        } else {
            all_notes
                .into_iter()
                .filter(|n| {
                    n.title.to_lowercase().contains(&q) || n.desc.to_lowercase().contains(&q)
                })
                .collect()
        };
        let count = notes.len();
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

        // Issue 19: search box under the header — filters the current list
        // (150ms debounce, ↩ jumps to the first hit).
        if self.list_filter.is_none() {
            let input = cx
                .new(|cx| InputState::new(window, cx).placeholder(t(i18n::Key::SearchNotesAction)));
            cx.subscribe(&input, |this, state, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) {
                    let q = state.read(cx).value().to_string();
                    this.queue_list_filter(q, cx);
                }
            })
            .detach();
            self.list_filter = Some(input);
        }
        let filter_entity = self.list_filter.clone().unwrap();

        let mut list = gpui_kit::base::v_flex()
            .w_full()
            .h_full()
            .bg(theme::material(Material::List, dark))
            // Issue 17: list column runs flush to the window top (drag
            // region is the left nav's 44px header only).
            .px_3()
            .gap_1()
            // Issue 19: top pad clears the transparent titlebar strip.
            .pt(px(44.))
            .child(header)
            .child(
                gpui_kit::div()
                    .id("list-filter")
                    .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                        if ev.keystroke.key == "enter" {
                            let notes = this.visible_notes();
                            if let Some(n) = notes.first() {
                                let id = n.note_id.clone();
                                this.select_note(id, window, cx);
                            }
                        }
                    }))
                    .child(
                        Input::new(&filter_entity)
                            .appearance(true)
                            .bordered(true)
                            .h_8()
                            .small(),
                    ),
            );

        if notes.is_empty() {
            list = list.child(small_label(t(i18n::Key::EmptyNotes), muted));
        } else {
            // Issue 17: virtualized — only visible rows render.
            let sel_id = self.selected.clone();
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
                                        &n.tags,
                                        n.updated_time,
                                    );
                                    // Issue 19: content excerpt (store-side
                                    // desc), single truncated line.
                                    let desc: SharedString = n.desc.clone().into();
                                    let is_sel = sel_id.as_ref() == Some(&n.note_id);
                                    note_list_row(
                                        &shell,
                                        n,
                                        meta.into(),
                                        desc,
                                        is_sel,
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
                        .w(px(12.))
                        .child(
                            Scrollbar::vertical(&scroll)
                                .viewport_from_layout()
                                .max_fps(60),
                        ),
                );
            list = list.child(list_view);
        }

        // Issue 18: "move note" picker replaces the note rows while
        // choosing (mirrors the nav's notebook-move picker).
        if let Some(NavEdit::MoveNote { id }) = &self.nav_edit {
            let moved = id.clone();
            let shell = cx.weak_entity();
            let mut picker = gpui_kit::base::v_flex().flex_1().min_h_0().gap_1().py_2();
            picker = picker.child(
                Label::new(format!("{} · {}", t(i18n::Key::MoveNoteTo), title))
                    .text_size(px(16.))
                    .text_color(fg),
            );
            let nodes = self.notebook_nav_nodes().clone();
            for node in &nodes {
                let shell2 = shell.clone();
                let pid = node.id.clone();
                let moved2 = moved.clone();
                picker = picker.child(
                    gpui_kit::base::h_flex()
                        .id(format!("note-mv-{pid}"))
                        .h_8()
                        .px_3()
                        .ml(px(12. * node.depth as f32))
                        .rounded(px(theme::RADIUS_SELECTION))
                        .on_click(move |_, _, cx| {
                            let _ = shell2.update(cx, |this, cx| {
                                this.move_note_to(moved2.clone(), pid.clone(), cx)
                            });
                        })
                        .child(Label::new(node.title.clone()).text_size(px(14.))),
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
                .bg(theme::material(Material::List, dark))
                .px_3()
                .gap_1()
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
        let fg = theme::foreground(dark);
        let muted = theme::muted_foreground(dark);
        let notebook_names = self.notebook_names().clone();

        let pane = gpui_kit::base::v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .bg(theme::material(Material::Background, dark))
            // Issue 17: top strip is the right-pane drag region.
            .child(reader_drag_strip(window, cx));

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
            let mut empty = gpui_kit::base::v_flex().gap_2();
            if let Some(bar) = bar {
                empty = empty.child(bar);
            }
            return pane.child(
                empty
                    .child(small_label(
                        i18n::t(self.lang, i18n::Key::NoSelection),
                        muted,
                    ))
                    .child(
                        Label::new(t(i18n::Key::NoSelection))
                            .text_size(px(16.))
                            .text_color(fg),
                    ),
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
                .on_click(
                    cx.listener(move |this, _ev, _window, cx| this.toggle_star(id.clone(), cx)),
                )
        } else {
            Button::new("star-toggle")
                .icon(IconName::Star)
                .ghost()
                .on_click(
                    cx.listener(move |this, _ev, _window, cx| this.toggle_star(id.clone(), cx)),
                )
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

        // Ticket 15: export actions (live notes only).
        if !is_trash_view {
            toolbar = toolbar
                .child(
                    Button::new("export-html")
                        .label(t(i18n::Key::ExportHtml))
                        .outline()
                        .on_click(cx.listener(|this, _ev, _window, cx| {
                            this.export_selected_file(false, cx)
                        })),
                )
                .child(
                    Button::new("export-markdown")
                        .label(t(i18n::Key::ExportMarkdown))
                        .outline()
                        .on_click(cx.listener(|this, _ev, _window, cx| {
                            this.export_selected_file(true, cx)
                        })),
                )
                .child(
                    Button::new("export-pdf")
                        .label(t(i18n::Key::ExportPdf))
                        .outline()
                        .on_click(
                            cx.listener(|this, _ev, _window, cx| this.export_selected_pdf(cx)),
                        ),
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

        let mut col = gpui_kit::base::v_flex().px(px(5.)).gap_2().min_h_0();
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
            // Issue 17: editor runs flush to the top; the reader's 28px
            // drag strip is above the toolbar when editing (the caller
            // nests this pane inside the reader).
            .child(reader_drag_strip(window, cx));

        let Some(ed) = &self.editing else {
            unreachable!("caller checked")
        };
        let title_entity = ed.title.clone();
        let content_entity = ed.content.clone();
        let preview = ed.preview;
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
                Input::new(&title_entity)
                    .appearance(false)
                    .bordered(false)
                    .h(px(32.))
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
            ))
            .child(icon_btn(
                "fullscreen",
                AssetIcon::Maximize,
                t(i18n::Key::Fullscreen),
                Box::new(|_ev, window, _cx| window.toggle_fullscreen()),
            ))
            .child(icon_btn(
                "done",
                AssetIcon::Check,
                t(i18n::Key::Done),
                Box::new(cx.listener(|this, _ev, _window, cx| this.stop_editing(cx))),
            ));

        // Markdown insertion toolbar (issue 19): wraps selection or inserts
        // at the cursor.
        let insert = |prefix: &'static str, suffix: &'static str| {
            cx.listener(move |this, _ev, window, cx| {
                this.insert_around(prefix, suffix, window, cx);
            })
        };
        let md_btn =
            |id: &'static str, icon: AssetIcon, prefix: &'static str, suffix: &'static str| {
                Button::new(id)
                    .icon(icon)
                    .ghost()
                    .xsmall()
                    .on_click(insert(prefix, suffix))
            };
        let md_toolbar = gpui_kit::base::h_flex()
            .gap_1()
            .child(md_btn("md-h1", AssetIcon::Heading1, "# ", ""))
            .child(md_btn("md-h2", AssetIcon::Heading2, "## ", ""))
            .child(md_btn("md-h3", AssetIcon::Heading3, "### ", ""))
            .child(md_btn("md-bold", AssetIcon::Bold, "**", "**"))
            .child(md_btn("md-italic", AssetIcon::Italic, "*", "*"))
            .child(md_btn("md-strike", AssetIcon::Strikethrough, "~~", "~~"))
            .child(md_btn("md-code", AssetIcon::Code, "`", "`"))
            .child(md_btn(
                "md-codeblock",
                AssetIcon::CodeXml,
                "\n```\n",
                "\n```\n",
            ))
            .child(md_btn("md-link", AssetIcon::Link, "[", "](url)"))
            .child(md_btn("md-list", AssetIcon::List, "- ", ""))
            .child(md_btn("md-olist", AssetIcon::ListOrdered, "1. ", ""))
            .child(md_btn("md-quote", AssetIcon::Quote, "> ", ""))
            .child(md_btn(
                "md-table",
                AssetIcon::Table,
                "\n| a | b |\n|---|---|\n| ",
                " |\n",
            ))
            .child(md_btn("md-hr", AssetIcon::Minus, "\n---\n", ""))
            .child(
                Button::new("md-help")
                    .icon(AssetIcon::MessageCircleQuestionMark)
                    .ghost()
                    .xsmall()
                    .on_click(cx.listener(|this, _ev, window, cx| this.open_md_help(window, cx))),
            );

        // Body: preview via TextView, source via kit Editor. Title input
        // lives in the title row; only the content scrolls.
        let body = if preview {
            let src = content_entity.read(cx).value().to_string();
            let title_txt = title_entity.read(cx).value().to_string();
            gpui_kit::base::v_flex()
                .min_h_0()
                .flex_1()
                .child(
                    TextView::markdown(
                        "editor-preview",
                        browse::reader_markdown_src(&title_txt, &src, true),
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

        let col = gpui_kit::base::v_flex()
            .flex_1()
            .min_w_0()
            .px(px(5.))
            .gap_2()
            .min_h_0()
            .h_full()
            .child(title_row)
            .when(!preview, |el| el.child(md_toolbar))
            .child(body)
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                // ⌘S / Ctrl+S save first (IME-safe: key_char is None for
                // cmd combos, so composition is never touched).
                let m = &ev.keystroke.modifiers;
                if (m.platform || m.control)
                    && !m.alt
                    && !m.shift
                    && ev.keystroke.key.eq_ignore_ascii_case("s")
                {
                    this.save_editor(cx);
                    cx.stop_propagation();
                    return;
                }
                // Vim grammar: bare keys carry no kit bindings, so this
                // capture listener is the first hook that sees them —
                // ahead of the input's own handlers and of AppKit
                // turning the key into text. Kit-bound keys (enter/
                // escape/backspace/delete) dispatch as actions and are
                // intercepted by the capture_action hooks below instead.
                if this.vim_key_down(&ev.keystroke, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action::<Enter>(cx.listener(move |this, _: &Enter, window, cx| {
                // Normal-mode enter / cmdline execute. Insert mode and
                // non-editor focus fall through to the kit handler.
                if this.vim_action_key("enter", window, cx) {
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
            .when(!preview, |el| {
                // Issue 17: vim mode indicator. Without it a Normal-mode
                // ESC silently no-ops and vim looks broken.
                let (label, cmdline) = if self.vim_enabled {
                    let m = crate::vim::mode_label(self.vim.mode);
                    (m, self.vim.cmdline().to_string())
                } else {
                    ("", String::new())
                };
                if label.is_empty() {
                    el
                } else {
                    el.child(
                        gpui_kit::base::h_flex()
                            .gap_2()
                            .px_1()
                            .child(small_label(label, muted))
                            .child(small_label(&cmdline, muted)),
                    )
                }
            });

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
        // ADR-0005: a fresh editor session starts in normal mode with a
        // block caret (the char under the cursor selected) — not a thin
        // insert caret.
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).selected_range(),
                0..1,
                "editor entry must show the normal-mode block caret"
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
            assert_eq!(range, 1..2, "vim l must block-select the next char");
        });
        // "i" + typing inserts at cursor; escape returns to normal.
        cx.simulate_keystrokes("i");
        cx.simulate_input("X");
        cx.simulate_keystrokes("escape");
        cx.update(|_, cx| {
            assert_eq!(
                content.read(cx).selected_range(),
                2..3,
                "esc from insert must block-select the char at the caret"
            );
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
                2..3,
                "$ must block-select the line's last char"
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
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// Window-state glue used by main.rs.

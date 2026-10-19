//! The main window: login form (right pane) → full-sync progress → Wake
//! three-pane shell (ADR-0003). Ticket 09: selectable nav + filtered list +
//! read-only Markdown reader, star toggle, trash actions.

use crate::browse::{self, NavNode};
use crate::conflict_bar;
use crate::i18n::{self, Lang};
use crate::theme::{self, Material};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::label::Label;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{px, SharedString, Task, Window};
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
                        shell.phase = Phase::Failed { error: e.into() };
                    }
                }
                cx.notify();
            });
        }));
    }

    fn dark(&self, window: &Window) -> bool {
        window.appearance() == gpui_kit::WindowAppearance::Dark
            || window.appearance() == gpui_kit::WindowAppearance::VibrantDark
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
        cx.notify();
    }

    fn select_note(&mut self, id: leanote_core::Id, cx: &mut gpui_kit::Context<Self>) {
        self.selected = Some(id);
        cx.notify();
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
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let dark = self.dark(window);
        let lang = self.lang;
        let t = move |k: i18n::Key| i18n::t(lang, k);
        let base_bg = theme::material(Material::Base, dark);
        let list_bg = theme::material(Material::List, dark);

        // Whole window: material[0] shows through the 44px transparent titlebar.
        let root = gpui_kit::base::v_flex().size_full().bg(base_bg).text_color(cx.theme().foreground);

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
                    .child(self.render_nav(dark, &t, cx))
                    .child(self.render_list(dark, &t, cx))
                    .child(self.render_reader(dark, &t, cx));
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
        cx: &mut gpui_kit::Context<Self>,
    ) -> gpui_kit::Div {
        let fg = theme::foreground(dark);
        let (notebooks_open, tags_open) = (self.notebooks_open, self.tags_open);

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

        if notebooks_open {
            for node in self.notebook_nav_nodes() {
                let sel = NavSelection::Notebook(node.id.clone());
                nav = nav.child(
                    self.nav_row(dark, &node, "notebook", Some(IconName::FolderClosed), self.nav == sel, cx)
                        .when(node.depth > 0, |el| el),
                );
            }
        }

        nav = nav.child(self.group_header(dark, t(i18n::Key::NavTags), tags_open, Group::Tags, cx));

        if tags_open {
            for node in self.tag_nav_nodes() {
                let sel = NavSelection::Tag(node.id.clone());
                nav = nav.child(self.nav_row(dark, &node, "tag", None, self.nav == sel, cx));
            }
        }

        nav
    }

    /// Middle list: two-row rows (title + meta), Wake 336px.
    fn render_list(
        &mut self,
        dark: bool,
        t: &dyn Fn(i18n::Key) -> &'static str,
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

        let mut list = gpui_kit::base::v_flex()
            .w(px(MID_W))
            .h_full()
            .flex_none()
            .bg(theme::material(Material::List, dark))
            .pt(px(TITLEBAR_H))
            .px_3()
            .gap_1()
            .child(
                Label::new(format!("{} · {}", title, count))
                    .text_size(px(16.))
                    .text_color(fg),
            );

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
    fn render_reader(
        &mut self,
        dark: bool,
        t: &dyn Fn(i18n::Key) -> &'static str,
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

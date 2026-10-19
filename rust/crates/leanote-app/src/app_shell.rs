//! The main window: login form (right pane) → full-sync progress → Wake
//! three-pane shell (ADR-0003).

use crate::i18n::{self, Lang};
use crate::theme::{self, Material};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::label::Label;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme, Disableable};
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
    /// Sync done → three-pane shell. stats for the summary line.
    Shell { stats: SyncStats },
    /// Login or sync failed → back to login with an error line.
    Failed { error: SharedString },
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
                Phase::Shell { stats: SyncStats { notebooks: 0, notes: 0, contents: 0, tags: 0 } }
            } else {
                Phase::Login
            },
            lang,
            data_dir,
            host,
            email,
            password,
            _task: None,
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
                    Ok(stats) => {
                        shell.phase = Phase::Shell { stats };
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
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let dark = self.dark(window);
        let t = |k: i18n::Key| i18n::t(self.lang, k);
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
            Phase::Shell { stats } => {
                body = body
                    .child(render_nav(dark, &t))
                    .child(self.render_list(dark, &t))
                    .child(render_reader(dark, stats, &t));
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

/// Left nav: identity area + All/Starred/Trash + Notebooks/Tags groups.
fn render_nav(dark: bool, t: &dyn Fn(i18n::Key) -> &'static str) -> impl IntoElement {
    let fg = theme::foreground(dark);
    let muted = theme::muted_foreground(dark);
    let item = |label: &str, selected: bool| {
        gpui_kit::base::h_flex()
            .px_3()
            .h_8()
            .rounded(px(theme::RADIUS_SELECTION))
            .when(selected, |el| el.bg(theme::accent()))
            .child(
                Label::new(label)
                    .text_size(px(14.))
                    .text_color(if selected { theme::accent_foreground() } else { fg }),
            )
    };
    gpui_kit::base::v_flex()
        .w(px(LEFT_W))
        .h_full()
        .flex_none()
        .bg(theme::material(Material::Base, dark))
        .pt(px(TITLEBAR_H)) // below the transparent titlebar
        .px_3()
        .gap_1()
        .child(Label::new(t(i18n::Key::AppName)).text_size(px(16.)).text_color(fg))
        .child(item(t(i18n::Key::NavAllNotes), true))
        .child(item(t(i18n::Key::NavStarred), false))
        .child(item(t(i18n::Key::NavTrash), false))
        .child(
            gpui_kit::base::v_flex()
                .mt_3()
                .gap_1()
                .child(Label::new(t(i18n::Key::NavNotebooks)).text_size(px(11.)).text_color(muted)),
        )
        .child(
            gpui_kit::base::v_flex()
                .mt_3()
                .gap_1()
                .child(Label::new(t(i18n::Key::NavTags)).text_size(px(11.)).text_color(muted)),
        )
        .into_element()
}

impl AppShell {
    /// Middle list: two-row rows (title + meta), Wake 336px.
    fn render_list(&self, dark: bool, t: &dyn Fn(i18n::Key) -> &'static str) -> impl IntoElement {
        let fg = theme::foreground(dark);
        let muted = theme::muted_foreground(dark);
        let notes = self.notes_snapshot();
        let count = notes.len();
        let mut list = gpui_kit::base::v_flex()
            .w(px(MID_W))
            .h_full()
            .flex_none()
            .bg(theme::material(Material::List, dark))
            .pt(px(TITLEBAR_H))
            .px_3()
            .gap_1()
            .child(Label::new(format!("{} · {}", t(i18n::Key::NotesListTitle), count)).text_size(px(16.)).text_color(fg));
        if notes.is_empty() {
            list = list.child(small_label(t(i18n::Key::EmptyNotes), muted));
        } else {
            for (title, meta) in &notes {
                list = list.child(
                    gpui_kit::base::v_flex()
                        .px_2()
                        .py_2()
                        .rounded(px(theme::RADIUS_SELECTION))
                        .child(Label::new(title.clone()).text_size(px(14.)).text_color(fg))
                        .child(small_label(meta.clone(), muted)),
                );
            }
        }
        list.into_element()
    }

    /// (title, meta) pairs for the middle list, from the store.
    fn notes_snapshot(&self) -> Vec<(SharedString, SharedString)> {
        let Ok(store) = leanote_core::Store::open(self.data_dir.clone(), "user1".into()) else {
            return Vec::new();
        };
        let Ok(notes) = store.list_notes() else {
            return Vec::new();
        };
        notes
            .into_iter()
            .map(|n| {
                let meta = format!("{} {}", i18n::t(self.lang, i18n::Key::Updated), fmt_time(n.updated_time));
                (SharedString::from(n.title), SharedString::from(meta))
            })
            .collect()
    }
}

/// Right pane: remainder width, 720px centered reading column.
fn render_reader(dark: bool, stats: &SyncStats, t: &dyn Fn(i18n::Key) -> &'static str) -> impl IntoElement {
    let fg = theme::foreground(dark);
    let muted = theme::muted_foreground(dark);
    gpui_kit::base::v_flex()
        .flex_1()
        .h_full()
        .min_w_0()
        .bg(theme::material(Material::Background, dark))
        .pt(px(TITLEBAR_H))
        .child(
            gpui_kit::base::v_flex()
                .w(px(READER_W))
                .mx_auto()
                .gap_2()
                .child(small_label(
                    format!(
                        "{} {} · {} {} · {} {}",
                        stats.notes, t(i18n::Key::NotesCount),
                        stats.notebooks, t(i18n::Key::NotebooksCount),
                        stats.tags, t(i18n::Key::TagsCount),
                    ),
                    muted,
                ))
                .child(Label::new(t(i18n::Key::NoSelection)).text_size(px(16.)).text_color(fg)),
        )
        .into_element()
}

/// epoch ms → "YYYY-MM-DD" (display only; store keeps epoch ms).
fn fmt_time(ms: i64) -> String {
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

// Window-state glue used by main.rs.

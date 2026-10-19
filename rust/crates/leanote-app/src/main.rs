//! leanote-app: GPUI desktop shell (login → full sync → three-pane reader).

mod app_shell;
mod i18n;
mod single_instance;
mod theme;
mod window_state;

use app_shell::AppShell;
use gpui_kit::component::Theme;
use gpui_kit::{px, AppContext as _, TitlebarOptions, WindowOptions};
use std::path::PathBuf;

/// `LEANOTE_RUST_DATA` wins (CLI convention); else the core default.
fn data_dir() -> PathBuf {
    std::env::var_os("LEANOTE_RUST_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(leanote_core::app_data_dir)
}

static FAKE_URL: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// `LEANOTE_FAKE_SERVER`:
///   `1`        → boot+seed fake server, prefill credentials
///   `http://…` → just prefill the host
fn fake_prefill() -> Option<(String, String, String)> {
    let v = std::env::var("LEANOTE_FAKE_SERVER").ok()?;
    if v.starts_with("http") {
        Some((v, String::new(), String::new()))
    } else if !v.is_empty() {
        boot_fake_server();
        Some((FAKE_URL.get().cloned().unwrap_or_default(), "user@example.com".into(), "right-password".into()))
    } else {
        None
    }
}

/// Fake server on its own thread + runtime, seeded for the demo.
/// Publishes its URL via the FAKE_URL lock.
fn boot_fake_server() {
    use serde_json::json;
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio rt");
        rt.block_on(async {
            let server = leanote_fakeserver::start().await;
            let _ = FAKE_URL.set(server.url());
            server.set_sync_usn(12);
            server.set_notebooks(vec![
                json!({"NotebookId":"nb1","Title":"Inbox","Seq":1,"ParentNotebookId":"","IsDeleted":false,"Usn":1,"CreatedTime":"2026-01-02T10:00:00","UpdatedTime":"2026-01-02T10:00:00"}),
                json!({"NotebookId":"nb2","Title":"Work","Seq":2,"ParentNotebookId":"","IsDeleted":false,"Usn":2,"CreatedTime":"2026-01-03T10:00:00","UpdatedTime":"2026-01-03T10:00:00"}),
                json!({"NotebookId":"nb3","Title":"Reading","Seq":3,"ParentNotebookId":"","IsDeleted":false,"Usn":3,"CreatedTime":"2026-01-04T10:00:00","UpdatedTime":"2026-01-04T10:00:00"}),
            ]);
            server.set_notes(vec![
                json!({"NoteId":"n1","Title":"Welcome to Leanote","NotebookId":"nb1","Usn":4,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-05T10:00:00","UpdatedTime":"2026-01-05T10:00:00"}),
                json!({"NoteId":"n2","Title":"Rust workspace layout","NotebookId":"nb2","Usn":5,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[\"rust\"]","CreatedTime":"2026-01-06T10:00:00","UpdatedTime":"2026-01-06T10:00:00"}),
                json!({"NoteId":"n3","Title":"GPUI notes","NotebookId":"nb2","Usn":6,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[\"rust\",\"gpui\"]","CreatedTime":"2026-01-07T10:00:00","UpdatedTime":"2026-01-07T10:00:00"}),
                json!({"NoteId":"n4","Title":"Reading list — 2026","NotebookId":"nb3","Usn":7,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-08T10:00:00","UpdatedTime":"2026-01-08T10:00:00"}),
                json!({"NoteId":"n5","Title":"Wake design tokens","NotebookId":"nb3","Usn":8,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[\"design\"]","CreatedTime":"2026-01-09T10:00:00","UpdatedTime":"2026-01-09T10:00:00"}),
            ]);
            let mut contents = serde_json::Map::new();
            for (id, body) in [
                ("n1", "# Welcome\n\nThis is your first synced note."),
                ("n2", "# Rust workspace layout\n\ncrates/leanote-core, cli, http, app."),
                ("n3", "# GPUI notes\n\nElement tree, entities, contexts."),
                ("n4", "# Reading list — 2026\n\n- Wake paper\n- GPUI docs"),
                ("n5", "# Wake design tokens\n\nMaterials, sizes, radii, spacing."),
            ] {
                contents.insert(id.to_string(), json!(body));
            }
            server.set_contents(contents);
            server.set_tags(vec![
                json!({"TagId":"t1","Tag":"rust","Usn":9,"IsDeleted":false}),
                json!({"TagId":"t2","Tag":"gpui","Usn":10,"IsDeleted":false}),
                json!({"TagId":"t3","Tag":"design","Usn":11,"IsDeleted":false}),
                json!({"TagId":"t4","Tag":"todo","Usn":12,"IsDeleted":false}),
            ]);
            // Serve until the process exits.
            std::future::pending::<()>().await;
        })
    });
    // The server binds before start() returns; give the thread a beat.
    for _ in 0..100 {
        if FAKE_URL.get().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn main() {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);

    let fake = fake_prefill();

    // Single instance: second launch relays focus and exits.
    let listener = match single_instance::acquire() {
        Ok(l) => l,
        Err(()) => std::process::exit(0),
    };

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);

        // Theme: follow system first (loads registered theme), then overlay
        // Wake tokens. Re-applied on every appearance change.
        Theme::sync_system_appearance(None, cx);
        Theme::update(cx, apply_wake_theme);

        let lang = i18n::system_lang();

        // Already signed in? → straight to shell (startup incr-sync is 04's).
        let signed_in = leanote_core::Store::open(dir.clone(), "user1".into())
            .ok()
            .and_then(|s| s.active_account().ok().flatten())
            .is_some();

        let displays: Vec<gpui_kit::Bounds<gpui_kit::Pixels>> =
            cx.displays().iter().map(|d| d.visible_bounds()).collect();
        let state = window_state::WindowState::load(&dir)
            .map(|s| s.clamped(&displays))
            .unwrap_or_else(|| {
                let b = gpui_kit::Bounds::centered(
                    None,
                    gpui_kit::size(px(window_state::DEFAULT_W), px(window_state::DEFAULT_H)),
                    cx,
                );
                window_state::WindowState::from_bounds(&b)
            });

        let bounds = state.bounds();
        let options = WindowOptions {
            window_bounds: Some(gpui_kit::WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Leanote".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_min_size: Some(gpui_kit::size(px(960.), px(600.))),
            ..Default::default()
        };

        let dir_for_window = dir.clone();
        let fake_for_window = fake.clone();
        let dir_close = dir.clone();
        let (handle, shell) = gpui_kit::open_window(options, cx, move |window, cx| {
            // Per-window appearance → keep tokens in sync with the OS.
            let _ = window.observe_window_appearance(|_window, cx| {
                Theme::sync_system_appearance(None, cx);
                Theme::update(cx, apply_wake_theme);
            });
            // Persist size/position at close.
            window.on_window_should_close(cx, move |window, _cx| {
                if let gpui_kit::WindowBounds::Windowed(b) = window.window_bounds() {
                    window_state::WindowState::from_bounds(&b).save(&dir_close);
                }
                true
            });
            cx.new(|cx| {
                AppShell::new(
                    window,
                    cx,
                    dir_for_window.clone(),
                    lang,
                    signed_in,
                    fake_for_window
                        .as_ref()
                        .map(|(h, e, p)| (h.as_str(), e.as_str(), p.as_str())),
                )
            })
        })
        .expect("open window");
        single_instance::remember_window(handle);
        single_instance::listen(listener, cx);

        // Demo helper: auto-submit the fake login for screenshots.
        if std::env::var("LEANOTE_FAKE_AUTOSUBMIT").ok().as_deref() == Some("1") {
            cx.spawn(async move |cx| {
                smol::Timer::after(std::time::Duration::from_secs(1)).await;
                let _ = shell.update(cx, |s, cx| s.start_login(cx));
            })
            .detach();
        }
    });
}

/// Wake tokens on top of the kit theme (materials, radii, fonts).
/// Call AFTER sync_system_appearance, never inside the same update that
/// changes mode (mode swap reloads and wipes color edits).
fn apply_wake_theme(theme: &mut Theme) {
    use gpui_kit::component::theme::{ThemeColor, ThemeMode};
    let dark = theme.mode == ThemeMode::Dark;
    // ThemeColor is Copy; light()/dark() hand back an Arc of the base table.
    let mut colors = *if dark { ThemeColor::dark() } else { ThemeColor::light() };
    colors.title_bar = theme::material(theme::Material::Base, dark);
    colors.sidebar = theme::material(theme::Material::Base, dark);
    colors.list = theme::material(theme::Material::List, dark);
    colors.background = theme::material(theme::Material::Background, dark);
    colors.popover = theme::material(theme::Material::Popover, dark);
    colors.foreground = theme::foreground(dark);
    colors.muted_foreground = theme::muted_foreground(dark);
    colors.border = theme::border(dark);
    colors.accent = theme::accent();
    colors.primary = theme::accent();
    theme.colors = colors;
    theme.radius = px(theme::RADIUS_PANEL);
    theme.radius_lg = px(theme::RADIUS_PANEL);
    theme.font_family = theme::SANS.into();
    theme.mono_font_family = theme::MONO.into();
    theme.font_size = px(14.);
    theme.shadow = false; // ADR-0003: zero shadow on persistent UI
}

//! leanote-app: GPUI desktop shell (login → full sync → three-pane reader).

// No console window on Windows GUI.
#![windows_subsystem = "windows"]

mod app_shell;
mod browse;
mod conflict_bar;
mod export;
mod fonts;
mod i18n;
mod markdown_input;
mod pdf;
mod prefs;
mod search_overlay;
mod single_instance;
mod theme;
mod vim;
mod window_state;

use app_shell::AppShell;
use gpui_base::input::set_language_config;
use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::component::input::language_config::LanguageConfig;
use gpui_kit::component::Theme;
use gpui_kit::{px, AppContext as _, TitlebarOptions, WindowOptions};
use std::path::PathBuf;

/// `LEANOTE_DATA` wins (CLI convention); else the core default.
fn data_dir() -> PathBuf {
    std::env::var_os("LEANOTE_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(leanote_core::app_data_dir)
}

/// ureq-backed `HttpClient` so `TextView::markdown` can fetch `Resource::Uri`
/// images (gpui defaults to `NullHttpClient`, which rejects every URI).
struct UreqHttpClient(ureq::Agent);

impl gpui_kit::http_client::HttpClient for UreqHttpClient {
    fn send(
        &self,
        req: gpui_kit::http_client::Request<gpui_kit::http_client::AsyncBody>,
    ) -> futures::future::BoxFuture<
        'static,
        anyhow::Result<gpui_kit::http_client::Response<gpui_kit::http_client::AsyncBody>>,
    > {
        use futures::FutureExt as _;
        use gpui_kit::http_client::{AsyncBody, RedirectPolicy};
        use std::io::Read as _;

        let follow = req
            .extensions()
            .get::<RedirectPolicy>()
            .cloned()
            .unwrap_or_default();
        let agent = self.0.clone();
        // Blocking ureq runs on smol's thread pool, off the UI thread.
        async move {
            smol::unblock(move || {
                let (parts, body) = req.into_parts();
                // AsyncBody is an in-memory cursor or async reader; image GETs
                // carry empty bodies — drain on this worker thread's executor.
                let mut send_body: Vec<u8> = Vec::new();
                smol::block_on(async {
                    use smol::io::AsyncReadExt as _;
                    let mut body = body;
                    body.read_to_end(&mut send_body).await
                })?;
                let agent = match follow {
                    RedirectPolicy::FollowAll => agent,
                    RedirectPolicy::FollowLimit(n) => ureq::Agent::config_builder()
                        .max_redirects(n)
                        .build()
                        .new_agent(),
                    RedirectPolicy::NoFollow => ureq::Agent::config_builder()
                        .max_redirects(0)
                        .build()
                        .new_agent(),
                };
                let mut request = http::Request::builder()
                    .method(parts.method.as_str())
                    .uri(parts.uri.to_string());
                for (name, value) in parts.headers.iter() {
                    request = request.header(name.as_str(), value.to_str().unwrap_or(""));
                }
                let (parts, mut body) = agent.run(request.body(send_body)?)?.into_parts();
                let mut bytes = Vec::new();
                body.as_reader().read_to_end(&mut bytes)?;
                Ok(http::Response::builder()
                    .status(parts.status)
                    .body(AsyncBody::from(bytes))?)
            })
            .await
        }
        .boxed()
    }

    fn user_agent(&self) -> Option<&gpui_kit::http_client::http::HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&gpui_kit::http_client::Url> {
        None
    }
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
        Some((
            FAKE_URL.get().cloned().unwrap_or_default(),
            "user@example.com".into(),
            "right-password".into(),
        ))
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
            server.set_sync_usn(17);
            server.set_notebooks(vec![
                json!({"NotebookId":"nb2","Title":"Work","Seq":1,"ParentNotebookId":"","IsDeleted":false,"Usn":2,"CreatedTime":"2026-01-03T10:00:00","UpdatedTime":"2026-01-03T10:00:00"}),
                // Nested children of Work: Seq order + nesting in nav (②).
                json!({"NotebookId":"nb2a","Title":"Research","Seq":1,"ParentNotebookId":"nb2","IsDeleted":false,"Usn":5,"CreatedTime":"2026-01-03T11:30:00","UpdatedTime":"2026-01-03T11:30:00"}),
                json!({"NotebookId":"nb2b","Title":"Meetings","Seq":2,"ParentNotebookId":"nb2","IsDeleted":false,"Usn":4,"CreatedTime":"2026-01-03T11:00:00","UpdatedTime":"2026-01-03T11:00:00"}),
                json!({"NotebookId":"nb3","Title":"Reading","Seq":2,"ParentNotebookId":"","IsDeleted":false,"Usn":3,"CreatedTime":"2026-01-04T10:00:00","UpdatedTime":"2026-01-04T10:00:00"}),
                json!({"NotebookId":"nb1","Title":"Inbox","Seq":3,"ParentNotebookId":"","IsDeleted":false,"Usn":1,"CreatedTime":"2026-01-02T10:00:00","UpdatedTime":"2026-01-02T10:00:00"}),
            ]);
            server.set_notes(vec![
                json!({"NoteId":"n1","Title":"Welcome to Leanote","NotebookId":"nb1","Usn":4,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-05T10:00:00","UpdatedTime":"2026-01-05T10:00:00"}),
                json!({"NoteId":"n2","Title":"Rust workspace layout","NotebookId":"nb2","Usn":5,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[\"rust\"]","CreatedTime":"2026-01-06T10:00:00","UpdatedTime":"2026-01-06T10:00:00"}),
                json!({"NoteId":"n3","Title":"GPUI notes","NotebookId":"nb2","Usn":6,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[\"rust\",\"gpui\"]","CreatedTime":"2026-01-07T10:00:00","UpdatedTime":"2026-01-07T10:00:00"}),
                json!({"NoteId":"n4","Title":"Reading list — 2026","NotebookId":"nb3","Usn":7,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-08T10:00:00","UpdatedTime":"2026-01-08T10:00:00"}),
                json!({"NoteId":"n5","Title":"Wake design tokens","NotebookId":"nb3","Usn":8,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[\"design\"]","CreatedTime":"2026-01-09T10:00:00","UpdatedTime":"2026-01-09T10:00:00"}),
                // Markdown showcase: headings/table/strike/code/link/quote/img (③⑥).
                json!({"NoteId":"n6","Title":"Markdown 渲染示例","NotebookId":"nb2a","Usn":13,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[\"demo\"]","CreatedTime":"2026-01-10T10:00:00","UpdatedTime":"2026-01-10T10:00:00"}),
                // Plain note → reader converts to Markdown for reading.
                json!({"NoteId":"n7","Title":"Plain text note","NotebookId":"nb1","Usn":14,"IsDeleted":false,"IsTrash":false,"IsMarkdown":false,"Tags":"[]","CreatedTime":"2026-01-11T10:00:00","UpdatedTime":"2026-01-11T10:00:00"}),
                // Trash (⑤).
                json!({"NoteId":"n8","Title":"Old draft (trash)","NotebookId":"nb2","Usn":15,"IsDeleted":false,"IsTrash":true,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-12T10:00:00","UpdatedTime":"2026-01-12T10:00:00"}),
                // Issue 21 需求9: image note (④) — getImage serves the png.
                json!({"NoteId":"n12","Title":"图片笔记","NotebookId":"nb1","Usn":20,"IsDeleted":false,"IsTrash":false,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-16T10:00:00","UpdatedTime":"2026-01-16T10:00:00"}),
                json!({"NoteId":"n9","Title":"Scratch (trash)","NotebookId":"nb3","Usn":16,"IsDeleted":false,"IsTrash":true,"IsMarkdown":true,"Tags":"[]","CreatedTime":"2026-01-13T10:00:00","UpdatedTime":"2026-01-13T10:00:00"}),
                // Rich text (TinyMCE) notes → read-only Markdown render + explicit convert (ticket 12).
                json!({"NoteId":"n10","Title":"2018 归档（富文本）","NotebookId":"nb3","Usn":18,"IsDeleted":false,"IsTrash":false,"IsMarkdown":false,"Tags":"[]","CreatedTime":"2026-01-14T10:00:00","UpdatedTime":"2026-01-14T10:00:00"}),
                json!({"NoteId":"n11","Title":"旅行清单（富文本）","NotebookId":"nb1","Usn":19,"IsDeleted":false,"IsTrash":false,"IsMarkdown":false,"Tags":"[\"travel\"]","CreatedTime":"2026-01-15T10:00:00","UpdatedTime":"2026-01-15T10:00:00"}),
            ]);
            let mut contents = serde_json::Map::new();
            for (id, body) in [
                ("n1", "# Welcome\n\nThis is your first synced note."),
                ("n2", "# Rust workspace layout\n\ncrates/leanote-core, cli, http, app."),
                ("n3", "# GPUI notes\n\nElement tree, entities, contexts."),
                ("n4", "# Reading list — 2026\n\n- Wake paper\n- GPUI docs"),
                ("n5", "# Wake design tokens\n\nMaterials, sizes, radii, spacing."),
                ("n6", "# Heading 1\n\n## Heading 2\n\nParagraph with **bold**, *italic*, `inline code`, a [link](https://leanote.com) and ~~strikethrough~~.\n\n> A blockquote for emphasis.\n\n| Feature | Electron | Rust |\n| --- | --- | --- |\n| Panes | 3 | 3 |\n| Markdown | marked | TextView |\n\n```rust\nfn main() {\n    println!(\"hello, leanote\");\n}\n```\n\n- list item one\n- list item two\n  - nested item\n\nRemote image:\n\n![red dot](https://upload.wikimedia.org/wikipedia/commons/thumb/0/02/Red_circle.svg/120px-Red_circle.svg.png)"),
                ("n7", "Plain text note\n\n  indented line\nline3"),
                ("n8", "# Old draft\n\nRestore me from trash (⑤)."),
                ("n9", "# Scratch\n\nHard-delete me."),
                ("n10", "<h2>2018 归档</h2><p>TinyMCE 时代的富文本笔记，<span style=\"color: #e03e3d;\">彩色文字</span>与<strong>加粗</strong>共存。</p><ul><li>第一季度<ul><li>一月</li><li>二月</li></ul></li><li>第二季度</li></ul><table><thead><tr><th>事件</th><th>月份</th></tr></thead><tbody><tr><td>启动</td><td>3 月</td></tr></tbody></table><blockquote><p>存档目的</p></blockquote>"),
                ("n11", "<h3>旅行清单</h3><p><em>北海道</em>，<u>冬季</u>出发。</p><ol><li>机票<ol><li>看价格</li></ol></li><li>酒店</li></ol><p><a href=\"https://leanote.com\">参考</a> <img src=\"https://via.placeholder.com/150\" alt=\"地图\"></p>"),
                ("n12", "# 图片笔记\n\n![dot](leanote://file/getImage?fileId=srvimg_demo)"),
            ] {
                contents.insert(id.to_string(), json!(body));
            }
            server.set_contents(contents);
            // Issue 21 需求9: the png bytes file/getImage serves. ponytail:
            // header-only png (8 bytes) proves the pipe; real photos come
            // from the local insert path, not this demo seed.
            server.set_images([(
                "srvimg_demo".to_string(),
                ("dot.png".to_string(), "image/png".to_string(), vec![
                    0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a,
                ]),
            )]);
            server.set_tags(vec![
                json!({"TagId":"t1","Tag":"rust","Usn":9,"IsDeleted":false}),
                json!({"TagId":"t2","Tag":"gpui","Usn":10,"IsDeleted":false}),
                json!({"TagId":"t3","Tag":"design","Usn":11,"IsDeleted":false}),
                json!({"TagId":"t4","Tag":"todo","Usn":12,"IsDeleted":false}),
                json!({"TagId":"t5","Tag":"demo","Usn":17,"IsDeleted":false}),
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
    // Crashes land in logs/app.log before anything else can go wrong.
    leanote_core::logging::install_panic_hook();
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);

    let fake = fake_prefill();

    // Single instance: second launch relays focus and exits.
    let listener = match single_instance::acquire() {
        Ok(l) => l,
        Err(()) => std::process::exit(0),
    };

    gpui_kit::application()
        .with_http_client(std::sync::Arc::new(UreqHttpClient(
            ureq::Agent::config_builder()
                .timeout_connect(Some(std::time::Duration::from_secs(10)))
                .timeout_global(Some(std::time::Duration::from_secs(30)))
                .http_status_as_error(false)
                .build()
                .new_agent(),
        )))
        // Icons: without an AssetSource every svg().path("icons/…") load
        // fails and Icon/IconName render empty — register the kit's
        // embedded Lucide catalog.
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);

            // Issue 21 需求5: markdown pair closing/fences are handled by
            // the shell's key capture (app_layer `markdown_input`) — turn
            // the kit's default auto-closing pairs off for markdown so
            // `(`/`[`/quotes aren't doubled.
            set_language_config(
                "markdown",
                LanguageConfig::default()
                    .brackets([])
                    .auto_closing_pairs([]),
                cx,
            );

            // Ticket 13: ⌘K / Ctrl+K toggle the global search overlay.
            // Issue 17: ⌘⇧P / Ctrl+Shift+P open the same palette.
            //
            // `secondary-` = ⌘ on macOS, Ctrl elsewhere: one binding per
            // gesture instead of a cmd- + ctrl- pair. Beyond brevity, the
            // pair is what made the ⌘K palette show the wrong hint: Kbd
            // picks the *last* registered binding for an action, so ⌘N
            // displayed as ⌃N on macOS.
            cx.bind_keys([
                gpui_kit::KeyBinding::new("secondary-k", search_overlay::ToggleSearch, None),
                gpui_kit::KeyBinding::new("secondary-shift-p", search_overlay::ToggleSearch, None),
                // Issue 19: ⇧⌘F focus mode (both cases: macOS reports the
                // shifted key as "F" on some IME paths).
                gpui_kit::KeyBinding::new(
                    "secondary-shift-f",
                    search_overlay::ToggleFocusMode,
                    None,
                ),
                gpui_kit::KeyBinding::new(
                    "secondary-shift-F",
                    search_overlay::ToggleFocusMode,
                    None,
                ),
                // Issue 21 需求10: ⌘T / Ctrl+T full-window markdown preview.
                gpui_kit::KeyBinding::new("secondary-t", search_overlay::ToggleFullPreview, None),
                // ⌘, / Ctrl+, settings. NOTE: was "cmd-", which parses to
                // key="" (trailing dash) and never matched a real keystroke.
                gpui_kit::KeyBinding::new("secondary-,", search_overlay::OpenSettings, None),
                // App-scope ⌘S sync / ⌘N new note. In the editor, ⌘S is
                // captured earlier (save + sync) and never reaches these.
                gpui_kit::KeyBinding::new("secondary-s", search_overlay::QuickSyncNow, None),
                gpui_kit::KeyBinding::new("secondary-n", search_overlay::QuickNewNote, None),
            ]);

            let prefs = prefs::Prefs::load(&dir);
            let pref_palette: std::sync::Arc<String> =
                std::sync::Arc::new(prefs.editor_palette.clone());
            let palette_boot = pref_palette.clone();

            // Theme: follow system first (loads registered theme), then overlay
            // Wake tokens + editor palette. Re-applied on every appearance
            // change. `apply_config` overwrites highlight_theme on every mode
            // reload, so the palette assignment lives here too.
            // Panes read window.appearance(); keep it in sync with the saved
            // pref (issue 19 #26).
            cx.set_window_appearance(match prefs.appearance {
                prefs::AppearancePref::Follow => None,
                prefs::AppearancePref::Light => Some(gpui_kit::WindowAppearance::Light),
                prefs::AppearancePref::Dark => Some(gpui_kit::WindowAppearance::Dark),
            });
            match prefs.appearance {
                prefs::AppearancePref::Follow => Theme::sync_system_appearance(None, cx),
                prefs::AppearancePref::Light => {
                    Theme::change(gpui_kit::component::ThemeMode::Light, None, cx)
                }
                prefs::AppearancePref::Dark => {
                    Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx)
                }
            }
            // Issue 19: restore persisted accent BEFORE the wake overlay —
            // apply_wake_theme reads theme::accent() into colors/tokens, so
            // setting it afterwards left kit hover/selection on the old hue.
            theme::set_accent(&prefs.accent);
            let zoom_boot = prefs.zoom;
            let font_boot = prefs.font.clone();
            let editor_size_boot = prefs.editor_font_size.clone();
            Theme::update(cx, move |theme| {
                apply_wake_theme(
                    theme,
                    &palette_boot,
                    &font_boot,
                    &editor_size_boot,
                    zoom_boot,
                )
            });

            // UI round 10 #6: fetch the WenKai ttf into ~/Library/Fonts on a
            // background thread. Non-fatal: a failure keeps the system font;
            // the settings row can retry. CoreText only sees the new file on
            // the next launch.
            std::thread::spawn(|| {
                if let Err(e) = fonts::install_wenkai() {
                    eprintln!("wenkai font install failed: {e}");
                }
            });

            let lang = match prefs.lang {
                prefs::LangPref::Follow => i18n::system_lang(),
                prefs::LangPref::Zh => i18n::Lang::Zh,
                prefs::LangPref::En => i18n::Lang::En,
            };

            // Already signed in? → straight to shell (startup incr-sync is 04's).
            let signed_in_account = leanote_core::Store::open(dir.clone(), "user1".into())
                .ok()
                .and_then(|s| s.active_account().ok().flatten());
            let signed_in = signed_in_account.is_some();
            // Login prefill: last account row (logout keeps it) — host+email
            // only; token is dead and password is never stored.
            let remembered = if signed_in {
                None
            } else {
                leanote_core::Store::open(dir.clone(), "user1".into())
                    .ok()
                    .and_then(|s| s.last_account().ok().flatten())
            };

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
                // Issue 23 #5: min size scales with the saved zoom factor.
                window_min_size: Some(gpui_kit::size(px(960. * prefs.zoom), px(600. * prefs.zoom))),
                // Issue 19: frosted window (materials stay translucent).
                window_background: gpui_kit::WindowBackgroundAppearance::Blurred,
                ..Default::default()
            };

            let dir_for_window = dir.clone();
            let fake_for_window = fake.clone();
            let remembered_for_window = remembered.clone();
            let dir_close = dir.clone();
            let (handle, shell) = gpui_kit::open_window(options, cx, move |window, cx| {
                // Per-window appearance → keep tokens in sync with the OS.
                // Mode reloads overwrite highlight_theme, so the palette is
                // re-applied here too.
                //
                // Issue 24: Follow-only. On a fixed Light/Dark pick this
                // observer still fires (AppKit's effectiveAppearance callback
                // is deferred), and its second replay was the lag users saw
                // on top of the already-correct same-frame switch. `Follow`
                // is the only mode where the system can move under us.
                let dir_for_observer = dir.clone();
                let _ = window.observe_window_appearance(move |_window, cx| {
                    let p = crate::prefs::Prefs::load(&dir_for_observer);
                    if p.appearance != crate::prefs::AppearancePref::Follow {
                        return;
                    }
                    Theme::sync_system_appearance(None, cx);
                    let palette = p.editor_palette.clone();
                    let font = p.font;
                    let editor_size = p.editor_font_size.clone();
                    let zoom = p.zoom;
                    Theme::update(cx, move |theme| {
                        apply_wake_theme(theme, &palette, &font, &editor_size, zoom)
                    });
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
                        signed_in_account.map(|a| a.user_id),
                        fake_for_window
                            .as_ref()
                            .map(|(h, e, p)| (h.as_str(), e.as_str(), p.as_str())),
                        remembered_for_window.clone(),
                    )
                })
            })
            .expect("open window");
            single_instance::remember_window(handle);
            single_instance::listen(listener, cx);

            // Review item 2: returning user → background incr sync (non-blocking).
            if signed_in {
                let shell_boot = shell.clone();
                cx.spawn(async move |cx| {
                    let _ = shell_boot.update(cx, |s, cx| s.sync_now(cx));
                })
                .detach();
            }

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
pub(crate) fn apply_wake_theme(
    theme: &mut Theme,
    palette_id: &str,
    font_pref: &str,
    editor_size: &str,
    zoom: f32,
) {
    use gpui_kit::component::theme::{ThemeColor, ThemeMode};
    let dark = theme.mode == ThemeMode::Dark;
    // ThemeColor is Copy; light()/dark() hand back an Arc of the base table.
    let mut colors = *if dark {
        ThemeColor::dark()
    } else {
        ThemeColor::light()
    };
    colors.title_bar = theme::material(theme::Material::Base, dark);
    colors.sidebar = theme::material(theme::Material::Base, dark);
    colors.list = theme::material(theme::Material::List, dark);
    colors.background = theme::material(theme::Material::Background, dark);
    colors.popover = theme::material(theme::Material::Popover, dark);
    colors.foreground = theme::foreground(dark);
    colors.muted_foreground = theme::muted_foreground(dark);
    colors.border = theme::border(dark);
    // Selection color: kit defaults (light: blue wash 0x1d4ed8@0.3, dark:
    // 0x55a0fc@0.3) — overriding with solid white made light-mode selection
    // invisible (white on white). Issue 24 regression: apply_wake_theme runs
    // on every theme switch AND accent change, so the override leaked into
    // the reader/preview selection after any accent pick in light mode.
    colors.accent = theme::accent();
    colors.primary = theme::accent();
    // White on the accent in BOTH modes: kit default (neutral-900/neutral-50)
    // put dark text on the mid-saturation wake accents — unreadable menu
    // hover / ⌘K selection / ghost-button hover in light mode.
    colors.accent_foreground = theme::accent_foreground();
    theme.colors = colors;
    theme.radius = px(theme::RADIUS_PANEL);
    theme.radius_lg = px(theme::RADIUS_PANEL);
    theme.font_family = theme::sans_for(font_pref).into();
    // Issue 24: rem size must come from theme.font_size — the kit Root
    // plugin re-stamps window rem from it every frame, so any window-level
    // set_rem_size was dead on arrival. Zoom drives the font size instead.
    theme.font_size = px(14. * zoom);
    // Editor face follows the UI font (one pref drives both); its SIZE is
    // the editor-font pref scaled by the same zoom as the rest of the UI.
    theme.mono_font_family = theme::sans_for(font_pref).into();
    theme.mono_font_size = px(theme::editor_font_px(editor_size) * zoom);
    theme.shadow = false; // ADR-0003: zero shadow on persistent UI
                          // Issue 17: editor palette; `""` = kit default (must RESET — skipping
                          // the write left a previously-picked palette live until restart).
    theme.highlight_theme = if palette_id.is_empty() {
        if dark {
            HighlightTheme::default_dark()
        } else {
            HighlightTheme::default_light()
        }
    } else {
        theme::editor_palette(palette_id, dark)
    };
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    /// Issue 24 regression: the white selection override made light-mode
    /// text selection invisible (white on white). apply_wake_theme runs on
    /// boot, every appearance change AND every accent change, so the token
    /// must stay a translucent kit default in BOTH modes.
    #[test]
    fn selection_stays_translucent_in_both_modes() {
        for mode in [
            gpui_kit::component::ThemeMode::Light,
            gpui_kit::component::ThemeMode::Dark,
        ] {
            let mut theme = gpui_kit::component::Theme::default();
            theme.mode = mode;
            apply_wake_theme(&mut theme, "", "", "", 1.0);
            let sel = theme.colors.selection;
            assert!(
                sel.a < 1.,
                "{mode:?}: selection must be a translucent wash, got {sel:?}"
            );
            // Light mode: a wash must not be white — white on white is
            // invisible on the light materials.
            if mode == gpui_kit::component::ThemeMode::Light {
                let white = gpui_kit::white();
                assert!(
                    (sel.l - white.l).abs() > 0.1 || sel.a < 0.9,
                    "{mode:?}: selection is effectively white — invisible in light mode"
                );
            }
        }
    }

    /// UI round 12 regression: a botched edit dropped `theme.font_size`, so
    /// the UI rendered at the kit default 16px rem while only the editor
    /// scaled — the zoom control appeared broken. Pin both font sizes.
    #[test]
    fn zoom_scales_ui_and_editor_fonts() {
        let mut theme = gpui_kit::component::Theme::default();
        apply_wake_theme(&mut theme, "", "", "medium", 1.2);
        assert_eq!(theme.font_size, gpui_kit::px(14. * 1.2));
        assert_eq!(theme.mono_font_size, gpui_kit::px(16. * 1.2));
    }
}

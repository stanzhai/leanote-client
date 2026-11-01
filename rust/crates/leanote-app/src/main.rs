//! leanote-app: GPUI desktop shell (login → full sync → three-pane reader).

// No console window on Windows GUI.
#![windows_subsystem = "windows"]

mod app_shell;
mod browse;
mod conflict_bar;
mod export;
mod fonts;
mod history_dialog;
mod i18n;
mod markdown_input;
mod mermaid;
mod pdf;
mod prefs;
mod search_overlay;
mod single_instance;
mod slash_panel;
mod theme;
mod theme_family;
mod vim;
mod window_state;

use app_shell::AppShell;
use gpui_base::input::set_language_config;
use gpui_kit::component::input::language_config::LanguageConfig;
use gpui_kit::component::Theme;
use gpui_kit::{px, AppContext as _, TitlebarOptions, WindowOptions};
use std::path::PathBuf;
gpui_kit::actions!(leanote, [QuitApp]);

/// Windows high-DPI viewport resync (ADR-0011 amendment, 2026-11-01).
///
/// `gpui-pre-windows` can end up with a layout viewport that disagrees with
/// the on-screen window on high-DPI desktops: during window creation the
/// OS DPI context can flip (96 ↔ monitor DPI, observed on a 175% remote
/// desktop), and a WM_SIZE handled while the backend's scale factor was
/// momentarily 1.0 records the CLIENT DEVICE size as the LOGICAL size. The
/// window then reports `scale_factor() = 1.75` while its client is only
/// `requested-logical` DEVICE pixels wide: the scene (logical × scale) is
/// ~57% larger than the backbuffer, so everything past the visible area
/// paints out of the window AND is unreachable by mouse hit-testing
/// (mouse positions are divided by the window scale). First user-visible
/// symptoms: caption buttons rendered mid-window, and the markdown
/// title-row toolbar buttons ignored clicks.
///
/// Fix at startup: measure the live window (its class/title identify it
/// uniquely in this single-instance app), and when
/// `viewport × scale ≠ live client`, force a WM_SIZE by nudging the window
/// ±2 device px. The backend recomputes the logical viewport with the
/// CURRENT scale factor, making layout, rendering and hit-testing agree.
/// `fixup_engaged()` then switches the close-time geometry save to device
/// pixels, because the creation path on such desktops treats the requested
/// LOGICAL size as DEVICE pixels — storing the live device size keeps the
/// window at the same on-screen size across relaunches instead of
/// shrinking by the scale factor each launch.
#[cfg(target_os = "windows")]
pub(crate) mod win_dpi_resync {
    use std::sync::atomic::{AtomicBool, Ordering};

    use gpui_kit::Pixels;
    use windows::Win32::Foundation::HWND;

    static ENGAGED: AtomicBool = AtomicBool::new(false);

    /// Whether the startup resync had to correct the viewport.
    pub fn fixup_engaged() -> bool {
        ENGAGED.load(Ordering::Relaxed)
    }

    pub fn run(window: &mut gpui_kit::Window) {
        ENGAGED.store(resync(window), Ordering::Relaxed);
    }

    fn resync(window: &mut gpui_kit::Window) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetClientRect, GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
        };

        let scale = window.scale_factor();
        if (scale - 1.0).abs() < 0.01 {
            return false;
        }
        let Some(hwnd) = our_hwnd() else {
            return false;
        };
        // Live client size in device pixels.
        let mut client = windows::Win32::Foundation::RECT::default();
        if !unsafe { GetClientRect(hwnd, &mut client) }.is_ok() {
            return false;
        }
        let (live_w, live_h) = (
            (client.right - client.left) as f32,
            (client.bottom - client.top) as f32,
        );
        let vp = window.viewport_size();
        let (expect_w, expect_h) = (vp.width * scale, vp.height * scale);
        // Consistent when the layout viewport times the window scale
        // equals the live client size (within a couple of frame pixels).
        if (expect_w.as_f32() - live_w).abs() <= 2. && (expect_h.as_f32() - live_h).abs() <= 2. {
            return false;
        }
        // Outer rect: SetWindowPos sizes the whole window incl. its frame.
        let mut outer = windows::Win32::Foundation::RECT::default();
        if !unsafe { GetWindowRect(hwnd, &mut outer) }.is_ok() {
            return false;
        }
        let (w, h) = (outer.right - outer.left, outer.bottom - outer.top);
        // Nudge ±2 device px so WM_SIZE fires with the CURRENT scale
        // factor; the backend then recomputes logical = device / scale.
        for (cw, ch) in [(w + 2, h + 2), (w, h)] {
            let _ = unsafe {
                SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    cw,
                    ch,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
        }
        true
    }

    /// Start an OS window drag from a client-area press. gpui's
    /// `start_window_move` is a no-op on the Windows backend, where dragging
    /// only works through HTCAPTION hit-testing — app_shell's title row (a
    /// client area that must keep its clicks) calls this after a
    /// press-and-move threshold: the classic `ReleaseCapture` +
    /// `WM_NCLBUTTONDOWN(HTCAPTION)` handoff moves the press into the OS
    /// caption drag loop without touching the window's NCHITTEST mapping.
    ///
    /// The handoff MUST be asynchronous (`PostMessageW`, never `SendMessageW`):
    /// this runs from inside a gpui mouse-move listener, deep in gpui's
    /// event dispatch, where App/dispatcher borrows are held. A synchronous
    /// send starts DefWindowProc's MODAL move loop right there, pumping
    /// messages reentrantly into that state — gpui panics (RefCell
    /// double-borrow) and, with `windows_subsystem = "windows"`, the panic
    /// aborts via failfast: exit code 0xC0000409 (the reported title-drag
    /// crash). `PostMessageW` queues the press; the drag loop is entered at
    /// message-loop level — exactly where a native HTCAPTION press enters
    /// it, which gpui handles fine (native strip drags work this way).
    pub fn start_window_drag() {
        use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
        use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetCursorPos, PostMessageW, HTCAPTION, WM_NCLBUTTONDOWN,
        };

        let Some(hwnd) = our_hwnd() else {
            return;
        };
        let mut point = POINT::default();
        if !unsafe { GetCursorPos(&mut point) }.is_ok() {
            return;
        }
        let _ = unsafe { ReleaseCapture() };
        let lparam = LPARAM(((point.y as isize) << 16) | ((point.x as isize) & 0xFFFF));
        // Async on purpose — see the comment above.
        let _ = unsafe {
            PostMessageW(
                Some(hwnd),
                WM_NCLBUTTONDOWN,
                WPARAM(HTCAPTION as usize),
                lparam,
            )
        };
    }

    /// The app's own main window: gpui-pre-windows registers the
    /// `Zed::Window` class; single-instance means at most one such window
    /// titled "Leanote" exists in this process.
    fn our_hwnd() -> Option<HWND> {
        use windows::core::w;
        use windows::Win32::UI::WindowsAndMessaging::{
            FindWindowW, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
        };

        // FindWindowW returns Result<HWND> in windows 0.62.
        let hwnd = unsafe { FindWindowW(w!("Zed::Window"), w!("Leanote")).ok()? };
        if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return None;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid != std::process::id() || !unsafe { IsWindowVisible(hwnd).as_bool() } {
            return None;
        }
        Some(hwnd)
    }

    /// Convert logical geometry to the device pixels the creation path on
    /// mismatched-DPI desktops expects (see the module comment).
    pub fn bounds_to_device(b: gpui_kit::Bounds<Pixels>, scale: f32) -> gpui_kit::Bounds<Pixels> {
        let s = |v: Pixels| gpui_kit::px(v.as_f32() * scale);
        gpui_kit::Bounds {
            origin: gpui_kit::point(s(b.origin.x), s(b.origin.y)),
            size: gpui_kit::size(s(b.size.width), s(b.size.height)),
        }
    }
}

/// `LEANOTE_DATA` wins (CLI convention); else the core default.
pub(crate) fn data_dir() -> PathBuf {
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
            // 历史版本演示：n2 的两个旧版本（服务端 getHistories，最新在前）。
            server.set_histories(
                "n2",
                vec![
                    json!({"UpdatedTime": "2026-02-02T09:30:00Z", "Content": "# Rust workspace layout\n\ncrates/leanote-core, cli, http, app.\n\nUpcoming: crates/leanote-sync split."}),
                    json!({"UpdatedTime": "2026-01-06T08:00:00Z", "Content": "# Rust workspace\n\ncore + cli, more coming."}),
                ],
            );
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
    // Windows main thread: run on a big-stack thread. The MSVC default is a
    // 1 MB main-thread stack, and the editor render path (title row + tag
    // chips + slash panel, each a gpui element subtree built recursively)
    // measures within ~10% of that ceiling on debug builds — the ADR-0011
    // round added just enough element depth to overflow it at boot with a
    // restored note (exit 0xC0000409, stack overflow inside
    // render_editor's tag-chip children). 64 MB gives the debug-build
    // element trees headroom; release builds are far shallower but pay
    // nothing for the reserve. The OS commits stack pages on demand, so
    // this costs no memory until used.
    if cfg!(target_os = "windows") {
        let child = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .name("main".into())
            .spawn(leanote_main)
            .expect("spawn main thread");
        // Park the process main thread; gpui's dispatcher owns the UI.
        let _ = child.join();
        return;
    }
    leanote_main()
}

fn leanote_main() {
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
            // ADR-0011: macOS 原生应用菜单 —— 系统服务入口 + ⌘Q 退出。
            // Windows/Linux 无应用菜单，不适用。
            #[cfg(target_os = "macos")]
            {
                use gpui_kit::{Menu, MenuItem, SystemMenuType};

                cx.set_menus([Menu::new("Leanote").items([
                    MenuItem::os_submenu("Services", SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action("Quit Leanote", QuitApp),
                ])]);
                cx.bind_keys([gpui_kit::KeyBinding::new("cmd-q", QuitApp, None)]);
                cx.on_action(|_: &QuitApp, cx| cx.quit());
            }

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
                gpui_kit::KeyBinding::new("secondary-n", search_overlay::QuickNewNote, None),
            ]);

            let prefs = prefs::Prefs::load(&dir);
            // ADR-0010: family first — apply_wake_theme's material/fg/border
            // reads go through CURRENT_FAMILY.
            theme::set_family(&prefs.ui_theme);
            let pref_palette: std::sync::Arc<String> = std::sync::Arc::new(prefs.ui_theme.clone());
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

            // UI round 10 #6 / Issue 39: register the cached WenKai ttf with
            // the text system IN-PROCESS before the first layout (works on
            // every platform, no restart — see fonts.rs). Missing file →
            // one background download, then live register + repaint.
            fonts::register(cx, &dir);
            if !fonts::font_path(&dir).is_file() {
                let dir_dl = dir.clone();
                cx.spawn(async move |cx| {
                    let d = dir_dl.clone();
                    if smol::unblock(move || fonts::install_wenkai(&d))
                        .await
                        .is_ok()
                    {
                        cx.update(|cx| {
                            if fonts::register(cx, &dir_dl) {
                                cx.refresh_windows();
                            }
                        });
                    }
                })
                .detach();
            }

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

            // Primary display FIRST: clamped()'s "saved display is gone"
            // fallback re-centers on displays[0], which must be the primary
            // (Issue 39 spec: 恢复到主屏幕居中).
            let mut displays: Vec<gpui_kit::Bounds<gpui_kit::Pixels>> =
                cx.displays().iter().map(|d| d.visible_bounds()).collect();
            if let Some(primary) = cx.primary_display() {
                let pb = primary.visible_bounds();
                displays.sort_by_key(|b| *b != pb);
            }
            let state = window_state::WindowState::load(&dir)
                .map(|s| s.clamped(&displays))
                .unwrap_or_else(|| {
                    let b = gpui_kit::Bounds::centered(
                        None,
                        gpui_kit::size(px(window_state::DEFAULT_W), px(window_state::DEFAULT_H)),
                        cx,
                    );
                    window_state::WindowState::from_bounds(&b, false)
                });

            let bounds = state.bounds();
            let options = WindowOptions {
                // Issue 39: reopen at the last geometry; maximized windows
                // reopen maximized (bounds act as the restore size).
                window_bounds: Some(if state.maximized {
                    gpui_kit::WindowBounds::Maximized(bounds)
                } else {
                    gpui_kit::WindowBounds::Windowed(bounds)
                }),
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
                // Follow bug: `let _ =` dropped the Subscription right here,
                // unsubscribing before the OS theme could ever change — the
                // observer never fired while running. detach() keeps it
                // alive for the window's lifetime.
                window
                    .observe_window_appearance(move |_window, cx| {
                        let p = crate::prefs::Prefs::load(&dir_for_observer);
                        if p.appearance != crate::prefs::AppearancePref::Follow {
                            return;
                        }
                        Theme::sync_system_appearance(None, cx);
                        let palette = p.ui_theme.clone();
                        let font = p.font;
                        let editor_size = p.editor_font_size.clone();
                        let zoom = p.zoom;
                        Theme::update(cx, move |theme| {
                            apply_wake_theme(theme, &palette, &font, &editor_size, zoom)
                        });
                    })
                    .detach();
                // Persist size/position at close. Windows DPI fixup: when
                // the startup resync engaged (see win_dpi_resync), the
                // creation path treats requested LOGICAL sizes as DEVICE
                // pixels — store the live geometry in device pixels so the
                // window reopens at the same on-screen size.
                window.on_window_should_close(cx, move |window, _cx| {
                    save_window_state(window, &dir_close);
                    true
                });
                cx.new(|cx| {
                    let shell = AppShell::new(
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
                    );
                    // Issue 39: geometry persistence also on move/resize,
                    // debounced. ⌘Q (cx.quit) and crashes never reach
                    // on_window_should_close, so a close-only hook lost the
                    // last position; ≤500ms staleness is invisible.
                    let dir_bounds = dir_for_window.clone();
                    let last_save = std::rc::Rc::new(std::cell::Cell::new(
                        std::time::Instant::now() - GEOMETRY_SAVE_EVERY,
                    ));
                    cx.observe_window_bounds(window, move |_, window, _| {
                        if last_save.get().elapsed() >= GEOMETRY_SAVE_EVERY {
                            last_save.set(std::time::Instant::now());
                            save_window_state(window, &dir_bounds);
                        }
                    })
                    .detach();
                    shell
                })
            })
            .expect("open window");
            // ADR-0011 amendment: re-sync the Windows layout viewport with
            // the live window on high-DPI desktops (see win_dpi_resync).
            #[cfg(target_os = "windows")]
            handle
                .update(cx, |_, window, _| win_dpi_resync::run(window))
                .ok();

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

/// Debounce for move/resize persistence (see save_window_state).
const GEOMETRY_SAVE_EVERY: std::time::Duration = std::time::Duration::from_millis(500);

/// Write the live window geometry to window-state.json. Windows DPI fixup:
/// when the startup resync engaged (see win_dpi_resync), the creation path
/// treats requested LOGICAL sizes as DEVICE pixels — store the live
/// geometry in device pixels so the window reopens at the same on-screen
/// size. Maximized/fullscreen both reopen maximized (bounds = restore size).
fn save_window_state(window: &mut gpui_kit::Window, dir: &std::path::Path) {
    let wb = window.window_bounds();
    let maximized = !matches!(wb, gpui_kit::WindowBounds::Windowed(_));
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    let mut b = wb.get_bounds();
    #[cfg(target_os = "windows")]
    if win_dpi_resync::fixup_engaged() {
        b = win_dpi_resync::bounds_to_device(b, window.scale_factor());
    }
    window_state::WindowState::from_bounds(&b, maximized).save(dir);
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
    // Selected segmented pills (settings rows) paint from the primary
    // button tokens. Written on BOTH sides (colors + tokens): reconcile
    // keeps a field only when the two agree — a token-only write survived
    // same-mode replays but any mode/appearance switch changed
    // colors.button_primary (kit per-mode default: neutral-900 light /
    // neutral-50 dark) and reconcile stamped that near-black/near-white
    // default back over the accent. Same trick as the transparent
    // background below.
    colors.button_primary = theme::accent();
    colors.button_primary_hover = theme::accent().opacity(0.9);
    colors.button_primary_active = theme::accent().opacity(0.8);
    colors.button_primary_foreground = theme::accent_foreground();
    // White on the accent in BOTH modes: kit default (neutral-900/neutral-50)
    // put dark text on the mid-saturation wake accents — unreadable menu
    // hover / ⌘K selection / ghost-button hover in light mode.
    colors.accent_foreground = theme::accent_foreground();
    theme.colors = colors;
    // Issue 19: the kit Root plugin paints the window from
    // `tokens.background`, a field distinct from `colors` — the kit default
    // (opaque white / neutral-950) covered the blurred window background.
    // Root must stay fully transparent; the nav pane paints the glass tint.
    //
    // Both sides are set to the SAME transparent value. Reconcile then sees
    // `colors.background` changed but `tokens.background.color` already
    // equal to it and keeps the token (its "set on both sides to one color"
    // branch) — writing only the token made reconcile stamp the opaque
    // material back in whenever the family/mode changed `colors.background`,
    // killing the glass until an identical second click. Only this pair:
    // tokens.popover feeds menus/dialog panels, which stay opaque.
    let transparent = gpui_kit::transparent_black();
    theme.colors.background = transparent;
    theme.tokens = (&theme.colors).into();
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
                          // ADR-0010: highlight theme comes from the active family ("" = kit
                          // default). Must RESET every call — skipping the write left a
                          // previously-picked family live until restart.
    theme.highlight_theme = theme::family_highlight(palette_id, dark);
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

#[cfg(test)]
mod glass_tests {
    use super::*;

    /// Round 4 regression: `Theme::update` runs `reconcile` after the
    /// closure, which overwrites `tokens.background` with the (opaque)
    /// `colors.background` whenever that color changed — family/accent/
    /// appearance switches killed the window glass until a no-change second
    /// click. The fix writes the SAME transparent value on both sides, the
    /// branch reconcile leaves untouched. Simulate reconcile's field loop
    /// for `background` after a family switch and assert the token survives.
    #[test]
    fn reconcile_keeps_transparent_background_token() {
        let mut theme = gpui_kit::component::Theme::default();
        apply_wake_theme(&mut theme, "moss", "", "", 1.0);
        // reconcile's per-field logic, hand-rolled for `background`.
        let colors_before = theme.colors;
        let tokens_before = theme.tokens;
        // A family/mode switch changes colors.background (opaque material
        // of the OTHER family) before our closure re-runs.
        apply_wake_theme(&mut theme, "", "", "", 1.0);
        if theme.colors.background != colors_before.background {
            if theme.tokens.background.color != theme.colors.background {
                theme.tokens.background = theme.colors.background.into();
            }
        } else if theme.tokens.background != tokens_before.background {
            theme.colors.background = theme.tokens.background.color;
        }
        let t = theme.tokens.background.color;
        assert!(
            t.a == 0.,
            "glass token must stay fully transparent after reconcile, got {t:?}"
        );
    }

    /// The settings segmented pills paint from tokens.button_primary. A
    /// token-only accent write was wiped by reconcile on every mode
    /// switch (colors.button_primary carried the kit per-mode default —
    /// near black in light, near WHITE in dark: the "sometimes white"
    /// selected pill). Writing colors + tokens to one value keeps the
    /// accent through reconcile, like the transparent background above.
    #[test]
    fn reconcile_keeps_accent_primary_button() {
        let mut theme = gpui_kit::component::Theme::default();
        apply_wake_theme(&mut theme, "", "", "", 1.0);
        // Mode switch: kit loads the OTHER mode's table, changing
        // colors.button_primary to its per-mode default, then our closure
        // re-runs. Hand-roll reconcile's field loop for button_primary.
        let colors_before = theme.colors;
        let tokens_before = theme.tokens;
        theme.colors.button_primary = gpui_kit::component::theme::ThemeColor::dark().button_primary;
        apply_wake_theme(&mut theme, "", "", "", 1.0);
        if theme.colors.button_primary != colors_before.button_primary {
            if theme.tokens.button_primary.color != theme.colors.button_primary {
                theme.tokens.button_primary = theme.colors.button_primary.into();
            }
        } else if theme.tokens.button_primary != tokens_before.button_primary {
            theme.colors.button_primary = theme.tokens.button_primary.color;
        }
        let expected = crate::theme::accent();
        let got = theme.tokens.button_primary.color;
        assert_eq!(
            got, expected,
            "selected pill must stay the accent after a mode switch, got {got:?}"
        );
    }
}

//! Minimal tray (ticket 16): status-item icon + menu 显示/退出.
//!
//! - left-click the icon → show + activate the main window
//! - 显示 → show + activate
//! - 退出 → true quit (`cx.quit()`, same teardown as ⌘Q)
//!
//! Platform norm for closing the last window is gpui's QuitMode::Default:
//! quit on Win/Linux, keep running on macOS (dock icon policy left
//! default) — tray 退出 is the explicit quit everywhere.
//!
//! Uses `tray-icon` (gpui-kit ships no tray API). macOS requires the tray
//! be created on the main thread inside the running app loop, so `init`
//! is only called from inside `Application::run`.

use crate::single_instance;
use gpui_kit::App;

thread_local! {
    /// The tray lives for the process lifetime, owned by the main thread
    /// (macOS TrayIcon is !Send; dropping would remove the status item).
    static TRAY: std::cell::RefCell<Option<tray_icon::TrayIcon>> =
        const { std::cell::RefCell::new(None) };
}

/// Embedded 512×512 app icon, decoded to RGBA for the tray. The `image`
/// crate is already a gpui dependency, so this adds no tree weight beyond
/// tray-icon itself.
fn icon() -> Option<tray_icon::Icon> {
    const ICON_PNG: &[u8] = include_bytes!("../../../../build/icon.png");
    let img = image::load_from_memory(ICON_PNG).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).ok()
}

/// Show + focus the main window (shared by left-click and 显示).
fn show_main_window(cx: &mut App) {
    cx.activate(true);
    if let Some(h) = single_instance::WINDOW.get() {
        let _ = h.update(cx, |_, window, _| window.activate_window());
    }
}

/// Commands bounced from the tray callbacks to the app thread.
enum TrayCmd {
    Show,
    Quit,
}

pub fn init(cx: &mut App) {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{TrayIconBuilder, TrayIconEvent};

    let lang = crate::i18n::system_lang();
    let t = |k: crate::i18n::Key| crate::i18n::t(lang, k);
    let show = MenuItem::with_id("show", t(crate::i18n::Key::TrayShow), true, None);
    let quit = MenuItem::with_id("quit", t(crate::i18n::Key::TrayQuit), true, None);
    let menu = Menu::new();
    let _ = menu.append(&show);
    let _ = menu.append(&quit);

    // Left-click must show the window, not open the menu.
    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("Leanote");
    if let Some(ic) = icon() {
        builder = builder.with_icon(ic);
    }
    if let Ok(tray) = builder.build() {
        TRAY.with(|t| *t.borrow_mut() = Some(tray));
    }

    // Handlers fire on whatever thread AppKit picks; bounce every event to
    // the app thread through one async-channel (send_blocking from the
    // handlers, recv on the app thread).
    let (tx, rx) = smol::channel::unbounded::<TrayCmd>();
    MenuEvent::set_event_handler(Some({
        let tx = tx.clone();
        move |ev: MenuEvent| match ev.id().as_ref() {
            "show" => {
                let _ = tx.send_blocking(TrayCmd::Show);
            }
            "quit" => {
                let _ = tx.send_blocking(TrayCmd::Quit);
            }
            _ => {}
        }
    }));
    TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: tray_icon::MouseButton::Left,
            button_state: tray_icon::MouseButtonState::Up,
            ..
        } = ev
        {
            let _ = tx.send_blocking(TrayCmd::Show);
        }
    }));

    // Drain on the app thread. The receiver's recv future is !Unpin, so
    // pin it once outside the loop.
    cx.spawn(async move |cx| {
        let mut rx = std::pin::pin!(rx);
        while let Ok(cmd) = rx.as_mut().recv().await {
            let _ = cx.update(|cx| match cmd {
                TrayCmd::Show => show_main_window(cx),
                TrayCmd::Quit => cx.quit(),
            });
        }
    })
    .detach();
}

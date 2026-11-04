//! leanote 移动端壳（issue 02）：登录门 + 底部 Tab 骨架。
//! 平台层 = vendored gpui-pre-mobile；数据 = leanote-core 直连。

pub mod app;
pub mod fake;
pub mod i18n;
pub mod notes_tab;
pub mod login;
pub mod prefs;
pub mod search_tab;
pub mod settings_tab;
pub mod theme;

/// Issue 01 spike 的双 target check 钩子，保留作冒烟。
pub fn platform_ok() -> &'static str {
    let _ = gpui_mobile::target_platform();
    "gpui-mobile"
}

// ── Android 入口（issue 10）─────────────────────────────────────────────────

#[cfg(target_os = "android")]
mod android {
    use super::app::MobileApp;
    use gpui::App;
    use gpui::WindowOptions;
    use gpui_kit::component::Root;
    use gpui_kit::prelude::*;
    use gpui_mobile::android::jni;

    #[no_mangle]
    fn android_main(app: android_activity::AndroidApp) {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("leanote-mobile"),
        );
        jni::install_panic_hook();

        // Android 窗口在 MainEvent::InitWindow 后才存在：run 回调延迟到
        // surface 就绪（gpui-mobile 语义），Application 必须活在整个循环上。
        let _platform = jni::init_platform(&app);
        let shared = match jni::shared_platform() {
            Some(s) => s,
            None => {
                log::error!("android_main: shared_platform() returned None");
                return;
            }
        };

        gpui::Application::with_platform(shared.into_rc()).run(|cx: &mut App| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let content = cx.new(|cx| MobileApp::new(window, cx));
                cx.new(|cx| Root::new(content, window, cx))
            })
            .expect("open leanote mobile window");
        });
    }
}

// ── iOS 入口 ────────────────────────────────────────────────────────────────

#[cfg(target_os = "ios")]
mod ios {
    use super::app::MobileApp;
    use gpui::App;
    use gpui::WindowOptions;
    use gpui_kit::component::Root;
    use gpui_kit::prelude::*;

    fn nslog(msg: &str) {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        unsafe {
            extern "C" {
                fn NSLog(fmt: *mut AnyObject, ...);
            }
            let c_msg = std::ffi::CString::new(msg).unwrap_or_default();
            let ns_msg: *mut AnyObject = msg_send![class!(NSString), alloc];
            let ns_msg: *mut AnyObject = msg_send![ns_msg, initWithUTF8String: c_msg.as_ptr()];
            let c_fmt = std::ffi::CString::new("%@").unwrap_or_default();
            let ns_fmt: *mut AnyObject = msg_send![class!(NSString), alloc];
            let ns_fmt: *mut AnyObject = msg_send![ns_fmt, initWithUTF8String: c_fmt.as_ptr()];
            NSLog(ns_fmt, ns_msg);
        }
    }

    struct NsLogLogger;
    impl log::Log for NsLogLogger {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            nslog(&format!("{}", record.args()));
        }
        fn flush(&self) {}
    }

    /// Called from main.m before the UIKit run loop starts.
    #[unsafe(no_mangle)]
    pub extern "C" fn leanote_ios_register_app() {
        let _ = log::set_logger(&NsLogLogger).map(|()| log::set_max_level(log::LevelFilter::Info));
        std::panic::set_hook(Box::new(|info| {
            nslog(&format!("leanote-mobile PANIC: {info}"));
        }));

        gpui_mobile::ios::ffi::set_app_callback(Box::new(|cx: &mut App| {
            gpui_kit::init(cx);
            cx.open_window(WindowOptions::default(), |window, cx| {
                // 主题/偏好/外观 observer 全在 MobileApp::new（有 window 语境，
                // 且避开 open_window 前 mode-reload 覆盖的坑）。
                let content = cx.new(|cx| MobileApp::new(window, cx));
                cx.new(|cx| Root::new(content, window, cx))
            })
            .expect("open leanote mobile window");
        }));
    }
}

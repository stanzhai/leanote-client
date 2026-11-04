//! 登录页（issue 02）：host/email/密码 → leanote-core `Api::login`。
//! 同步在 issue 03；这里只登录并把账户持久化进设备端 Store。

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::label::Label;
use gpui_kit::component::{ActiveTheme, Disableable};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, rems, Entity, EventEmitter, Task, Window};
use leanote_core::sync::Api;
use leanote_core::{app_data_dir, Account, Store};

pub enum LoginEvent {
    Success(Account),
}

pub struct LoginView {
    host: Entity<InputState>,
    email: Entity<InputState>,
    password: Entity<InputState>,
    busy: bool,
    error: Option<String>,
    _task: Option<Task<()>>,
}

impl EventEmitter<LoginEvent> for LoginView {}

impl LoginView {
    pub fn new(
        remembered: Option<(String, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let host = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("https://leanote.com");
            if let Some((h, _)) = &remembered {
                s = s.default_value(h);
            }
            s
        });
        let email = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("user@example.com");
            if let Some((_, e)) = &remembered {
                s = s.default_value(e);
            }
            s
        });
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("••••••")
                .masked(true)
        });
        Self {
            host,
            email,
            password,
            busy: false,
            error: None,
            _task: None,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.submit_with(
            self.host.read(cx).value().to_string(),
            self.email.read(cx).value().to_string(),
            self.password.read(cx).value().to_string(),
            cx,
        );
    }

    /// Dev auto-login path (LEANOTE_FAKE_SERVER) shares the real submit
    /// pipeline; the simulator can't be typed into programmatically.
    pub fn submit_with(
        &mut self,
        host: String,
        email: String,
        password: String,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();

        // Login 是阻塞 ureq —— off the UI thread（对齐桌面 run_login_sync）。
        let work = cx.background_spawn(async move {
            let dir = app_data_dir();
            (|| {
                let store = Store::open(dir, "user1".into())
                    .map_err(|_| "cannot open local store".to_string())?;
                Api::login(&host, &email, &password, &store).map_err(|e| e.to_string())
            })()
        });
        self._task = Some(cx.spawn(async move |this, cx| match work.await {
            Ok(account) => {
                let _ = this.update(cx, |_, cx| cx.emit(LoginEvent::Success(account)));
            }
            Err(e) => {
                let _ = this.update(cx, |v, cx| {
                    v.busy = false;
                    v.error = Some(e);
                    cx.notify();
                });
            }
        }));
    }
}

impl Render for LoginView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = |t: &str| {
            Label::new(t)
                .text_size(rems(0.75))
                .text_color(cx.theme().muted_foreground)
        };
        let field = |s: &Entity<InputState>| Input::new(s).appearance(true).bordered(true);

        let sign_in = if self.busy {
            Button::new("sign-in")
                .label("登录中…")
                .primary()
                .disabled(true)
        } else {
            Button::new("sign-in")
                .label(crate::i18n::t("sign_in"))
                .primary()
                .on_click(cx.listener(|this, _ev, _window, cx| this.submit(cx)))
        };

        let mut form = gpui_kit::base::v_flex()
            .gap_3()
            .w(px(320.))
            .child(Label::new(crate::i18n::t("login_title")).text_size(rems(1.4)))
            .child(
                gpui_kit::base::v_flex()
                    .gap_1()
                    .child(label(crate::i18n::t("host")))
                    .child(field(&self.host)),
            )
            .child(
                gpui_kit::base::v_flex()
                    .gap_1()
                    .child(label(crate::i18n::t("email")))
                    .child(field(&self.email)),
            )
            .child(
                gpui_kit::base::v_flex()
                    .gap_1()
                    .child(label(crate::i18n::t("password")))
                    .child(field(&self.password)),
            )
            .child(sign_in);
        if let Some(err) = &self.error {
            form = form.child(
                Label::new(err.clone())
                    .text_size(rems(0.75))
                    .text_color(cx.theme().danger),
            );
        }

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().background)
            .child(form)
    }
}

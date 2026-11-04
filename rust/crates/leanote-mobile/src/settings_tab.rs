//! 设置 Tab（issue 07）：外观（明暗/语言/缩放/主题对）+ 账户（信息/立即同步/登出）。
//! 偏好变更经事件回 MobileApp（保存 + 应用 + 全 UI 重渲染）。

use crate::i18n::t;
use crate::prefs::{MobilePrefs, APPEARANCES, LANGS, ZOOMS};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::label::Label;
use gpui_kit::component::{ActiveTheme, Disableable};
use gpui_kit::prelude::*;
use gpui_kit::{div, rems, EventEmitter, SharedString, Window};

pub enum SettingsEvent {
    /// 偏好已改（本地副本），由 MobileApp 应用并持久化。
    PrefsChanged(MobilePrefs),
    SyncNow,
    Logout,
}

pub struct SettingsTab {
    pub prefs: MobilePrefs,
    account: Option<(String, String)>, // (email, host)
    sync_busy: bool,
}

impl EventEmitter<SettingsEvent> for SettingsTab {}

impl SettingsTab {
    pub fn new(prefs: MobilePrefs, account: Option<(String, String)>) -> Self {
        Self {
            prefs,
            account,
            sync_busy: false,
        }
    }

    pub fn set_sync_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.sync_busy = busy;
        cx.notify();
    }

    /// 一行：标签 + 右侧选项 chips。
    fn row(
        label: &str,
        cx: &Context<Self>,
        children: Vec<gpui_kit::AnyElement>,
    ) -> gpui_kit::AnyElement {
        div()
            .id(SharedString::new(format!("set-{label}")))
            .w_full()
            .px_4()
            .py_3()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.5))
            .child(Label::new(label).text_size(rems(0.9)))
            .child(div().flex().gap_1().children(children))
            .into_any_element()
    }

    fn chip(
        text: &str,
        selected: bool,
        cx: &Context<Self>,
        on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static,
    ) -> gpui_kit::AnyElement {
        div()
            .id(SharedString::new(format!("chip-{text}")))
            .px_2()
            .py_1()
            .rounded_md()
            .when(selected, |el| {
                el.bg(cx.theme().accent)
                    .text_color(cx.theme().accent_foreground)
            })
            .when(!selected, |el| el.bg(cx.theme().sidebar))
            .child(Label::new(text).text_size(rems(0.75)))
            .on_click(on_click)
            .into_any_element()
    }
}

impl Render for SettingsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.insets().safe_area;
        let lang = self.prefs.lang.clone();
        let names = crate::theme::theme_names(cx);

        let mut col = gpui_kit::base::v_flex().w_full();
        col = col.child(
            div()
                .w_full()
                .px_4()
                .py_3()
                .child(Label::new(t("tab_settings")).text_size(rems(1.2))),
        );

        // ── 外观 ──
        col = col.child(
            div().w_full().px_4().pt_2().pb_1().child(
                Label::new(t("appearance"))
                    .text_size(rems(0.75))
                    .text_color(cx.theme().muted_foreground),
            ),
        );
        let appearance_chips: Vec<_> = APPEARANCES
            .iter()
            .map(|(id, zh, en)| {
                let text = if lang == "zh" { *zh } else { *en };
                let sel = self.prefs.appearance == *id;
                let id = *id;
                Self::chip(
                    text,
                    sel,
                    cx,
                    cx.listener(move |this, _ev, _window, cx| {
                        this.prefs.appearance = id.to_string();
                        cx.emit(SettingsEvent::PrefsChanged(this.prefs.clone()));
                        cx.notify();
                    }),
                )
            })
            .collect();
        col = col.child(Self::row(t("appearance"), cx, appearance_chips));

        // 主题对：浅色/深色各一个循环选择器（列表长，不做全列表弹层）。
        for (key, is_dark) in [("theme_light", false), ("theme_dark", true)] {
            let pool: Vec<gpui_kit::SharedString> = names
                .iter()
                .filter(|(_, d)| *d == is_dark)
                .map(|(n, _)| n.clone())
                .collect();
            let current = if is_dark {
                self.prefs.dark_theme.clone()
            } else {
                self.prefs.light_theme.clone()
            };
            let label = t(key);
            let cyc = Self::row(
                label,
                cx,
                vec![Self::chip(
                    &current.clone(),
                    false,
                    cx,
                    cx.listener(move |this, _ev, _window, cx| {
                        let ix = pool
                            .iter()
                            .position(|n| *n == current)
                            .map(|i| (i + 1) % pool.len())
                            .unwrap_or(0);
                        let picked = pool.get(ix).cloned().unwrap_or_default().to_string();
                        if is_dark {
                            this.prefs.dark_theme = picked;
                        } else {
                            this.prefs.light_theme = picked;
                        }
                        cx.emit(SettingsEvent::PrefsChanged(this.prefs.clone()));
                        cx.notify();
                    }),
                )],
            );
            col = col.child(cyc);
        }

        let zoom_chips: Vec<_> = ZOOMS
            .iter()
            .map(|z| {
                let z = *z;
                let sel = (self.prefs.zoom - z).abs() < 1e-3;
                Self::chip(
                    &format!("{}%", (z * 100.) as i32),
                    sel,
                    cx,
                    cx.listener(move |this, _ev, _window, cx| {
                        this.prefs.zoom = z;
                        cx.emit(SettingsEvent::PrefsChanged(this.prefs.clone()));
                        cx.notify();
                    }),
                )
            })
            .collect();
        col = col.child(Self::row(t("zoom"), cx, zoom_chips));

        let lang_chips: Vec<_> = LANGS
            .iter()
            .map(|(id, label)| {
                let sel = self.prefs.lang == *id;
                let id = *id;
                Self::chip(
                    label,
                    sel,
                    cx,
                    cx.listener(move |this, _ev, _window, cx| {
                        this.prefs.lang = id.to_string();
                        cx.emit(SettingsEvent::PrefsChanged(this.prefs.clone()));
                        cx.notify();
                    }),
                )
            })
            .collect();
        col = col.child(Self::row(t("language"), cx, lang_chips));

        // ── 账户 ──
        col = col.child(
            div().w_full().px_4().pt_4().pb_1().child(
                Label::new(t("account"))
                    .text_size(rems(0.75))
                    .text_color(cx.theme().muted_foreground),
            ),
        );
        if let Some((email, host)) = &self.account {
            col = col.child(Self::row(
                email,
                cx,
                vec![Label::new(host.clone())
                    .text_size(rems(0.7))
                    .text_color(cx.theme().muted_foreground)
                    .into_any_element()],
            ));
        }
        col = col.child(div().px_4().py_2().child(if self.sync_busy {
            Button::new("sync-now")
                .label(t("syncing_now"))
                .primary()
                .disabled(true)
        } else {
            Button::new("sync-now")
                .label(t("sync_now"))
                .primary()
                .on_click(cx.listener(|this, _ev, _window, cx| {
                    this.set_sync_busy(true, cx);
                    cx.emit(SettingsEvent::SyncNow);
                }))
        }));
        col =
            col.child(
                div().px_4().py_2().child(
                    Button::new("logout")
                        .label(t("logout"))
                        .on_click(cx.listener(|_, _ev, _window, cx| {
                            cx.emit(SettingsEvent::Logout);
                        })),
                ),
            );

        div()
            .size_full()
            .pt(safe.top)
            .bg(cx.theme().background)
            .child(
                div()
                    .id("settings-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .child(col),
            )
    }
}

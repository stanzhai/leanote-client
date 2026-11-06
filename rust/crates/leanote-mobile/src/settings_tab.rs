//! 设置 Tab（issue 07）：外观（明暗/语言/缩放/主题对）+ 账户（信息/立即同步/登出）。
//! 偏好变更经事件回 MobileApp（保存 + 应用 + 全 UI 重渲染）。

use crate::i18n::t;
use crate::prefs::{MobilePrefs, APPEARANCES, LANGS, ZOOMS};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::label::Label;
use gpui_kit::component::searchable_list::SearchableVec;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, IndexPath};
use gpui_kit::prelude::*;
use gpui_kit::{
    canvas, div, point, px, rems, rgb, Entity, EventEmitter, PathBuilder, SharedString, Window,
};

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
    /// 主题下拉状态（首帧惰性创建，需要 Window）。
    theme_select: Option<Entity<SelectState<SearchableVec<SharedString>>>>,
}

impl EventEmitter<SettingsEvent> for SettingsTab {}

impl SettingsTab {
    pub fn new(prefs: MobilePrefs, account: Option<(String, String)>) -> Self {
        Self {
            prefs,
            account,
            sync_busy: false,
            theme_select: None,
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

    /// 色板圆片（桌面 app_shell 同款结构）：None = 彩虹轮（跟随主题）
    /// + 白勾浮层；纯色片直接在圆角 div 上涂 bg——内容裁剪不裁圆角，
    /// 嵌套子 div 的 bg 会渲染成方块；选中勾 Icon 直接作子元素。
    fn swatch(
        id: &str,
        hex: Option<u32>,
        selected: bool,
        on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static,
    ) -> gpui_kit::AnyElement {
        match hex {
            Some(hex) => {
                let selector = format!("sw-{id}");
                div()
                    .id(SharedString::new(format!("sw-{id}")))
                    .debug_selector(move || selector)
                    .size(px(18.))
                    .rounded_full()
                    .bg(rgb(hex))
                    .when(selected, |el| {
                        el.child(
                            Icon::new(IconName::Check)
                                .size(px(18.))
                                .text_color(crate::theme::accent_foreground_for(hex)),
                        )
                    })
                    .on_click(on_click)
                    .into_any_element()
            }
            None => div()
                .id("sw-follow")
                .debug_selector(|| "sw-follow".into())
                .relative()
                .size(px(18.))
                .rounded_full()
                .overflow_hidden()
                .child(Self::rainbow_wheel().absolute().size_full())
                .when(selected, |el| {
                    // macOS accent-pick 风：白勾浮层；圆环易读成 hover。
                    el.child(
                        div()
                            .absolute()
                            .size_full()
                            .flex()
                            .justify_center()
                            .items_center()
                            .child(
                                Icon::new(IconName::Check)
                                    .size(px(14.))
                                    .text_color(gpui_kit::white()),
                            ),
                    )
                })
                .on_click(on_click)
                .into_any_element(),
        }
    }

    /// 六色扇形拼圆（桌面 app_shell 同款画法）。
    fn rainbow_wheel() -> gpui_kit::Canvas<()> {
        canvas(
            |_, _, _| {},
            |bounds, _, window, _| {
                const WHEEL: [u32; 6] =
                    [0xff453a, 0xff9f0a, 0xffd60a, 0x32d74b, 0x0a84ff, 0xbf5af2];
                let center = bounds.center();
                let r = bounds.size.width.min(bounds.size.height) * 0.5;
                let at = |a: f32| point(center.x + r * a.cos(), center.y + r * a.sin());
                let step = std::f32::consts::TAU / 6.;
                for (i, hex) in WHEEL.iter().enumerate() {
                    let a0 = i as f32 * step - std::f32::consts::FRAC_PI_2;
                    let mut wedge = PathBuilder::fill();
                    wedge.move_to(center);
                    wedge.line_to(at(a0));
                    wedge.arc_to(point(r, r), px(0.), false, true, at(a0 + step));
                    wedge.close();
                    if let Ok(path) = wedge.build() {
                        window.paint_path(path, rgb(*hex));
                    }
                }
            },
        )
    }
}

impl Render for SettingsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.insets().safe_area;
        let lang = self.prefs.lang.clone();
        let trunks = crate::theme::theme_trunks();

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

        // 主题（ADR-0014 修订，与桌面同步）：单族干下拉选择。
        // 首帧惰性建 SelectState（需要 Window），trunk 列表启动后不变。
        let select = self.theme_select.get_or_insert_with(|| {
            // 旧存值 "Navop" 归一为合并后的 "Default"（resolve 仍认两者）。
            let current: SharedString = match self.prefs.ui_theme.as_str() {
                "" | "Navop" => "Default".into(),
                other => other.into(),
            };
            let ix = trunks
                .iter()
                .position(|n| *n == current)
                .map(IndexPath::new);
            let state =
                cx.new(|cx| SelectState::new(SearchableVec::new(trunks.clone()), ix, window, cx));
            cx.subscribe(&state, |this, _, ev, cx| {
                if let SelectEvent::Confirm(Some(v)) = ev {
                    this.prefs.ui_theme = v.to_string();
                    cx.emit(SettingsEvent::PrefsChanged(this.prefs.clone()));
                    cx.notify();
                }
            })
            .detach();
            state
        });
        col = col.child(Self::row(
            t("theme"),
            cx,
            vec![Select::new(select)
                .id("theme-select")
                .w(px(180.))
                .menu_width(px(220.))
                .menu_max_h(px(320.))
                .into_any_element()],
        ));

        // 强调色：彩虹圆 = 跟随主题 primary（默认），纯色圆片 = 手动覆盖。
        let mut swatches = vec![Self::swatch(
            "follow",
            None,
            self.prefs.accent.is_empty(),
            cx.listener(|this, _ev, _window, cx| {
                this.prefs.accent.clear();
                cx.emit(SettingsEvent::PrefsChanged(this.prefs.clone()));
                cx.notify();
            }),
        )];
        for (id, hex) in crate::theme::ACCENTS {
            let sel = self.prefs.accent == *id;
            swatches.push(Self::swatch(
                id,
                Some(*hex),
                sel,
                cx.listener(move |this, _ev, _window, cx| {
                    this.prefs.accent = id.to_string();
                    cx.emit(SettingsEvent::PrefsChanged(this.prefs.clone()));
                    cx.notify();
                }),
            ));
        }
        col = col.child(Self::row(t("accent"), cx, swatches));

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
        col = col.child(
            div()
                .px_4()
                .py_2()
                .child(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::MobilePrefs;
    use gpui_kit::{px, TestAppContext, VisualTestContext};

    /// 强调色圆片可点、可选中：点纯色片落 accent 偏好，点彩虹轮清除
    /// 覆盖（修复前：纯色片渲染成方块、选中勾不出现）。
    #[gpui_kit::test]
    fn accent_swatches_click_and_pick(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx: &mut gpui_kit::App| {
            crate::theme::apply(&MobilePrefs::default(), None, cx);
        });
        let settings = cx.add_window(|_, _| SettingsTab::new(MobilePrefs::default(), None));
        cx.run_until_parked();
        let mut vcx = VisualTestContext::from_window(settings.into(), cx);
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        let follow = vcx
            .debug_bounds("sw-follow")
            .expect("follow swatch laid out");
        let blue = vcx.debug_bounds("sw-blue").expect("blue swatch laid out");
        for (name, b) in [("follow", follow), ("blue", blue)] {
            assert_eq!(
                (b.size.width, b.size.height),
                (px(18.), px(18.)),
                "{name} swatch must be the 18×18 pick target"
            );
        }
        assert!(blue.origin.x > follow.origin.x, "solids follow the wheel");
        vcx.simulate_click(blue.center(), gpui_kit::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            cx.update(|cx: &mut gpui_kit::App| settings.read(cx).unwrap().prefs.accent.clone()),
            "blue",
            "clicking the blue swatch picks blue"
        );
        vcx.simulate_click(follow.center(), gpui_kit::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            cx.update(|cx: &mut gpui_kit::App| settings.read(cx).unwrap().prefs.accent.clone()),
            "",
            "clicking the rainbow clears the override"
        );
    }
}

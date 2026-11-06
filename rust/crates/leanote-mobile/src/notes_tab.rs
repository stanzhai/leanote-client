//! 笔记 Tab（issue 04）：最近列表 + 笔记本/标签浏览 + 推入栈导航。
//! 根屏「最近」栏的汉堡按钮开左侧笔记本抽屉（滑入）。

use gpui_kit::assets::IconName as AssetIcon;
use gpui_kit::component::label::Label;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, rems, AnimationExt, EventEmitter, SharedString, Window};
use leanote_core::{NoteMeta, Store};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const RECENT_LIMIT: usize = 20; // 桌面同款常量（app_shell.rs#57 的镜像）

/// 手动刷新请求（issue 08：点「最近」头行的 ⟳ = 下拉刷新的等价物）。
pub enum NotesEvent {
    RefreshRequested,
}

/// 栈内一屏。
#[derive(Clone)]
pub enum Screen {
    Root,
    Notebooks,
    Tags,
    /// 某笔记本或标签下的笔记列表。
    NoteList {
        title: SharedString,
        filter: ListFilter,
    },
    /// 阅读页：note id + 展示标题。
    Reader {
        note_id: leanote_core::Id,
        title: SharedString,
    },
}

#[derive(Clone)]
pub enum ListFilter {
    Notebook(leanote_core::Id),
    Tag(String),
}

impl EventEmitter<NotesEvent> for NotesTab {}

pub struct NotesTab {
    store: Option<Arc<Mutex<Store>>>,
    stack: Vec<Screen>,
    /// 根屏左侧笔记本抽屉开关（汉堡按钮 / 点遮罩关闭）。
    drawer_open: bool,
}

impl NotesTab {
    /// Tab 栏显隐联动：阅读屏全屏沉浸（TabShell 观察 entity 变化）。
    pub fn is_reader(&self) -> bool {
        matches!(self.stack.last(), Some(Screen::Reader { .. }))
    }
    pub fn new(store: Option<Arc<Mutex<Store>>>) -> Self {
        // Dev 深链（模拟器无法点击）：notebooks|tags|reader:<noteId>
        let initial = match std::env::var("LEANOTE_INITIAL_SCREEN").as_deref() {
            Ok("notebooks") => Some(Screen::Notebooks),
            Ok("tags") => Some(Screen::Tags),
            Ok(v) if v.starts_with("reader:") => {
                v.strip_prefix("reader:").map(|id| Screen::Reader {
                    note_id: leanote_core::Id(id.to_string()),
                    title: "".into(),
                })
            }
            _ => None,
        };
        let mut stack = vec![Screen::Root];
        if let Some(s) = initial {
            stack.push(s);
        }
        Self {
            store,
            stack,
            drawer_open: false,
        }
    }

    fn push(&mut self, s: Screen, cx: &mut Context<Self>) {
        self.stack.push(s);
        cx.notify();
    }

    /// 跨 Tab 入口：搜索结果跳阅读屏（issue 06）。
    pub fn open_reader(
        &mut self,
        note_id: leanote_core::Id,
        title: SharedString,
        cx: &mut Context<Self>,
    ) {
        self.stack.truncate(1); // 从任意浏览深度直达
        self.push(Screen::Reader { note_id, title }, cx);
    }

    fn pop(&mut self, cx: &mut Context<Self>) {
        if self.stack.len() > 1 {
            self.stack.pop();
            cx.notify();
        }
    }

    fn with_store<T>(&self, f: impl FnOnce(&Store) -> T) -> Option<T> {
        self.store.as_ref().map(|s| f(&s.lock().unwrap()))
    }

    fn recent(&self) -> Vec<NoteMeta> {
        self.with_store(|s| s.list_notes_meta_recent(RECENT_LIMIT).unwrap_or_default())
            .unwrap_or_default()
    }

    /// (id, title, depth) — 父先于子、兄弟按 Seq；两层缩进够 v1 用。
    fn notebooks(&self) -> Vec<(leanote_core::Id, String, usize)> {
        self.with_store(|s| {
            let mut nbs = s.notebooks().unwrap_or_default();
            nbs.sort_by(|a, b| {
                a.parent_notebook_id
                    .cmp(&b.parent_notebook_id)
                    .then(a.seq.cmp(&b.seq))
            });
            nbs.iter()
                .map(|n| {
                    (
                        n.notebook_id.clone(),
                        n.title.clone(),
                        usize::from(!n.parent_notebook_id.is_empty()),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
    }

    fn tags(&self) -> Vec<(String, i64)> {
        self.with_store(|s| s.tag_note_counts().unwrap_or_default())
            .unwrap_or_default()
    }

    fn notes_for(&self, filter: &ListFilter) -> Vec<NoteMeta> {
        self.with_store(|s| match filter {
            ListFilter::Notebook(id) => s
                .list_notes_filtered_meta(Some(id), false)
                .unwrap_or_default(),
            ListFilter::Tag(t) => s.list_notes_by_tag_meta(t).unwrap_or_default(),
        })
        .unwrap_or_default()
    }

    /// 通用行：标题 + 摘要/计数 + 底部细线，on_click 由调用方用 cx.listener 构建。
    fn row(
        id: &str,
        title: &str,
        sub: Option<&str>,
        cx: &Context<Self>,
        on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static,
    ) -> gpui_kit::AnyElement {
        div()
            .id(SharedString::new(format!("row-{id}")))
            .w_full()
            .px_4()
            .py_3()
            .flex_col()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.5))
            .child(Label::new(title).text_size(rems(0.95)))
            .when_some(sub, |el, s| {
                el.child(
                    Label::new(s)
                        .text_size(rems(0.75))
                        .text_color(cx.theme().muted_foreground),
                )
            })
            .on_click(on_click)
            .into_any_element()
    }

    /// 导航头（非根屏）：返回 + 标题。
    fn header(&self, title: &str, cx: &Context<Self>) -> impl IntoElement + use<> {
        div()
            .id("nav-back")
            .w_full()
            .h(px(44.))
            .px_4()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.5))
            .child(
                Label::new(crate::i18n::t("back"))
                    .text_size(rems(0.9))
                    .text_color(cx.theme().accent),
            )
            .child(Label::new(title).text_size(rems(1.0)))
            .on_click(cx.listener(|this, _ev, _window, cx| this.pop(cx)))
    }

    /// 左侧笔记本抽屉：从左滑入 280px 面板，点笔记本直达其笔记列表，
    /// 点遮罩关闭。子笔记本按层级缩进（两层，与 Notebooks 屏同源）。
    fn drawer(&self, window: &mut Window, cx: &Context<Self>) -> gpui_kit::AnyElement {
        let safe = window.insets().safe_area;
        let mut list = gpui_kit::base::v_flex().w_full().pt(safe.top);
        list = list.child(
            div().w_full().px_4().py_3().child(
                Label::new(crate::i18n::t("notebooks"))
                    .text_size(rems(0.75))
                    .text_color(cx.theme().muted_foreground),
            ),
        );
        for (id, title, depth) in self.notebooks() {
            let t: SharedString = title.clone().into();
            let filter_id = id.clone();
            list = list.child(
                div()
                    .id(SharedString::new(format!("nb-drawer-{id}")))
                    .w_full()
                    .pl(px(16.) + px(24.) * depth as f32)
                    .pr_4()
                    .py_3()
                    .flex()
                    .items_center()
                    .child(Label::new(title).text_size(rems(0.9)))
                    .on_click(cx.listener(move |this, _ev, _window, cx| {
                        this.drawer_open = false;
                        this.stack.truncate(1);
                        this.push(
                            Screen::NoteList {
                                title: t.clone(),
                                filter: ListFilter::Notebook(filter_id.clone()),
                            },
                            cx,
                        );
                    })),
            );
        }
        let panel = div()
            .id("drawer-panel")
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .w(px(280.))
            .bg(cx.theme().popover)
            .shadow_lg()
            .overflow_y_scroll()
            .block_mouse_except_scroll()
            .child(list);
        // 滑入：左缘 -280 → 0（160ms ease-in-out）。
        let panel = panel.with_animation(
            "drawer-slide",
            gpui_kit::Animation::new(Duration::from_millis(160)).with_easing(gpui_kit::ease_in_out),
            |el, delta| el.left(px(-280. * (1. - delta))),
        );
        div()
            .id("drawer-scrim")
            .debug_selector(|| "drawer-scrim".to_string())
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(gpui_kit::black().opacity(0.4))
            // 遮罩必须挡住底下列表：Normal 命中盒不拦截，一次 tap 会
            // 穿透叠加的每一层（笔记本行、遮罩、底下的笔记行全触发）。
            .occlude()
            .child(panel)
            .on_click(cx.listener(|this, _ev, _window, cx| {
                this.drawer_open = false;
                cx.notify();
            }))
            .into_any_element()
    }
}

impl Render for NotesTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.insets().safe_area;
        let screen = self.stack.last().cloned().unwrap_or(Screen::Root);
        let is_root = matches!(screen, Screen::Root);
        let mut col = gpui_kit::base::v_flex().w_full();
        match screen {
            Screen::Root => {
                col = col
                    .child(
                        // 「最近」栏：☰ 开笔记本抽屉，⟳ 手动刷新（issue 08）。
                        div()
                            .id("recent-bar")
                            .w_full()
                            .px_4()
                            .py_3()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .id("drawer-toggle")
                                    .debug_selector(|| "drawer-toggle".to_string())
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Icon::new(AssetIcon::Menu)
                                            .size(px(20.))
                                            .text_color(cx.theme().foreground),
                                    )
                                    .child(
                                        Label::new(crate::i18n::t("recent")).text_size(rems(1.2)),
                                    )
                                    .on_click(cx.listener(|this, _ev, _window, cx| {
                                        this.drawer_open = true;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("recent-refresh")
                                    .flex()
                                    .items_center()
                                    .child(
                                        Icon::new(AssetIcon::RefreshCw)
                                            .size(px(18.))
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .on_click(cx.listener(|_, _ev, _window, cx| {
                                        cx.emit(NotesEvent::RefreshRequested);
                                    })),
                            ),
                    )
                    .children(self.recent().iter().map(|n| {
                        let title: SharedString = if n.title.is_empty() {
                            crate::i18n::t("untitled").into()
                        } else {
                            n.title.clone().into()
                        };
                        let t = title.clone();
                        let id = n.note_id.clone();
                        Self::row(
                            &n.note_id,
                            &title,
                            Some(&n.desc),
                            cx,
                            cx.listener(move |this, _ev, _window, cx| {
                                this.push(
                                    Screen::Reader {
                                        note_id: id.clone(),
                                        title: t.clone(),
                                    },
                                    cx,
                                )
                            }),
                        )
                    }))
                    .child(Self::row(
                        "browse-tags",
                        "标签",
                        None,
                        cx,
                        cx.listener(|this, _ev, _window, cx| this.push(Screen::Tags, cx)),
                    ));
            }
            Screen::Notebooks => {
                col = col.child(self.header(crate::i18n::t("notebooks"), cx));
                for (id, title, depth) in self.notebooks() {
                    let pad = px(16.) + px(32.) * depth as f32;
                    let t: SharedString = title.clone().into();
                    let filter_id = id.clone();
                    col = col.child(
                        div()
                            .id(SharedString::new(format!("nb-{id}")))
                            .w_full()
                            .pl(pad)
                            .pr_4()
                            .py_3()
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(0.5))
                            .child(Label::new(title).text_size(rems(0.95)))
                            .on_click(cx.listener(move |this, _ev, _window, cx| {
                                this.push(
                                    Screen::NoteList {
                                        title: t.clone(),
                                        filter: ListFilter::Notebook(filter_id.clone()),
                                    },
                                    cx,
                                );
                            })),
                    );
                }
            }
            Screen::Tags => {
                col = col.child(self.header(crate::i18n::t("tags"), cx));
                for (tag, count) in self.tags() {
                    let t = tag.clone();
                    let sub = format!("{count} 篇");
                    col = col.child(Self::row(
                        &format!("tag-{tag}"),
                        &tag,
                        Some(&sub),
                        cx,
                        cx.listener(move |this, _ev, _window, cx| {
                            this.push(
                                Screen::NoteList {
                                    title: t.clone().into(),
                                    filter: ListFilter::Tag(t.clone()),
                                },
                                cx,
                            );
                        }),
                    ));
                }
            }
            Screen::NoteList { title, filter } => {
                col = col.child(self.header(&title, cx));
                for n in self.notes_for(&filter) {
                    let t: SharedString = if n.title.is_empty() {
                        crate::i18n::t("untitled").into()
                    } else {
                        n.title.clone().into()
                    };
                    let nid = n.note_id.clone();
                    col = col.child(Self::row(&n.note_id, &t, Some(&n.desc), cx, {
                        let t = t.clone();
                        cx.listener(move |this, _ev, _window, cx| {
                            this.push(
                                Screen::Reader {
                                    note_id: nid.clone(),
                                    title: t.clone(),
                                },
                                cx,
                            )
                        })
                    }));
                }
            }
            Screen::Reader { note_id, title } => {
                // 渲染管线与桌面同源：图片解析 → HTML 转 md → TextView+Mermaid。
                let note = self
                    .with_store(|s| s.get_note(&note_id).ok().flatten())
                    .flatten();
                let src = note
                    .as_ref()
                    .map(|n| {
                        let content = self
                            .with_store(|s| leanote_reader::resolve_note_images(&n.content, s))
                            .unwrap_or_else(|| n.content.clone());
                        leanote_reader::reader_markdown_src(&n.title, &content, n.is_markdown)
                    })
                    .unwrap_or_else(|| format!("_{}_]", crate::i18n::t("note_unavailable")).into());
                return div()
                    .size_full()
                    .pt(safe.top)
                    .bg(cx.theme().background)
                    .child(
                        gpui_kit::base::v_flex()
                            .size_full()
                            .child(self.header(&title, cx))
                            .child(
                                div()
                                    .id("reader-scroll")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .child(
                                        leanote_reader::markdown_view(
                                            "reader".into(),
                                            src,
                                            cx.theme().is_dark(),
                                            cx.theme().accent,
                                        )
                                        .px_4()
                                        .pb_8(),
                                    ),
                            ),
                    )
                    .into_any_element();
            }
        }
        let content = div()
            .size_full()
            .pt(safe.top)
            .bg(cx.theme().background)
            .child(
                div()
                    .size_full()
                    .id("notes-scroll")
                    .overflow_y_scroll()
                    .child(col),
            );
        // 根屏抽屉覆盖层（面板自身处理刘海，不吃 safe.top）。
        if is_root && self.drawer_open {
            div()
                .size_full()
                .relative()
                .child(content)
                .child(self.drawer(window, cx))
                .into_any_element()
        } else {
            content.into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::MobilePrefs;
    use gpui_kit::{px, TestAppContext, VisualTestContext};

    /// 汉堡开抽屉 → 点遮罩关闭（无 store 时空列表也算打开）。
    #[gpui_kit::test]
    fn drawer_toggle_opens_and_closes(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx: &mut gpui_kit::App| {
            crate::theme::apply(&MobilePrefs::default(), None, cx);
        });
        let notes = cx.add_window(|_, _| NotesTab::new(None));
        cx.run_until_parked();
        let mut vcx = VisualTestContext::from_window(notes.into(), cx);
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        let toggle = vcx
            .debug_bounds("drawer-toggle")
            .expect("drawer toggle laid out");
        assert_eq!(toggle.size.height, px(20.), "icon-sized toggle");
        assert!(toggle.size.width > px(40.), "icon + 最近 label");
        vcx.simulate_click(toggle.center(), gpui_kit::Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.update(|cx: &mut gpui_kit::App| notes.read(cx).unwrap().drawer_open),
            "hamburger opens the drawer"
        );
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        let scrim = vcx
            .debug_bounds("drawer-scrim")
            .expect("scrim laid out while open");
        vcx.simulate_click(
            scrim.center() + gpui_kit::point(px(120.), px(0.)),
            gpui_kit::Modifiers::none(),
        );
        cx.run_until_parked();
        assert!(
            !cx.update(|cx: &mut gpui_kit::App| notes.read(cx).unwrap().drawer_open),
            "tapping the scrim closes the drawer"
        );
    }
}

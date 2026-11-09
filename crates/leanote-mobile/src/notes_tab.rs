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

/// 手动刷新请求（issue 08：点「最近」头行的 ⟳ = 下拉刷新的等价物）；
/// ReturnToSearch：从搜索结果进入的阅读屏 pop 时请求切回搜索 Tab
/// （返回到来源列表，而非笔记 Tab 根屏）。
pub enum NotesEvent {
    RefreshRequested,
    ReturnToSearch,
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
    /// 边缘右滑返回（issue 17）：进行中的手势累计位移；None = 无手势。
    edge_swipe: Option<(f32, f32)>,
    /// 栈顶阅读屏是否由搜索结果进入（pop 时切回搜索 Tab）。
    reader_from_search: bool,
}
/// 右滑起手须在左缘多少 px 内（iOS 边缘手势同款语义）。
const EDGE_SWIPE_START: f32 = 24.;
/// 累计右滑多少 px 触发返回。
const EDGE_SWIPE_MIN: f32 = 48.;

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
            edge_swipe: None,
            reader_from_search: false,
        }
    }

    fn push(&mut self, s: Screen, cx: &mut Context<Self>) {
        self.stack.push(s);
        self.edge_swipe = None; // 换屏废弃进行中的手势
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
        self.reader_from_search = true;
    }
    fn pop(&mut self, cx: &mut Context<Self>) {
        if self.stack.len() > 1 {
            let popped = self.stack.pop();
            if self.reader_from_search && matches!(popped, Some(Screen::Reader { .. })) {
                self.reader_from_search = false;
                cx.emit(NotesEvent::ReturnToSearch);
            }
            cx.notify();
        }
        self.edge_swipe = None;
    }

    /// 边缘右滑返回（issue 17）：触摸 pan 在平台层映射为 ScrollWheel 派发
    /// （vendored gpui-pre-mobile ios/events.rs），非根屏上从左缘
    /// `EDGE_SWIPE_START` 内起手、横向主导且累计右滑 ≥ `EDGE_SWIPE_MIN`
    /// 即 pop 一屏。只观察不消费——事件继续冒泡，列表竖滚不受影响。
    fn on_edge_swipe_scroll(&mut self, ev: &gpui_kit::ScrollWheelEvent, cx: &mut Context<Self>) {
        if self.stack.len() < 2 {
            self.edge_swipe = None;
            return;
        }
        let px_val = |p: &gpui_kit::Pixels| f32::from(p);
        match ev.touch_phase {
            gpui_kit::TouchPhase::Started => {
                // 仅触摸 pan 的 delta 是 Pixels（真实滚轮是 Lines）。
                self.edge_swipe = if matches!(ev.delta, gpui_kit::ScrollDelta::Pixels(_))
                    && px_val(&ev.position.x) <= EDGE_SWIPE_START
                {
                    Some((0., 0.))
                } else {
                    None
                };
            }
            gpui_kit::TouchPhase::Moved => {
                if let (Some(s), gpui_kit::ScrollDelta::Pixels(d)) = (self.edge_swipe, ev.delta) {
                    let s = (s.0 + px_val(&d.x), s.1 + px_val(&d.y));
                    if s.0 >= EDGE_SWIPE_MIN && s.0 > s.1.abs() * 2. {
                        self.pop(cx); // 右滑到位 → 返回
                    } else if s.1.abs() > s.0 {
                        self.edge_swipe = None; // 纵向主导 → 交给列表滚动
                    } else {
                        self.edge_swipe = Some(s);
                    }
                }
            }
            _ => self.edge_swipe = None, // Ended/Cancelled → 复位
        }
    }

    fn with_store<T>(&self, f: impl FnOnce(&Store) -> T) -> Option<T> {
        self.store.as_ref().map(|s| f(&s.lock().unwrap()))
    }

    fn recent(&self) -> Vec<NoteMeta> {
        self.with_store(|s| s.list_notes_meta_recent(RECENT_LIMIT).unwrap_or_default())
            .unwrap_or_default()
    }

    /// 桌面同源（leanote_core::nav::notebook_nodes，issue 15）：roots 按
    /// Seq、子挂父下按 Seq、深度优先；孤儿（父 id 无对应笔记本）不显示。
    /// 此前移动端自排（按 parent 字符串分组）→ 顺序/嵌套与桌面不一致。
    fn notebooks(&self) -> Vec<(leanote_core::Id, String, usize)> {
        self.with_store(|s| {
            let nbs = s.notebooks().unwrap_or_default();
            let rows: Vec<(String, String, String, i64)> = nbs
                .iter()
                .map(|n| {
                    (
                        n.notebook_id.to_string(),
                        n.title.clone(),
                        n.parent_notebook_id.clone(),
                        n.seq,
                    )
                })
                .collect();
            leanote_core::nav::notebook_nodes(&rows, &Default::default())
                .into_iter()
                .map(|n| (leanote_core::Id(n.id), n.title, n.depth as usize))
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
                                this.reader_from_search = false;
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
                            this.reader_from_search = false;
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
                    // 边缘右滑返回（issue 17）
                    .on_scroll_wheel(cx.listener(
                        |this, ev: &gpui_kit::ScrollWheelEvent, _w, cx| {
                            this.on_edge_swipe_scroll(ev, cx)
                        },
                    ))
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
            .when(!is_root, |el| {
                // 边缘右滑返回（issue 17）
                el.on_scroll_wheel(
                    cx.listener(|this, ev: &gpui_kit::ScrollWheelEvent, _w, cx| {
                        this.on_edge_swipe_scroll(ev, cx)
                    }),
                )
            })
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

    /// 回归（issue 15）：笔记本列表与桌面同序（leanote_core::nav 同源）——
    /// 子笔记本紧跟其父、深度 1；不再按 parent 字符串分组把子项甩到全表尾。
    #[test]
    fn notebooks_nest_children_under_parent() {
        let dir = std::env::temp_dir().join(format!("lt-nb-order-{}", std::process::id()));
        let store = Store::open(dir.clone(), "u1".into()).unwrap();
        // create_notebook 前插（新 root 取最小 Seq-1）：B 先建、A 后建 → A 在前。
        store.create_notebook("B", "").unwrap();
        let a = store.create_notebook("A", "").unwrap();
        store.create_notebook("A1", &a.notebook_id.0).unwrap();
        let tab = NotesTab::new(Some(std::sync::Arc::new(std::sync::Mutex::new(store))));
        let rows = tab.notebooks();
        let titles: Vec<(&str, usize)> = rows.iter().map(|(_, t, d)| (t.as_str(), *d)).collect();
        let ia = titles.iter().position(|(t, _)| *t == "A").unwrap();
        // 子紧随父（桌面 DFS 同款），且缩进一级。
        assert_eq!(
            titles[ia + 1],
            ("A1", 1),
            "child nested after parent: {titles:?}"
        );
        assert_eq!(titles[ia].1, 0);
        let ib = titles
            .iter()
            .position(|(t, d)| *t == "B" && *d == 0)
            .unwrap();
        assert!(
            ib > ia + 1,
            "later root stays after A's subtree: {titles:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 回归（issue 17）：非根屏左缘起手右滑 ≥48px → pop；屏幕中部起手
    /// 不触发。注入原始 TouchEvent 走真实管线：手势识别器 → pan 转滚轮
    /// 派发（与真机手指同一条路径）。
    #[gpui_kit::test]
    fn edge_swipe_right_pops_non_root_screen(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let notes = cx.add_window(|_, _| NotesTab::new(None));
        notes
            .update(cx, |t, _window, cx| t.push(Screen::Notebooks, cx))
            .unwrap();
        let wh: gpui_kit::AnyWindowHandle = notes.into();
        let mut vcx = VisualTestContext::from_window(wh, cx);
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        let touch = |vcx: &mut VisualTestContext, id: u64, phase, x: f32, y: f32| {
            vcx.simulate_event(gpui_kit::TouchEvent {
                id: gpui_kit::TouchId(id),
                phase,
                position: gpui_kit::point(px(x), px(y)),
                predicted_position: None,
                force: None,
            });
        };
        let depth = |cx: &mut TestAppContext| {
            cx.update(|cx: &mut gpui_kit::App| notes.read(cx).unwrap().stack.len())
        };

        // 屏幕中部起手右滑：不是返回手势。
        touch(&mut vcx, 1, gpui_kit::TouchPhase::Started, 200., 300.);
        touch(&mut vcx, 1, gpui_kit::TouchPhase::Moved, 240., 301.);
        touch(&mut vcx, 1, gpui_kit::TouchPhase::Ended, 280., 302.);
        assert_eq!(depth(cx), 2, "mid-screen pan is not a back gesture");

        // 左缘起手横向右滑到位：返回。
        touch(&mut vcx, 2, gpui_kit::TouchPhase::Started, 8., 300.);
        touch(&mut vcx, 2, gpui_kit::TouchPhase::Moved, 40., 300.);
        touch(&mut vcx, 2, gpui_kit::TouchPhase::Moved, 72., 301.);
        touch(&mut vcx, 2, gpui_kit::TouchPhase::Ended, 80., 301.);
        assert_eq!(depth(cx), 1, "edge swipe right pops the screen");
    }

    /// 回归（本次）：搜索进入的阅读屏 pop → 发 ReturnToSearch（Tab 壳切回
    /// 搜索列表）；笔记 Tab 内部进入的阅读屏 pop → 不发。
    #[gpui_kit::test]
    fn reader_pop_returns_to_origin(cx: &mut TestAppContext) {
        use std::cell::RefCell;
        cx.update(gpui_kit::init);
        let notes = cx.add_window(|_, _| NotesTab::new(None));
        let returns = std::rc::Rc::new(RefCell::new(0usize));
        cx.update(|cx: &mut gpui_kit::App| {
            let entity = notes.entity(cx).unwrap();
            cx.subscribe(&entity, {
                let returns = returns.clone();
                move |_, ev: &NotesEvent, _| {
                    if matches!(ev, NotesEvent::ReturnToSearch) {
                        *returns.borrow_mut() += 1;
                    }
                }
            })
            .detach();
        });

        // 搜索结果进入 → pop 触发 ReturnToSearch。
        notes
            .update(cx, |t, _, cx| {
                t.open_reader(leanote_core::Id("n1".into()), "T".into(), cx)
            })
            .unwrap();
        notes.update(cx, |t, _, cx| t.pop(cx)).unwrap();
        assert_eq!(
            *returns.borrow(),
            1,
            "search-opened reader pops back to search"
        );

        // 笔记 Tab 内部进入 → pop 不触发。
        notes
            .update(cx, |t, _, cx| {
                t.push(
                    Screen::Reader {
                        note_id: leanote_core::Id("n2".into()),
                        title: "T2".into(),
                    },
                    cx,
                )
            })
            .unwrap();
        notes.update(cx, |t, _, cx| t.pop(cx)).unwrap();
        assert_eq!(*returns.borrow(), 1, "local reader pop stays in notes tab");
        assert_eq!(
            cx.update(|cx: &mut gpui_kit::App| notes.read(cx).unwrap().stack.len()),
            1
        );
    }
}

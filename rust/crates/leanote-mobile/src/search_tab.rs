//! 搜索 Tab（issue 06）：统一搜索核心（leanote-core `search_notes`，
//! 桌面 ⌘K 同源）+ 触屏输入框 + 结果列表，点击经事件跳笔记 Tab 阅读屏。

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::label::Label;
use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::{div, rems, Entity, EventEmitter, SharedString, Task, Window};
use leanote_core::{search_notes, SearchHit, Store};
use std::sync::{Arc, Mutex};

pub enum SearchEvent {
    OpenNote {
        note_id: leanote_core::Id,
        title: SharedString,
    },
}

pub struct SearchTab {
    store: Option<Arc<Mutex<Store>>>,
    input: Entity<InputState>,
    hits: Vec<SearchHit>,
    _task: Option<Task<()>>,
}

impl EventEmitter<SearchEvent> for SearchTab {}

impl SearchTab {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Dev 冒烟：LEANOTE_SEARCH=关键词 预填并即搜（模拟器无法键入）。
        let prefill = std::env::var("LEANOTE_SEARCH").ok();
        let input = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder(crate::i18n::t("search_placeholder"));
            if let Some(q) = &prefill {
                s = s.default_value(q);
            }
            s
        });
        cx.subscribe(&input, |this, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                this.schedule(cx);
            }
        })
        .detach();
        let mut tab = Self {
            store: None,
            input,
            hits: Vec::new(),
            _task: None,
        };
        if prefill.is_some() {
            tab.run(cx);
        }
        tab
    }

    /// 登出时清状态（issue 07）。
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.store = None;
        self.hits.clear();
        cx.notify();
    }

    /// 同步完成后再挂 store（实体在 MobileApp::new 时随窗口先建好）。
    pub fn set_store(&mut self, store: Arc<Mutex<Store>>, cx: &mut Context<Self>) {
        self.store = Some(store);
        if !self.input.read(cx).value().is_empty() {
            self.run(cx);
        }
    }

    /// 250ms 防抖：停顿后再搜，输入中途不抖库（桌面 search_task 同语义）。
    fn schedule(&mut self, cx: &mut Context<Self>) {
        self._task = Some(cx.spawn(async move |this, cx| {
            smol::Timer::after(std::time::Duration::from_millis(250)).await;
            let _ = this.update(cx, |t, cx| t.run(cx));
        }));
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let kw = self.input.read(cx).value().to_string();
        self.hits = self
            .store
            .as_ref()
            .map(|s| {
                let s = s.lock().unwrap();
                search_notes(&s.list_notes().unwrap_or_default(), &kw, 20)
            })
            .unwrap_or_default();
        cx.notify();
    }

    fn row(
        hit: &SearchHit,
        cx: &Context<Self>,
        on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static,
    ) -> gpui_kit::AnyElement {
        let title: SharedString = if hit.note.title.is_empty() {
            crate::i18n::t("untitled").into()
        } else {
            hit.note.title.clone().into()
        };
        let excerpt: SharedString = hit.excerpt.clone().into();
        div()
            .id(SharedString::new(format!("hit-{}", hit.note.note_id.0)))
            .w_full()
            .px_4()
            .py_3()
            .flex_col()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.5))
            .child(Label::new(title).text_size(rems(0.95)))
            .child(
                Label::new(excerpt)
                    .text_size(rems(0.75))
                    .text_color(cx.theme().muted_foreground),
            )
            .on_click(on_click)
            .into_any_element()
    }
}

impl Render for SearchTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let safe = window.insets().safe_area;
        let mut col = gpui_kit::base::v_flex().w_full().gap_2().child(
            div()
                .px_4()
                .py_3()
                .child(Input::new(&self.input).appearance(true).bordered(true)),
        );
        if self.hits.is_empty() {
            col = col.child(
                div()
                    .h(px_(300.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Label::new(crate::i18n::t("search_empty"))
                            .text_size(rems(0.9))
                            .text_color(cx.theme().muted_foreground),
                    ),
            );
        } else {
            for hit in &self.hits {
                let id = hit.note.note_id.clone();
                let t: SharedString = if hit.note.title.is_empty() {
                    crate::i18n::t("untitled").into()
                } else {
                    hit.note.title.clone().into()
                };
                col = col.child(Self::row(
                    hit,
                    cx,
                    cx.listener(move |_, _ev, _window, cx| {
                        cx.emit(SearchEvent::OpenNote {
                            note_id: id.clone(),
                            title: t.clone(),
                        });
                    }),
                ));
            }
        }
        div()
            .size_full()
            .pt(safe.top)
            .bg(cx.theme().background)
            .child(
                div()
                    .id("search-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .child(col),
            )
    }
}

fn px_(v: f32) -> gpui_kit::Pixels {
    gpui_kit::px(v)
}

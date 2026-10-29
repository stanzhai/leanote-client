//! 斜杠面板（ADR-0011）：编辑器内键入 `/` 弹出的 Notion 风格 4×4 命令格。
//! 面板是唯一入口 —— 原 md_toolbar 与设置项 `editor_toolbar` 已删除。
//!
//! 16 项 = 14 个插入命令 + 图片 + 帮助。↑↓ 移动、Enter/Tab 执行、
//! Esc 关闭、继续键入过滤、Backspace 到头关闭。执行时先删除触发
//! 范围（`/` + 已输入过滤串），再 `insert_around(prefix, suffix)`。

use crate::app_shell::AppShell;
use crate::i18n::{self, Lang};
use gpui_base::input::EditorState;
use gpui_kit::component::Sizable;
use gpui_kit::prelude::*;
use gpui_kit::{
    deferred, div, px, rgb, AnyElement, App, AvailableSpace, Bounds, Element, GlobalElementId,
    InspectorElementId, IntoElement, KeyDownEvent, LayoutId, Pixels, Point, Size, Style,
    WeakEntity, Window,
};

/// 一条命令：图标、双语标签、插入前后缀。
pub(crate) struct Cmd {
    icon: gpui_kit::assets::IconName,
    zh: &'static str,
    en: &'static str,
    prefix: &'static str,
    suffix: &'static str,
    /// 特殊项：图片选择 / 帮助对话框。
    special: Special,
}

#[derive(PartialEq, Clone, Copy)]
enum Special {
    Insert,
    Image,
    Help,
}

/// ADR-0011 定死的 16 项，4×4。
pub(crate) const COMMANDS: [Cmd; 16] = [
    Cmd {
        icon: IconName::Heading1,
        zh: "一级标题",
        en: "Heading 1",
        prefix: "# ",
        suffix: "",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::Heading2,
        zh: "二级标题",
        en: "Heading 2",
        prefix: "## ",
        suffix: "",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::Heading3,
        zh: "三级标题",
        en: "Heading 3",
        prefix: "### ",
        suffix: "",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::Bold,
        zh: "粗体",
        en: "Bold",
        prefix: "**",
        suffix: "**",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::Italic,
        zh: "斜体",
        en: "Italic",
        prefix: "*",
        suffix: "*",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::Strikethrough,
        zh: "删除线",
        en: "Strikethrough",
        prefix: "~~",
        suffix: "~~",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::Code,
        zh: "行内代码",
        en: "Inline code",
        prefix: "`",
        suffix: "`",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::CodeXml,
        zh: "代码块",
        en: "Code block",
        prefix: "\n```\n",
        suffix: "\n```\n",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::Link,
        zh: "链接",
        en: "Link",
        prefix: "[",
        suffix: "](url)",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::List,
        zh: "无序列表",
        en: "Bullet list",
        prefix: "- ",
        suffix: "",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::ListOrdered,
        zh: "有序列表",
        en: "Numbered list",
        prefix: "1. ",
        suffix: "",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::Quote,
        zh: "引用",
        en: "Quote",
        prefix: "> ",
        suffix: "",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::Table,
        zh: "表格",
        en: "Table",
        prefix: "\n| a | b |\n|---|---|\n| ",
        suffix: " |\n",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::Minus,
        zh: "分隔线",
        en: "Divider",
        prefix: "\n---\n",
        suffix: "",
        special: Special::Insert,
    },
    Cmd {
        icon: IconName::Image,
        zh: "图片",
        en: "Image",
        prefix: "",
        suffix: "",
        special: Special::Image,
    },
    Cmd {
        icon: IconName::MessageCircleQuestionMark,
        zh: "语法帮助",
        en: "Syntax help",
        prefix: "",
        suffix: "",
        special: Special::Help,
    },
];

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;

/// 面板状态：过滤串（`/` 之后的字符）+ 选中索引。
#[derive(Default, Clone)]
pub(crate) struct SlashPanel {
    pub(crate) query: String,
    pub(crate) selected: usize,
}

impl SlashPanel {
    /// 过滤后的命令索引列表。
    pub(crate) fn matches(&self, lang: Lang) -> Vec<usize> {
        let q = self.query.to_lowercase();
        (0..COMMANDS.len())
            .filter(|&i| {
                q.is_empty()
                    || COMMANDS[i].zh.contains(&self.query)
                    || COMMANDS[i].en.to_lowercase().contains(&q)
                    || i18n::t(lang, i18n::Key::MarkdownHelpTitle)
                        .to_lowercase()
                        .contains(&q)
                        && COMMANDS[i].special == Special::Help
            })
            .collect()
    }
}

/// 键处理：面板打开时由 `capture_key_down` 先行调用。
/// `Some(consumed)`：已处理；consumued=false 表示放行（修饰键组合）。
/// `None`：面板应关闭（Esc / backspace 到头 / 过滤无结果 / 执行命令）。
/// 不直接改 `shell.slash_panel` —— 调用方持有克隆，写回会复活面板。
pub(crate) fn key(panel: &mut SlashPanel, ev: &KeyDownEvent, lang: Lang) -> Option<bool> {
    let matches = panel.matches(lang);
    let m = &ev.keystroke.modifiers;
    match ev.keystroke.key.as_str() {
        "escape" => None,
        // ↑↓ = 上下移动一行（COLS 列）；←→ = 左右移动一格。
        // 方向键在 kit 层是 Move* 动作（不是 key_down），由 app_shell
        // 的 capture_action 拦截后转成本函数调用（用户需求 2）。
        "up" | "down" | "left" | "right" if !m.platform && !m.control && !m.alt => {
            if !matches.is_empty() {
                let pos = matches.iter().position(|&i| i == panel.selected);
                let cur = pos.unwrap_or(0);
                let len = matches.len();
                let cols = COLS as usize;
                let next = match ev.keystroke.key.as_str() {
                    "down" => (cur + cols).min(len - 1),
                    "up" => cur.saturating_sub(cols),
                    "right" => (cur + 1) % len,
                    _ => (cur + len - 1) % len,
                };
                panel.selected = matches[next];
            }
            Some(true)
        }
        // Enter/Tab 执行：交由调用方（需要 window/cx 修改编辑器）。
        "enter" | "tab" if !m.platform && !m.control && !m.alt => Some(true),
        "backspace" => {
            if panel.query.is_empty() {
                None
            } else {
                panel.query.pop();
                Some(true)
            }
        }
        _ => {
            // 可打印字符继续过滤；非打印键放行。
            let ch = ev
                .keystroke
                .key_char
                .as_deref()
                .and_then(|s| s.chars().next());
            match ch {
                Some(c) if !c.is_control() && !m.platform && !m.control && !m.alt => {
                    panel.query.push(c);
                    let ms = panel.matches(lang);
                    if !ms.contains(&panel.selected) {
                        panel.selected = *ms.first().unwrap_or(&0);
                    }
                    if ms.is_empty() {
                        None
                    } else {
                        Some(true)
                    }
                }
                _ => Some(false),
            }
        }
    }
}

/// 执行选中命令。`/` 触发与过滤字符从未写入编辑器（capture_key_down
/// 一律吃掉，Esc 关闭才回填），所以这里不删任何文本，直接在当前光标
/// 插入（用户反馈 3：此前按 `1+query` 删了光标前 n 个字符，导致插入
/// 位置偏移、误删上一行内容）。
pub(crate) fn execute(
    shell: &mut AppShell,
    index: usize,
    window: &mut Window,
    cx: &mut gpui_kit::Context<AppShell>,
) {
    let cmd = &COMMANDS[index];
    match cmd.special {
        Special::Insert => shell.insert_around(cmd.prefix, cmd.suffix, window, cx),
        Special::Image => shell.pick_images(window, cx),
        Special::Help => shell.open_md_help(window, cx),
    }
    cx.notify();
}
/// 格尺寸常量（prepaint 面板尺寸计算与格子样式共用，防漂移）。
/// 图标 16px + 留白 → 28px 格（需求 3：面板随图标自适配）。
const CELL: f32 = 28.;
const GAP: f32 = 2.;
const PAD: f32 = 4.;
const COLS: u16 = 4;

/// 面板内容尺寸（需求 1）：宽随实际列数收紧，不按整行撑满；
/// 高 = 行数格 + 间隙 + 内边距 + 边框。panel_div 与 prepaint 共用。
fn panel_size(n: usize) -> (f32, f32) {
    let cols = n.min(COLS as usize) as f32;
    let rows = (n as f32 / COLS as f32).ceil();
    (
        cols * CELL + (cols - 1.) * GAP + 2. * PAD + 2.,
        rows * CELL + (rows - 1.) * GAP + 2. * PAD + 2.,
    )
}

/// 面板交互回调：格子点击执行、悬停同步选中（需求 1/2/4）。
/// 构造期由 render() 装好；paint 期每帧经 Rc 克隆进格子闭包。
pub(crate) struct SlashPanelHandlers {
    /// 点击第 index 格：执行该命令并关面板。
    pub(crate) on_click: Box<dyn Fn(usize, &mut Window, &mut App) + 'static>,
    /// 悬停第 index 格：把键盘选中移过去（hover = 选中，需求 2/4）。
    pub(crate) on_hover: Box<dyn Fn(usize, &bool, &mut Window, &mut App) + 'static>,
}

/// 面板内容：纯图标 4 列紧凑格。`matches` 为空时面板已关，不渲染。
fn panel_div(
    matches: &[usize],
    selected: usize,
    dark: bool,
    lang: Lang,
    viewport: Size<Pixels>,
    handlers: std::rc::Rc<SlashPanelHandlers>,
) -> AnyElement {
    let border = if dark { rgb(0x333344) } else { rgb(0xd8dee4) };
    let bg = if dark { rgb(0x1e1e2a) } else { rgb(0xffffff) };
    let fg = if dark { rgb(0xe0e0ea) } else { rgb(0x1a1a1a) };
    // 选中/hover 用强调色 + 白字，与常规菜单（MenuItem hover/selected）
    // 同一表现（用户需求 2）。
    let accent = crate::theme::accent();
    let accent_fg = crate::theme::accent_foreground();

    let cells: Vec<_> = matches
        .iter()
        .map(|&i| {
            let c = &COMMANDS[i];
            let click_handlers = handlers.clone();
            let hover_handlers = handlers.clone();
            let is_sel = i == selected;
            let tooltip_text = if lang == Lang::Zh { c.zh } else { c.en };
            div()
                .id(("slash-cell", i))
                .w(px(CELL))
                .h(px(CELL))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .bg(if is_sel { accent } else { bg.into() })
                .text_color(if is_sel { accent_fg } else { fg.into() })
                // hover 同强调色：与选中态同一观感（需求 2）。
                .hover(|s| s.bg(accent).text_color(accent_fg))
                .on_hover(move |hovered, window, cx| {
                    (hover_handlers.on_hover)(i, hovered, window, cx)
                })
                .on_click(move |_, window, cx| (click_handlers.on_click)(i, window, cx))
                // 需求 4：hover 立即显示功能提示（零延迟）。
                .tooltip_show_delay(std::time::Duration::ZERO)
                .tooltip(move |_window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip_text.to_string())
                        .build(_window, cx)
                })
                .child(Icon::new(c.icon).with_size(gpui_kit::component::Size::XSmall))
        })
        .collect();
    let grid = cells
        .into_iter()
        .fold(div().grid().grid_cols(COLS).gap(px(GAP)), |g, c| g.child(c));
    let (w, _) = panel_size(matches.len());
    div()
        .w(px(w))
        .p(px(PAD))
        .rounded(px(8.))
        .bg(bg)
        .border_1()
        .border_color(border)
        .shadow_md()
        .max_w(viewport.width)
        .max_h(viewport.height)
        .overflow_hidden()
        // 点击不穿透到编辑器（需求 1：点击即执行）。
        .occlude()
        .child(grid)
        .into_any_element()
}

/// 斜杠面板：锚定在光标下方的浮层。
///
/// 定位与绘制都放在 prepaint（不是 render）：锚点取编辑器当帧的
/// `prepaint_caret_geometry`（窗口坐标），面板经 `deferred()` 绘制，
/// 晚于编辑器 → 不被编辑器覆盖。上翻：光标靠近窗口底部时面板放光标上方。
pub(crate) struct SlashPanelElement {
    editor: WeakEntity<EditorState>,
    matches: Vec<usize>,
    selected: usize,
    dark: bool,
    lang: Lang,
    handlers: std::rc::Rc<SlashPanelHandlers>,
}

impl SlashPanelElement {
    pub(crate) fn new(
        editor: WeakEntity<EditorState>,
        panel: &SlashPanel,
        dark: bool,
        lang: Lang,
        handlers: std::rc::Rc<SlashPanelHandlers>,
    ) -> Self {
        Self {
            editor,
            matches: panel.matches(lang),
            selected: panel.selected,
            dark,
            lang,
            handlers,
        }
    }
}

impl Element for SlashPanelElement {
    type RequestLayoutState = ();
    type PrepaintState = AnyElement;

    fn id(&self) -> Option<gpui_kit::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (window.request_layout(Style::default(), [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let viewport = window.viewport_size();
        let content = panel_div(
            &self.matches,
            self.selected,
            self.dark,
            self.lang,
            viewport,
            self.handlers.clone(),
        );
        let Some(editor) = self.editor.upgrade() else {
            return content;
        };
        let Some(geo) = editor.read(cx).prepaint_caret_geometry() else {
            return content;
        };
        let caret = geo.cursor_bounds();
        let mut content = content;
        content.layout_as_root(
            Size {
                width: AvailableSpace::Definite(viewport.width),
                height: AvailableSpace::Definite(viewport.height),
            },
            window,
            cx,
        );
        // 面板宽随实际列数收紧（需求 1）：过滤后不足 4 项不再按
        // 整行撑满，icon 不稀疏。尺寸公式与 panel_div 共用。
        let (w, h) = panel_size(self.matches.len());
        let w = px(w);
        let h = px(h);
        // 默认在光标下方一行；窗口底部放不下时改到光标上方。
        let below = caret.origin.y + geo.line_height() + px(4.);
        let y = if below + h > viewport.height {
            caret.origin.y - h - px(4.)
        } else {
            below
        };
        let x = (caret.origin.x - px(4.))
            .min(viewport.width - w - px(8.))
            .max(px(8.));
        content.prepaint_at(Point::new(x, y), window, cx);
        content
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        content: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        content.paint(window, cx);
    }
}

impl IntoElement for SlashPanelElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// 渲染浮层（deferred，画在编辑器之上）。`matches` 为空时不渲染。
/// `shell`：AppShell 实体，格子点击/hover 回调经 `Entity::update` 回写
/// （panel 元素 paint 期只有 `App`，没有 `Context<AppShell>`）。
pub(crate) fn render(
    editor: &gpui_kit::Entity<EditorState>,
    panel: &SlashPanel,
    dark: bool,
    lang: Lang,
    shell: gpui_kit::Entity<AppShell>,
) -> Option<AnyElement> {
    if panel.matches(lang).is_empty() {
        return None;
    }
    let click_shell = shell.clone();
    let handlers = SlashPanelHandlers {
        on_click: Box::new(move |index, window, cx| {
            click_shell.update(cx, |this, cx| {
                crate::slash_panel::execute(this, index, window, cx);
                this.slash_panel = None;
                cx.notify();
            })
        }),
        on_hover: Box::new(move |index, hovered, _window, cx| {
            if !*hovered {
                return;
            }
            // hover 即选中：与方向键同一选中态（需求 2/4）。
            shell.update(cx, |this, cx| {
                if let Some(sp) = this.slash_panel.as_mut() {
                    if sp.selected != index {
                        sp.selected = index;
                        cx.notify();
                    }
                }
            })
        }),
    };
    let handlers = std::rc::Rc::new(handlers);
    Some(
        deferred(SlashPanelElement::new(
            editor.downgrade(),
            panel,
            dark,
            lang,
            handlers,
        ))
        .with_priority(1)
        .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 过滤：中文/英文子串、空查询全集、垃圾串关闭面板（ADR-0011）。
    #[test]
    fn matches_filters_by_substring() {
        let lang = Lang::En;
        let all = SlashPanel::default().matches(lang);
        assert_eq!(all.len(), COMMANDS.len());

        let mut p = SlashPanel::default();
        p.query = "表".into();
        let zh = p.matches(Lang::Zh);
        assert!(zh.iter().all(|&i| COMMANDS[i].zh.contains('表')));
        assert!(!zh.is_empty());

        p.query = "bold".into();
        let en = p.matches(lang);
        assert!(en
            .iter()
            .all(|&i| COMMANDS[i].en.to_lowercase().contains("bold")));
        assert_eq!(en.len(), 1);

        p.query = "zzz-no-such-cmd".into();
        assert!(p.matches(lang).is_empty());
    }

    fn key_ev(key: &str) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: gpui_kit::Keystroke {
                key: key.into(),
                ..Default::default()
            },
            is_held: false,
            prefer_character_input: false,
        }
    }

    /// 网格导航（用户需求 2）：↑↓ 行移（COLS 列）、←→ 环绕。
    #[test]
    fn arrow_keys_navigate_grid() {
        let mut p = SlashPanel::default();
        let lang = Lang::En;
        let all = p.matches(lang);

        // right: 0→1→2…
        crate::slash_panel::key(&mut p, &key_ev("right"), lang);
        assert_eq!(p.selected, all[1]);
        crate::slash_panel::key(&mut p, &key_ev("right"), lang);
        assert_eq!(p.selected, all[2]);
        // left 回退
        crate::slash_panel::key(&mut p, &key_ev("left"), lang);
        assert_eq!(p.selected, all[1]);
        // down 跨一行 = COLS 格
        crate::slash_panel::key(&mut p, &key_ev("down"), lang);
        assert_eq!(p.selected, all[1 + COLS as usize]);
        // up 回到第一行
        crate::slash_panel::key(&mut p, &key_ev("up"), lang);
        assert_eq!(p.selected, all[1]);
        // right 环绕到最后（0 → len-1 不经 0，从 1 → 2）
        let mut q = SlashPanel::default();
        q.selected = all[all.len() - 1];
        crate::slash_panel::key(&mut q, &key_ev("right"), lang);
        assert_eq!(q.selected, all[0]);
        // up 在第一行不动
        let mut r = SlashPanel::default();
        crate::slash_panel::key(&mut r, &key_ev("up"), lang);
        assert_eq!(r.selected, all[0]);
        // down 最后一行钳制在末项
        let mut s = SlashPanel::default();
        s.selected = all[all.len() - 1];
        crate::slash_panel::key(&mut s, &key_ev("down"), lang);
        assert_eq!(s.selected, all[all.len() - 1]);
    }

    /// Esc 关闭（None），调用方负责回填 `/`+query（用户需求 3）。
    #[test]
    fn escape_closes_without_consuming_as_filter() {
        let mut p = SlashPanel::default();
        p.query = "hea".into();
        assert_eq!(
            crate::slash_panel::key(&mut p, &key_ev("escape"), Lang::En),
            None
        );
    }
}

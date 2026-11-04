//! 历史版本对话框（左右分栏）：左侧窄版本列表（服务端优先，本地回退），
//! 右侧选中版本 ↔ 当前内容的 git 式 unified diff（kit Diff 组件），右上
//! 差异标题 + 左上「恢复此版本」按钮（AlertDialog 确认后覆盖当前笔记）。

use gpui_kit::component::button::Button;
use gpui_kit::component::diff::{Diff, DiffFile, DiffMode, DiffState};
use gpui_kit::component::label::Label;
use gpui_kit::component::WindowExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{px, Context, Entity, IntoElement, ParentElement, Render, SharedString, Window};

use crate::app_shell::{fmt_time, AppShell};
use crate::i18n::{self, Lang};
use crate::theme;

/// One selectable version. `local_id` is `Some` for local `note_history`
/// rows (restore also restores the old title); server rows restore
/// content-only (the server keeps no title per version).
#[derive(Clone)]
pub(crate) struct HistoryVersion {
    pub time: i64,
    pub content: String,
    pub local_id: Option<leanote_core::Id>,
}

pub(crate) struct HistoryDialog {
    note_id: leanote_core::Id,
    title: SharedString,
    current: String,
    versions: Vec<HistoryVersion>,
    selected: usize,
    lang: Lang,
    shell: Entity<AppShell>,
    diff: Entity<DiffState>,
}

/// Unified patch old→new, written directly (like the kit's diff fixtures):
/// one full-context hunk; `similar`'s unified writer loses CRLF endings.
fn unified_patch(old: &str, new: &str, path: &str) -> String {
    use similar::{ChangeTag, TextDiff};
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new.split_inclusive('\n').collect();
    let quoted = format!("\"{path}\"");
    let diff = TextDiff::from_slices(&old_lines, &new_lines);
    let mut patch = format!("--- {quoted}\n+++ {quoted}\n");
    if !old_lines.is_empty() || !new_lines.is_empty() {
        patch.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            usize::from(!old_lines.is_empty()),
            old_lines.len(),
            usize::from(!new_lines.is_empty()),
            new_lines.len()
        ));
        for op in diff.ops() {
            for change in diff.iter_changes(op) {
                patch.push(match change.tag() {
                    ChangeTag::Equal => ' ',
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                });
                patch.push_str(change.value());
                if !change.value().ends_with('\n') {
                    patch.push_str("\n\\ No newline at end of file\n");
                }
            }
        }
    }
    patch
}

impl HistoryDialog {
    pub(crate) fn new(
        note_id: leanote_core::Id,
        title: SharedString,
        current: String,
        versions: Vec<HistoryVersion>,
        lang: Lang,
        shell: Entity<AppShell>,
        cx: &mut Context<Self>,
    ) -> Self {
        let diff = Self::build_diff(&title, &current, versions.first(), cx);
        Self {
            note_id,
            title,
            current,
            versions,
            selected: 0,
            lang,
            shell,
            diff,
        }
    }

    fn build_diff(
        title: &str,
        current: &str,
        version: Option<&HistoryVersion>,
        cx: &mut Context<Self>,
    ) -> Entity<DiffState> {
        let old = version.map(|v| v.content.as_str()).unwrap_or("");
        let patch = unified_patch(old, current, &format!("{title}.md"));
        let file = DiffFile::parse(&patch)
            .ok()
            .and_then(|mut files| files.pop())
            .map(|f| f.with_language("markdown"));
        cx.new(|cx| DiffState::new(file.into_iter(), cx).with_mode(DiffMode::Unified))
    }

    fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.versions.len() || ix == self.selected {
            return;
        }
        self.selected = ix;
        self.diff = Self::build_diff(&self.title, &self.current, self.versions.get(ix), cx);
        cx.notify();
    }
}

impl Render for HistoryDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = |k: i18n::Key| i18n::t(self.lang, k);
        let dark = self.shell.read(cx).dark(cx);
        let muted = theme::muted_foreground(dark);
        let accent = theme::accent();
        let accent_fg = theme::accent_foreground();
        let nav_fg = theme::foreground(dark);

        // Left: narrow version list (newest first), settings-nav row look.
        let me = cx.entity();
        let mut list = gpui_kit::base::v_flex()
            .id("history-version-list")
            .w(px(190.))
            .h_full()
            .gap_1()
            .p_2()
            .border_r_1()
            .border_color(theme::border(dark))
            .overflow_y_scroll();
        if self.versions.is_empty() {
            list = list.child(
                Label::new(t(i18n::Key::HistoryEmpty))
                    .text_size(theme::ui(12.))
                    .text_color(muted),
            );
        }
        for (ix, v) in self.versions.iter().enumerate() {
            let active = ix == self.selected;
            let row_entity = me.clone();
            list = list.child(
                gpui_kit::div()
                    .id(("history-version", ix))
                    .w_full()
                    .px_2()
                    .py_1p5()
                    .rounded(px(theme::RADIUS_SELECTION))
                    .when(active, |el| el.bg(accent))
                    .on_click(move |_, _, cx| {
                        let _ = row_entity.update(cx, |this, cx| this.select(ix, cx));
                    })
                    .child(
                        gpui_kit::div()
                            .text_size(theme::ui(12.))
                            .text_color(if active { accent_fg } else { nav_fg })
                            .child(fmt_time(v.time)),
                    ),
            );
        }

        // Right: restore (top-left) + diff caption, then the git-style diff.
        let version = self.versions.get(self.selected).cloned();
        let note_id = self.note_id.clone();
        let shell = self.shell.clone();
        let lang = self.lang;
        let mut right = gpui_kit::base::v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_2()
            .p_2();
        if let Some(v) = version {
            right = right.child(
                gpui_kit::base::h_flex()
                    .gap_2()
                    .child(
                        Button::new("history-restore")
                            .label(t(i18n::Key::RestoreVersion).to_string())
                            .outline()
                            .on_click(move |_ev, window, cx| {
                                let shell = shell.clone();
                                let v = v.clone();
                                let note_id = note_id.clone();
                                let dark = shell.read(cx).dark(cx);
                                window.open_alert_dialog(cx, move |alert, _w, _cx| {
                                    let shell_ok = shell.clone();
                                    let v_ok = v.clone();
                                    let note_ok = note_id.clone();
                                    alert
                                        .bg(theme::material(theme::Material::Popover, dark))
                                        .title(i18n::t(lang, i18n::Key::RestoreConfirmTitle))
                                        .description(i18n::t(lang, i18n::Key::RestoreConfirmBody))
                                        .show_cancel(true)
                                        .ok_text(i18n::t(lang, i18n::Key::Confirm))
                                        .cancel_text(i18n::t(lang, i18n::Key::Cancel))
                                        .on_ok(move |_ev, window, cx| {
                                            window.close_all_dialogs(cx);
                                            let _ = shell_ok.update(cx, |this, cx| {
                                                this.restore_history_pick(
                                                    &note_ok,
                                                    v_ok.clone(),
                                                    window,
                                                    cx,
                                                )
                                            });
                                            true
                                        })
                                });
                            }),
                    )
                    .child(
                        Label::new(t(i18n::Key::DiffVsCurrent))
                            .text_size(theme::ui(12.))
                            .text_color(muted),
                    ),
            );
        }
        right = right.child(
            gpui_kit::div()
                .id("history-diff")
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .rounded(px(theme::RADIUS_SELECTION))
                .border_1()
                .border_color(theme::border(dark))
                .child(Diff::new(&self.diff)),
        );

        gpui_kit::base::h_flex()
            .w_full()
            .h(px(460.))
            .child(list)
            .child(right)
    }
}

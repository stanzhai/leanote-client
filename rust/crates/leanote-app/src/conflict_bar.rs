//! Conflict adjudication bar (ticket 14): when the selected note has an
//! unresolved sync conflict, a bar at the top of the right pane shows both
//! versions side by side with three actions — 用本地 / 用服务器 / 两边保留.

use crate::app_shell::fmt_time;
use crate::i18n::{self, Lang};
use crate::theme::{self, Material};
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::label::Label;
use gpui_kit::prelude::*;
use gpui_kit::{px, Hsla};
use leanote_core::sync::ConflictRow;

/// Which conflict the bar shows, if any. Selection-first: the bar belongs to
/// the note being read; with no selection (or one on an unrelated note) we
/// still show the oldest conflict — nothing else would ever surface it.
pub(crate) fn bar_state<'a>(
    conflicts: &'a [ConflictRow],
    selected: Option<&str>,
) -> Option<&'a ConflictRow> {
    match selected {
        Some(id) => conflicts
            .iter()
            .find(|c| c.local_note_id == id)
            .or_else(|| conflicts.first()),
        None => conflicts.first(),
    }
}

/// One-line content excerpt for the bar: newlines → spaces, markdown '#'
/// markers dropped, 80 chars.
pub(crate) fn excerpt(content: &str) -> String {
    content
        .replace('\n', " ")
        .replace('\r', " ")
        .replace('#', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(80)
        .collect()
}

/// Localized `（本地副本）` / ` (local copy)` suffix for keep-both.
pub(crate) fn keep_both_suffix(lang: Lang) -> &'static str {
    match lang {
        Lang::Zh => "（本地副本）",
        Lang::En => " (local copy)",
    }
}

fn small(text: &str, color: Hsla) -> Label {
    Label::new(text).text_size(px(11.)).text_color(color)
}

/// One version column of the bar.
fn version(label: &str, title: &str, content: &str, time: &str, fg: Hsla, muted: Hsla) -> gpui_kit::Div {
    v_flex()
        .flex_1()
        .min_w_0()
        .gap_1()
        .child(small(label, muted))
        .child(Label::new(title).text_size(px(13.)).text_color(fg))
        .child(
            Label::new(format!("{} · {}", excerpt(content), time))
                .text_size(px(12.))
                .text_color(muted),
        )
}

/// The bar element. Handlers receive the server note id via the closures the
/// caller builds (see `AppShell::render_reader`).
pub(crate) fn render(
    dark: bool,
    lang: Lang,
    c: &ConflictRow,
    on_use_local: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
    on_use_server: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
    on_keep_both: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
) -> impl IntoElement {
    let fg = theme::foreground(dark);
    let muted = theme::muted_foreground(dark);
    let t = move |k: i18n::Key| i18n::t(lang, k);

    v_flex()
        .gap_2()
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(theme::border(dark))
        .bg(theme::material(Material::Popover, dark))
        .child(
            Label::new(t(i18n::Key::ConflictTitle))
                .text_size(px(13.))
                .text_color(fg),
        )
        .child(
            h_flex()
                .gap_3()
                .child(version(
                    t(i18n::Key::ConflictLocal),
                    &c.local_title,
                    &c.local_content,
                    &fmt_time(c.local_updated_ms),
                    fg,
                    muted,
                ))
                .child(version(
                    t(i18n::Key::ConflictServer),
                    &c.server_title,
                    &c.server_content,
                    &fmt_time(c.server_updated_ms),
                    fg,
                    muted,
                )),
        )
        .child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("conflict-use-local")
                        .label(t(i18n::Key::UseLocal))
                        .primary()
                        .on_click(on_use_local),
                )
                .child(
                    Button::new("conflict-use-server")
                        .label(t(i18n::Key::UseServer))
                        .outline()
                        .on_click(on_use_server),
                )
                .child(
                    Button::new("conflict-keep-both")
                        .label(t(i18n::Key::KeepBoth))
                        .ghost()
                        .on_click(on_keep_both),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(local: &str, server: &str) -> ConflictRow {
        ConflictRow {
            server_note_id: server.to_string(),
            local_note_id: local.to_string(),
            server_title: format!("srv {server}"),
            server_content: "server".into(),
            server_updated_ms: 1,
            local_title: format!("loc {local}"),
            local_content: "local".into(),
            local_updated_ms: 2,
        }
    }

    #[test]
    fn bar_shows_selected_notes_conflict() {
        let cs = vec![row("n1", "s1"), row("n2", "s2")];
        assert_eq!(bar_state(&cs, Some("n2")).unwrap().server_note_id, "s2");
        assert_eq!(bar_state(&cs, Some("n1")).unwrap().server_note_id, "s1");
    }

    #[test]
    fn bar_falls_back_to_oldest_when_selection_unrelated_or_none() {
        let cs = vec![row("n1", "s1"), row("n2", "s2")];
        assert_eq!(bar_state(&cs, Some("zz")).unwrap().server_note_id, "s1");
        assert_eq!(bar_state(&cs, None).unwrap().server_note_id, "s1");
    }

    #[test]
    fn no_conflicts_no_bar() {
        let cs: Vec<ConflictRow> = vec![];
        assert!(bar_state(&cs, Some("n1")).is_none());
        assert!(bar_state(&cs, None).is_none());
    }

    #[test]
    fn excerpt_flattens_and_truncates() {
        assert_eq!(excerpt("# Hello\n\nWorld"), "Hello World");
        assert_eq!(excerpt("").len(), 0);
        let long: String = "x".repeat(200);
        assert_eq!(excerpt(&long).len(), 80);
        // CJK-safe: chars not bytes
        let cjk: String = "中".repeat(100);
        assert_eq!(excerpt(&cjk).chars().count(), 80);
    }

    #[test]
    fn keep_both_suffix_localizes() {
        assert_eq!(keep_both_suffix(Lang::Zh), "（本地副本）");
        assert_eq!(keep_both_suffix(Lang::En), " (local copy)");
    }
}

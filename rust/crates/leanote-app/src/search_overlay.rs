//! Ticket 13: ⌘K global search overlay (ADR-0003, Wake-style).
//!
//! 680px dialog centered-top, hosting the kit's command palette: borderless
//! query input, result list, shortcut footnote. Typing runs the unified
//! ticket-05 core search (`Store::search` → `search_notes`); rows show
//! title + excerpt in the exact CLI order and excerpt form (`format_rows`
//! mirrors `leanote-cli` cmd_search). ↑↓ moves the selection, ↩ opens the
//! note in the right pane, esc closes (empty query first).
//!
//! IME safety is structural, not gated here: the input engine only emits
//! `InputEvent::Change` on committed text — the preedit path
//! (`replace_and_mark_text_in_range`, gpui-base state.rs) never emits it —
//! and `CommandState::on_query` fires solely on `InputEvent::Change`. So no
//! search runs while a composition is in progress. `should_search` encodes
//! the same rule as a pure, unit-tested predicate.
use crate::app_shell::AppShell;
use crate::i18n;
use crate::theme;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::label::Label;
use gpui_kit::component::{IndexPath, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::{px, App, Context, Entity, Window};
use leanote_core::SearchHit;

gpui_kit::actions!(
    leanote,
    [
        ToggleSearch,
        ToggleFocusMode,
        ToggleFullPreview,
        OpenSettings
    ]
);
// Issue 17 Q6 quick actions: new note / sync / export backup / vim.
gpui_kit::actions!(
    leanote,
    [
        QuickNewNote,
        QuickSyncNow,
        QuickToggleVim,
        QuickExportBackup
    ]
);

/// ADR-0003 fixed overlay width.
pub const OVERLAY_W: f32 = 680.;
/// Result cap — the CLI's default surface (`-N 20`).
pub const RESULT_LIMIT: usize = 20;

// ---------------------------------------------------------------------------
// Pure logic — headless, unit-tested.
// ---------------------------------------------------------------------------

/// One result row, formatted for display exactly like the CLI hit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultRow {
    pub note_id: String,
    /// Title, `(untitled)` when empty — same rule as cmd_search.
    pub title: String,
    /// Excerpt verbatim from the core (already windowed, `\n` → space).
    pub excerpt: String,
    /// Notebook title — first row's right cluster (`{notebook} · {date}`).
    pub notebook: String,
    /// Updated date `YYYY-MM-DD`, same as the app's list rows.
    pub date: String,
}

/// Format hits into rows, notebook titles resolved up front.
pub fn format_rows(hits: &[SearchHit], notebook_of: impl Fn(&str) -> String) -> Vec<ResultRow> {
    hits.iter()
        .map(|h| {
            let n = &h.note;
            ResultRow {
                note_id: n.note_id.to_string(),
                title: if n.title.is_empty() {
                    "(untitled)".into()
                } else {
                    n.title.clone()
                },
                excerpt: h.excerpt.clone(),
                notebook: notebook_of(&n.notebook_id.to_string()),
                date: crate::app_shell::fmt_time(n.updated_time),
            }
        })
        .collect()
}

/// Search gate: run a search only for committed, non-empty queries.
///
/// `composing` = an IME preedit is active; the engine never delivers a
/// Change for those, so this predicate documents the same contract.
pub fn should_search(query: &str, composing: bool) -> bool {
    !composing && !query.is_empty()
}

/// ⌘K / Ctrl+K binding predicate.
#[cfg_attr(not(test), allow(dead_code))] // binding itself is declarative (main.rs)
pub fn is_cmd_k(platform_cmd: bool, ctrl: bool, key: &str) -> bool {
    key == "k" && (platform_cmd || ctrl)
}

// ---------------------------------------------------------------------------
// Overlay — owned by AppShell, rendered inside a Dialog.
// ---------------------------------------------------------------------------

pub struct SearchOverlay {
    state: Entity<CommandState>,
}

impl SearchOverlay {
    pub fn new(window: &mut Window, cx: &mut Context<AppShell>) -> Self {
        Self {
            state: cx.new(|cx| CommandState::new(window, cx)),
        }
    }

    /// ⌘K: open when no dialog is up, close otherwise. Works in any phase —
    /// the dialog layer sits above the shell, not inside a pane.
    ///
    /// The builder may re-run; `rows` are re-read from the shell entity each
    /// time so a query typed while open re-renders the list in place.
    pub fn toggle(&self, shell: &Entity<AppShell>, window: &mut Window, cx: &mut App) {
        if window.has_active_dialog(cx) {
            window.close_dialog(cx);
            return;
        }
        let state = self.state.clone();
        let shell = shell.clone();
        let first_focus = std::cell::Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if first_focus.replace(false) {
                state.update(cx, |s, cx| s.focus(window, cx));
            }
            render_dialog(dialog, window, cx, &state, &shell)
        });
    }
}

/// The dialog frame + palette with AppShell-owned callbacks wired in.
/// Issue 17 Q6: three phases — empty query shows quick actions + local
/// recents; a typed query shows core search results.
fn render_dialog(
    dialog: Dialog,
    _window: &mut Window,
    cx: &mut App,
    state: &Entity<CommandState>,
    shell: &Entity<AppShell>,
) -> Dialog {
    let lang = shell.read(cx).lang();
    let dark = shell.read(cx).dark(cx);
    let muted = theme::muted_foreground(dark);
    let t_footnote = i18n::t(lang, i18n::Key::SearchFootnote);

    let shell_q = shell.clone();
    let shell_c = shell.clone();
    let query = state.read(cx).query(cx);
    let searching = crate::search_overlay::should_search(&query, false);

    let mut palette = Command::new(state)
        .searchable(true)
        .filterable(false) // ranking comes from the core, order is fixed
        .bordered(false) // the dialog draws the frame
        .max_h(px(420.))
        .placeholder(i18n::t(lang, i18n::Key::SearchPlaceholder))
        .empty(move |state, _, cx| {
            // Empty input → how-to hint; typed query with no hits → no-results.
            let key = if state.query(cx).is_empty() {
                i18n::Key::SearchEmptyHint
            } else {
                i18n::Key::SearchNoResults
            };
            v_flex()
                .py_6()
                .w_full()
                .items_center()
                .child(
                    Label::new(i18n::t(lang, key))
                        .text_size(px(13.))
                        .text_color(muted),
                )
                .into_any_element()
        })
        .footer(move |_, _, _| {
            h_flex()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(theme::border(dark))
                .child(Label::new(t_footnote).text_size(px(11.)).text_color(muted))
                .into_any_element()
        })
        .on_query(move |query, _window, cx| {
            // Fires only on committed text (IME-safe — see module docs).
            shell_q.update(cx, |shell, cx| shell.run_overlay_search(query, cx));
        })
        .on_confirm(move |index: IndexPath, window, cx| {
            shell_c.update(cx, |shell, cx| shell.confirm_overlay(index, window, cx));
        });

    if searching {
        let rows = shell.read(cx).overlay_rows().to_vec();
        let group = CommandGroup::new().items(rows.iter().map(|r| {
            CommandItem::new().child({
                let r = r.clone();
                let muted = muted;
                move |_: &mut Window, _: &mut App| {
                    // Two aligned lines: title left, notebook · date right
                    // (flex-none so the time column stays flush), excerpt
                    // below, single line, ellipsized.
                    v_flex()
                        .gap_0p5()
                        .py_1()
                        .w_full()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    gpui_kit::div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .child(Label::new(r.title.clone()).text_size(px(14.))),
                                )
                                .child(
                                    h_flex()
                                        .flex_none()
                                        .gap_1()
                                        .child(
                                            Label::new(r.notebook.clone())
                                                .text_size(px(11.))
                                                .text_color(muted),
                                        )
                                        .child(
                                            Label::new(format!("· {}", r.date))
                                                .text_size(px(11.))
                                                .text_color(muted),
                                        ),
                                ),
                        )
                        .child(
                            gpui_kit::div().w_full().min_w_0().truncate().child(
                                Label::new(r.excerpt.clone())
                                    .text_size(px(12.))
                                    .text_color(muted),
                            ),
                        )
                        .into_any_element()
                }
            })
        }));
        palette = palette.group(group);
    } else {
        // Quick actions (issue 17 Q6) — each dispatches an action the root
        // div handles (root on_action handlers, app_shell.rs).
        let actions_group = CommandGroup::new()
            .label(i18n::t(lang, i18n::Key::QuickActions))
            .items(
                [
                    (
                        Box::new(QuickNewNote) as Box<dyn gpui_kit::Action>,
                        i18n::t(lang, i18n::Key::NewNote),
                    ),
                    (
                        Box::new(QuickExportBackup) as Box<dyn gpui_kit::Action>,
                        i18n::t(lang, i18n::Key::ActionExportBackup),
                    ),
                    (
                        Box::new(QuickToggleVim) as Box<dyn gpui_kit::Action>,
                        i18n::t(lang, i18n::Key::VimOn),
                    ),
                    (
                        Box::new(QuickSyncNow) as Box<dyn gpui_kit::Action>,
                        i18n::t(lang, i18n::Key::SyncNow),
                    ),
                ]
                .into_iter()
                .map(|(act, label)| CommandItem::new().label(label).action(act)),
            );
        palette = palette.group(actions_group);

        // Recents — local, from prefs (issue 17 Q6). Confirmed via
        // on_confirm: empty-query section 1 is this group.
        let recents = shell.read(cx).recent_rows();
        if !recents.is_empty() {
            let recents_group = CommandGroup::new()
                .label(i18n::t(lang, i18n::Key::Recents))
                .items(
                    recents
                        .iter()
                        .map(|(_, title)| CommandItem::new().label(title.clone())),
                );
            palette = palette.group(recents_group);
        }
    }

    dialog
        .w(px(OVERLAY_W))
        .margin_top(px(80.))
        .overlay(true)
        .keyboard(true)
        .p_0()
        .child(palette)
}

#[cfg(test)]
mod tests {
    use super::*;
    use leanote_core::{Id, Note};

    fn note(id: &str, title: &str, content: &str, tags: &[&str], nb: &str, updated: i64) -> Note {
        Note {
            note_id: Id(id.into()),
            user_id: "user1".into(),
            notebook_id: Id(nb.into()),
            title: title.into(),
            content: content.into(),
            desc: String::new(),
            img_src: String::new(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            abstract_: String::new(),
            is_markdown: false,
            is_trash: false,
            is_dirty: false,
            is_new: false,
            is_deleted: false,
            is_blog: false,
            star: false,
            created_time: updated,
            updated_time: updated,
        }
    }

    #[test]
    fn format_rows_mirrors_cli_layout() {
        // Row fields for the two-line ⌘K layout: title / excerpt /
        // notebook / date.
        let hits = leanote_core::search_notes(
            &[note(
                "000000000000000000000001",
                "Leanote intro",
                "leanote is a note app with markdown support",
                &["rust", "notes"],
                "nb1",
                1_735_235_200_000,
            )],
            "leanote",
            20,
        );
        let rows = format_rows(&hits, |id| format!("Notebook-{id}"));
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.note_id, "000000000000000000000001");
        assert_eq!(r.title, "Leanote intro");
        assert_eq!(r.excerpt, "leanote is a note app with markdown support");
        assert_eq!(r.notebook, "Notebook-nb1");
        assert_eq!(r.date, "2024-12-26"); // UTC days for that epoch ms
    }

    #[test]
    fn format_rows_untitled_and_no_tags() {
        let hits = leanote_core::search_notes(
            &[note(
                "000000000000000000000002",
                "",
                "body mentions zebra",
                &[],
                "nb2",
                0,
            )],
            "zebra",
            20,
        );
        let rows = format_rows(&hits, |_| "Inbox".into());
        assert_eq!(rows[0].title, "(untitled)");
        assert_eq!(rows[0].notebook, "Inbox");
    }

    #[test]
    fn should_search_gates_composition_and_empty() {
        assert!(should_search("leanote", false));
        assert!(
            !should_search("", false),
            "empty query → hint state, no search"
        );
        assert!(!should_search("ле", true), "preedit in flight → no search");
        assert!(should_search("ле", false), "committed → search");
    }

    #[test]
    fn is_cmd_k_matches_platform_modifier() {
        // macOS: ⌘K (platform) or Ctrl+K; others: Ctrl+K only.
        assert!(is_cmd_k(true, false, "k"));
        assert!(is_cmd_k(false, true, "k"));
        assert!(!is_cmd_k(false, false, "k"), "bare k must not toggle");
        assert!(!is_cmd_k(true, false, "j"));
    }
}

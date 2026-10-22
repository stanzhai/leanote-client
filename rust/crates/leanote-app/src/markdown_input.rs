//! Issue 21: editor input assists — list continuation on Enter, pair
//! closing / fence completion on bare keys, and list-line Tab/Shift-Tab
//! indentation. All operate on the kit `EditorState` public API
//! (selected_range / set_selected_range / insert / replace) so edits
//! flow through the normal edit pipeline (undo + InputEvent::Change →
//! autosave).
//!
//! The kit's markdown auto-closing pairs are disabled at startup
//! (`set_language_config("markdown", …auto_closing_pairs([]))`) so only
//! this module inserts closers.

use gpui_kit::component::input::{EditorState, Rope, RopeExt};
use gpui_kit::{Context, Window};

/// Marker the NEXT list line should start with (indent excluded).
/// Ordered lists increment; `- [x]` reverts to `- [ ]`; `- ` / `* ` /
/// `+ ` / `> ` repeat. `None` = not a list/quote line.
pub(crate) fn next_list_marker(line: &str) -> Option<String> {
    let rest = line.trim_start_matches([' ', '\t']);
    let m = if rest.starts_with("- [ ] ") || rest.starts_with("- [x] ") {
        "- [ ] "
    } else if rest.starts_with("- ") || rest.starts_with("* ") || rest.starts_with("+ ") {
        &rest[..2]
    } else if rest.starts_with("> ") {
        "> "
    } else {
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() && rest[digits.len()..].starts_with(". ") {
            let n: u64 = digits.parse().ok()?;
            return Some(format!("{}. ", n + 1));
        } else {
            return None;
        }
    };
    Some(m.to_string())
}

/// Length of the list marker at the start of `rest` (indent stripped):
/// `1. ` → 3, `- [x] ` → 6, `- ` → 2. 0 = no marker.
fn marker_len(rest: &str) -> usize {
    if rest.starts_with("- [ ] ") || rest.starts_with("- [x] ") {
        6
    } else if rest.starts_with("- ")
        || rest.starts_with("* ")
        || rest.starts_with("+ ")
        || rest.starts_with("> ")
    {
        2
    } else {
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && rest[digits..].starts_with(". ") {
            digits + 2
        } else {
            0
        }
    }
}

/// (row, line text, line-start offset, indent length, marker length) at
/// `cursor`. Marker 0 = plain line.
fn line_parts(text: &str, cursor: usize) -> (usize, String, usize, usize, usize) {
    let rope = Rope::from(text);
    let point = rope.offset_to_point(cursor);
    let line = rope.slice_line(point.row).to_string();
    let line_start = rope.line_start_offset(point.row);
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let mlen = marker_len(&line[indent..]);
    (point.row, line, line_start, indent, mlen)
}

/// Enter (insert mode / vim off): continue the list prefix on the new
/// line; an empty item drops its prefix and ends the list. Returns true
/// when consumed (caller stops propagation — the kit's plain enter
/// never runs).
pub(crate) fn enter_continue_list(
    state: &mut EditorState,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) -> bool {
    let range = state.selected_range();
    if range.start != range.end {
        return false; // selection replace is the kit's job
    }
    let cursor = range.start;
    let text = state.value().to_string();
    let (_, line, line_start, indent, mlen) = line_parts(&text, cursor);
    if mlen == 0 {
        return false;
    }
    let rest = &line[indent..];
    let item_body = &rest[mlen..];
    if item_body.trim().is_empty() {
        // Empty item: remove prefix (and its indent), keep the newline.
        state.set_selected_range(line_start..line_start + indent + mlen, cx);
        state.replace("", window, cx);
        return true;
    }
    // Continue: newline + indent + next marker, cursor after it.
    let next = next_list_marker(&line).unwrap_or_default();
    state.set_selected_range(cursor..cursor, cx);
    state.insert(format!("\n{}{}", &line[..indent], next), window, cx);
    true
}

/// Bare-key pair closing (insert mode / vim off). `*`/`~` only trigger
/// doubled (`**`/`~~`); the third backtick on a bare ``` line opens a
/// fence; `[`/`(` close themselves (selection wrapped when present).
/// Returns true when handled.
pub(crate) fn close_pair_on_key(
    state: &mut EditorState,
    key_char: char,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) -> bool {
    let range = state.selected_range();
    let (start, end) = (range.start.min(range.end), range.start.max(range.end));
    let text = state.value().to_string();
    let prev = if start > 0 {
        text[..start].chars().next_back()
    } else {
        None
    };
    let (open, close) = match key_char {
        '*' if prev == Some('*') => ("**", "**"),
        '~' if prev == Some('~') => ("~~", "~~"),
        '`' => {
            let (_, line, line_start, _, _) = line_parts(&text, start);
            if line.trim() == "``" {
                // Third backtick turns ``` into an open fence with its
                // closer; cursor on the blank line inside.
                state.set_selected_range(line_start..line_start + 2, cx);
                state.replace("```\n\n```", window, cx);
                state.set_selected_range(line_start + 4..line_start + 4, cx);
                return true;
            }
            ("`", "`")
        }
        '[' => ("[", "]"),
        '(' => ("(", ")"),
        _ => return false,
    };
    if start != end {
        let selected = text[start..end].to_string();
        state.set_selected_range(start..end, cx);
        state.replace(format!("{open}{selected}{close}"), window, cx);
        let s = start + open.len();
        state.set_selected_range(s..s + selected.len(), cx);
    } else {
        state.insert(format!("{open}{close}"), window, cx);
        let c = state.selected_range().start - close.len();
        state.set_selected_range(c..c, cx);
    }
    true
}

/// Tab/Shift-Tab on a list line: indent/outdent the line start by two
/// spaces. Non-list lines fall through to the kit default. Returns true
/// when consumed.
pub(crate) fn indent_list_line(
    state: &mut EditorState,
    outdent: bool,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) -> bool {
    let range = state.selected_range();
    let cursor = range.start.min(range.end);
    let text = state.value().to_string();
    let (_, _line, line_start, indent, mlen) = line_parts(&text, cursor);
    if mlen == 0 {
        return false;
    }
    if outdent {
        if indent == 0 {
            return false; // at column 0: kit default (outdent whole block)
        }
        let cut = indent.min(2);
        state.set_selected_range(line_start..line_start + cut, cx);
        state.replace("", window, cx);
    } else {
        state.set_selected_range(line_start..line_start, cx);
        state.insert("  ", window, cx);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers() {
        assert_eq!(next_list_marker("1. a"), Some("2. ".into()));
        assert_eq!(next_list_marker("9. a"), Some("10. ".into()));
        assert_eq!(next_list_marker("- a"), Some("- ".into()));
        assert_eq!(next_list_marker("* a"), Some("* ".into()));
        assert_eq!(next_list_marker("+ a"), Some("+ ".into()));
        assert_eq!(next_list_marker("> q"), Some("> ".into()));
        assert_eq!(next_list_marker("- [ ] t"), Some("- [ ] ".into()));
        assert_eq!(next_list_marker("- [x] t"), Some("- [ ] ".into()));
        assert_eq!(next_list_marker("  - nested"), Some("- ".into()));
        assert_eq!(next_list_marker("plain"), None);
        assert_eq!(next_list_marker("# head"), None);
    }

    #[test]
    fn marker_lengths() {
        assert_eq!(marker_len("1. a"), 3);
        assert_eq!(marker_len("- [x] a"), 6);
        assert_eq!(marker_len("- a"), 2);
        assert_eq!(marker_len("> a"), 2);
        assert_eq!(marker_len("text"), 0);
    }
}

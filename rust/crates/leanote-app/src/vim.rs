//! Vim mode for the kit editor, following rgpui's `editor/vim` architecture
//! (ADR-0004: keybinding layer, not an engine fork; ADR-0005: cursor shape
//! is the mode indicator — no status line).
//!
//! Three layers:
//!
//! - **Grammar** (`VimState::handle_key`): pure state machine, key →
//!   [`VimAction`]; unit-tested headless.
//! - **Buffer math**: rgpui-ported rope helpers (UTF-8 byte offsets,
//!   char-count columns so CJK aligns, preferred-column memory).
//! - **Execution** ([`exec`]/[`consume`]): mutates `EditorState` directly
//!   (`set_selected_range`/`replace`/`insert`) — no kit-action round-trips,
//!   so no dispatch-reentry guard. The one exception is Undo:
//!   `InputBaseState::undo` is `pub(super)` in gpui-base, so it stays a
//!   dispatched kit action (deferred by gpui, harmless).
//!
//! Caret model (ADR-0005): normal mode rests on a collapsed caret and the
//! kit's native block cursor (vendored `set_block_cursor`); nothing is
//! selected — not even a trailing `\n`. Insert mode clears the flag and
//! uses the thin caret. Visual selection is `anchor..caret+1` (rgpui's
//! `extend_to` plus one char, because the block covers a char).
//! `consume` tracks the visual caret itself — the kit reports the
//! selection end as its cursor, which is not the moving end.
//!
//! Deliberate subset (ADR-0004): no `"` registers, no macros, no plugins,
//! no counts; `:w`/`:q` only. Incompatibilities with full vi are
//! intentional — do not "fix" them into completeness.

use gpui_kit::component::input::{EditorState, Rope, RopeExt as _};
use gpui_kit::{Context, Window};

// ---------------------------------------------------------------------------
// Grammar (pure)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VimMode {
    #[default]
    Normal,
    Insert,
    Visual,
}

/// `:` command parsed from the cmdline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Write,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Down,
    Up,
    /// `0`
    LineStart,
    /// `$`
    LineEnd,
    /// `^`
    FirstNonBlank,
    /// `gg` — first line, preferred column
    DocStart,
    /// `G` — last line, preferred column
    DocEnd,
    /// `w`
    NextWord,
    /// `b`
    PrevWord,
    /// `e` — end of the current/next word
    WordEnd,
    /// `ge` — start of the previous word end
    PrevWordEnd,
    /// `{` — previous blank line (paragraph boundary)
    ParaBackward,
    /// `}` — next blank line (paragraph boundary)
    ParaForward,
}

impl Motion {
    /// Valid as the target of an operator (`d`/`y`/`c` + motion)?
    fn as_operator_target(self) -> bool {
        !matches!(
            self,
            Motion::Down | Motion::Up | Motion::DocStart | Motion::DocEnd
        )
    }
}

/// A deletion, from `x` or operator+motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteOp {
    /// `x`/`delete` — char under the caret
    Char,
    /// `de`
    ToWordEnd,
    ToNextWord,
    /// `d0`
    ToLineStart,
    /// `d$`
    ToLineEnd,
    /// `d^`
    ToFirstNonBlank,
    /// `d}`/`y}`/`c}` — to the next paragraph boundary
    ToParaForward,
    /// `d{`/`y{`/`c{` — to the previous paragraph boundary
    ToParaBackward,
}

/// `dd`/`yy`/`cc` — the current line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Delete,
    Yank,
    Change,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VimAction {
    /// Key eaten, nothing to execute (unknown keys must never type).
    Consumed,
    Move(Motion),
    Delete(DeleteOp),
    /// `dd`/`yy`/`cc`. `usize::MAX` = current line (the machine does not
    /// track the cursor; the executor does).
    LineOp(Op, usize),
    EnterInsert {
        append: bool,
        open_below: bool,
        open_above: bool,
    },
    /// Esc from insert: block-select the char at the insert caret.
    EnterNormal,
    /// `v`.
    EnterVisual,
    /// `V` — linewise visual (round 34): anchor + cursor snap to whole
    /// lines, motions extend by line, `d`/`y` act line-wise.
    EnterVisualLine,
    /// `V`/`v` toggle while already visual: back to normal.
    LeaveVisual,
    YankSelection,
    /// `x`/`d`/`delete` in visual.
    DeleteSelection,
    /// `y{motion}` — yank without deleting.
    YankMotion(DeleteOp),
    /// `c{motion}` — delete then insert.
    ChangeThenInsert(DeleteOp),
    /// `p`.
    Paste,
    Undo,
    Command(Cmd),
}

#[derive(Debug, Clone, Default)]
pub struct VimState {
    pub mode: VimMode,
    /// Operator awaiting its second key: `d`/`y`/`c`/`g`.
    pending: Option<char>,
    /// Visual anchor (byte offset).
    pub visual_anchor: Option<usize>,
    /// Moving end of the visual selection (tracked here; the kit reports
    /// the selection end as its cursor, which is not always this).
    pub visual_cursor: usize,
    /// `V`: visual selects whole lines (round 34). Cleared by
    /// leave_visual/reset.
    pub visual_linewise: bool,
    cmdline: String,
    /// Last yank.
    pub register: Option<String>,
    /// Register holds whole lines (`yy`) vs characters.
    pub register_linewise: bool,
    /// Preferred column (char count) remembered across j/k.
    pub preferred_col: usize,
}

impl VimState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset for a new session or a vim toggle-off (starts in normal).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    /// Issue 21: status line removed; cmdline stays test-visible.
    #[cfg(test)]
    pub fn cmdline(&self) -> &str {
        &self.cmdline
    }

    /// Enter visual at `anchor` (the executor knows the caret offset).
    pub fn begin_visual(&mut self, anchor: usize) {
        self.mode = VimMode::Visual;
        self.visual_anchor = Some(anchor);
        self.visual_cursor = anchor;
    }

    /// Test/shell helper.
    #[cfg(test)]
    pub fn set_mode(&mut self, mode: VimMode) {
        if mode != VimMode::Visual {
            self.visual_anchor = None;
            self.visual_linewise = false;
        }
        self.mode = mode;
    }

    fn leave_visual(&mut self) {
        self.visual_anchor = None;
        self.visual_linewise = false;
        self.mode = VimMode::Normal;
    }

    /// Feed one key: `key` is the binding name ("h", "G", "enter",
    /// "escape", …); `key_char` the printable it produced, if any.
    pub fn handle_key(&mut self, key: &str, key_char: Option<char>) -> VimAction {
        if !self.cmdline.is_empty() {
            return self.cmdline_key(key, key_char);
        }
        match self.mode {
            VimMode::Insert => {
                if key == "escape" {
                    self.mode = VimMode::Normal;
                    VimAction::EnterNormal
                } else {
                    VimAction::Consumed // the shell never routes insert keys here
                }
            }
            VimMode::Normal => self.normal_key(key, key_char),
            VimMode::Visual => self.visual_key(key),
        }
    }

    fn cmdline_key(&mut self, key: &str, key_char: Option<char>) -> VimAction {
        match key {
            "escape" => {
                self.cmdline.clear();
                VimAction::Consumed
            }
            "enter" => {
                let cmd = match self.cmdline.trim() {
                    ":w" | ":wq" | ":x" => Some(Cmd::Write),
                    ":q" => Some(Cmd::Quit),
                    _ => None,
                };
                self.cmdline.clear();
                match cmd {
                    Some(c) => VimAction::Command(c),
                    None => VimAction::Consumed,
                }
            }
            "backspace" => {
                self.cmdline.pop();
                VimAction::Consumed
            }
            _ => {
                // ADR-0004: only :w/:q exist; a few chars of headroom, then
                // the line stops growing (still enter/esc-able).
                if self.cmdline.len() < 8 {
                    if let Some(c) = key_char.filter(|c| c.is_ascii_graphic()) {
                        self.cmdline.push(c);
                    }
                }
                VimAction::Consumed
            }
        }
    }

    fn normal_key(&mut self, key: &str, key_char: Option<char>) -> VimAction {
        // Pending operator: dd/yy/cc, gg, or operator+motion. A key that
        // completes nothing voids the prefix and is swallowed (it must
        // not also run as a fresh command).
        if let Some(op) = self.pending.take() {
            if key == "escape" {
                return VimAction::Consumed;
            }
            // Doubled operators (dd/yy/cc) and gg complete line-wise.
            if let Some(line_op) = match (op, key) {
                ('d', "d") => Some(Op::Delete),
                ('y', "y") => Some(Op::Yank),
                ('c', "c") => Some(Op::Change),
                ('g', "g") => {
                    return VimAction::Move(Motion::DocStart);
                }
                ('g', "e") => {
                    return VimAction::Move(Motion::PrevWordEnd);
                }
                _ => None,
            } {
                return VimAction::LineOp(line_op, usize::MAX);
            }
            if let Some(m) = motion_of(key).filter(|m| m.as_operator_target()) {
                return operator_motion(op, m);
            }
            return VimAction::Consumed;
        }

        match key {
            "h" => VimAction::Move(Motion::Left),
            "l" => VimAction::Move(Motion::Right),
            "j" | "enter" => VimAction::Move(Motion::Down),
            "k" => VimAction::Move(Motion::Up),
            "0" => VimAction::Move(Motion::LineStart),
            "$" => VimAction::Move(Motion::LineEnd),
            "{" => VimAction::Move(Motion::ParaBackward),
            "}" => VimAction::Move(Motion::ParaForward),
            "^" => VimAction::Move(Motion::FirstNonBlank),
            "G" => VimAction::Move(Motion::DocEnd),
            "w" => VimAction::Move(Motion::NextWord),
            "e" => VimAction::Move(Motion::WordEnd),
            "b" => VimAction::Move(Motion::PrevWord),
            "g" | "d" | "y" | "c" => {
                self.pending = key.chars().next();
                VimAction::Consumed
            }
            "x" | "delete" => VimAction::Delete(DeleteOp::Char),
            "D" => VimAction::Delete(DeleteOp::ToLineEnd),
            "backspace" => VimAction::Move(Motion::Left),
            "i" => VimAction::EnterInsert {
                append: false,
                open_below: false,
                open_above: false,
            },
            "a" => VimAction::EnterInsert {
                append: true,
                open_below: false,
                open_above: false,
            },
            "o" => VimAction::EnterInsert {
                append: false,
                open_below: true,
                open_above: false,
            },
            "O" => VimAction::EnterInsert {
                append: false,
                open_below: false,
                open_above: true,
            },
            "v" => {
                self.mode = VimMode::Visual; // anchor set by exec
                self.visual_linewise = false;
                VimAction::EnterVisual
            }
            "V" => {
                self.mode = VimMode::Visual;
                self.visual_linewise = true;
                VimAction::EnterVisualLine
            }
            "p" => VimAction::Paste,
            "u" => VimAction::Undo,
            "escape" => VimAction::Consumed, // no pending (taken above)
            ":" => {
                self.cmdline.clear();
                self.cmdline.push(':');
                VimAction::Consumed
            }
            _ => {
                let _ = key_char;
                VimAction::Consumed // subset: swallow, never type
            }
        }
    }

    fn visual_key(&mut self, key: &str) -> VimAction {
        match key {
            "h" => VimAction::Move(Motion::Left),
            "l" => VimAction::Move(Motion::Right),
            "j" | "enter" => VimAction::Move(Motion::Down),
            "k" => VimAction::Move(Motion::Up),
            "0" => VimAction::Move(Motion::LineStart),
            "$" => VimAction::Move(Motion::LineEnd),
            "{" => VimAction::Move(Motion::ParaBackward),
            "}" => VimAction::Move(Motion::ParaForward),
            "^" => VimAction::Move(Motion::FirstNonBlank),
            "G" => VimAction::Move(Motion::DocEnd),
            "g" => VimAction::Move(Motion::DocStart),
            "w" => VimAction::Move(Motion::NextWord),
            "e" => VimAction::Move(Motion::WordEnd),
            "b" => VimAction::Move(Motion::PrevWord),
            "x" | "d" | "delete" => VimAction::DeleteSelection,
            "y" => VimAction::YankSelection,
            "p" => VimAction::Paste,
            "escape" => {
                self.leave_visual();
                VimAction::LeaveVisual
            }
            // v/V toggle out; V inside charwise upgrades to linewise.
            "V" => {
                if self.visual_linewise {
                    self.leave_visual();
                    VimAction::LeaveVisual
                } else {
                    self.visual_linewise = true;
                    VimAction::EnterVisualLine
                }
            }
            "v" => {
                if self.visual_linewise {
                    self.visual_linewise = false;
                    VimAction::EnterVisual
                } else {
                    self.leave_visual();
                    VimAction::LeaveVisual
                }
            }
            _ => VimAction::Consumed,
        }
    }
}

fn motion_of(key: &str) -> Option<Motion> {
    Some(match key {
        "h" => Motion::Left,
        "l" => Motion::Right,
        "j" => Motion::Down,
        "k" => Motion::Up,
        "0" => Motion::LineStart,
        "$" => Motion::LineEnd,
        "^" => Motion::FirstNonBlank,
        "{" => Motion::ParaBackward,
        "}" => Motion::ParaForward,
        "w" => Motion::NextWord,
        "e" => Motion::WordEnd,
        "b" => Motion::PrevWord,
        _ => return None,
    })
}

fn operator_motion(op: char, m: Motion) -> VimAction {
    let del = match m {
        Motion::NextWord => DeleteOp::ToNextWord,
        Motion::WordEnd => DeleteOp::ToWordEnd,
        Motion::LineStart => DeleteOp::ToLineStart,
        Motion::LineEnd => DeleteOp::ToLineEnd,
        Motion::FirstNonBlank => DeleteOp::ToFirstNonBlank,
        Motion::ParaForward => DeleteOp::ToParaForward,
        Motion::ParaBackward => DeleteOp::ToParaBackward,
        Motion::Left | Motion::Right => DeleteOp::Char,
        _ => return VimAction::Consumed,
    };
    match op {
        'y' => VimAction::YankMotion(del),
        'c' => VimAction::ChangeThenInsert(del),
        _ => VimAction::Delete(del),
    }
}

// ---------------------------------------------------------------------------
// Buffer math (rgpui ports) — byte offsets, char-count columns
// ---------------------------------------------------------------------------

/// Vim's three character classes for word motions (`:h word`): keyword,
/// punctuation, whitespace. Punctuation runs are themselves stops
/// (`foo.bar` → `foo`/`.`/`bar`); the old binary classifier collapsed
/// them. CJK follows vim's default `iskeyword`: a CJK run is ONE word —
/// deliberate strict-vim choice (Zed splits per char; we do not).
#[derive(Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Keyword,
    Punct,
    Blank,
}

fn class_of(c: char) -> CharClass {
    if c.is_alphanumeric() || c == '_' {
        CharClass::Keyword
    } else if c.is_whitespace() {
        CharClass::Blank
    } else {
        CharClass::Punct
    }
}

/// Line span excluding its `\n`: `(start, end)`.
fn line_range(text: &Rope, row: usize) -> (usize, usize) {
    (text.line_start_offset(row), text.line_end_offset(row))
}

fn row_of(text: &Rope, offset: usize) -> usize {
    text.offset_to_point(offset).row
}

/// Whether `row` is empty or all whitespace — a paragraph boundary.
fn is_blank_row(text: &Rope, row: usize) -> bool {
    let (start, end) = line_range(text, row);
    text.slice(start..end).chars().all(|c| c.is_whitespace())
}

/// `}`: jump to the next blank line (paragraph boundary) strictly below
/// `row`; the buffer end when there is none (the kit's trailing virtual
/// row after a final `\n` is itself blank and matches vim's end stop).
/// `d}` deletes `caret..boundary` (exclusive).
fn para_forward(text: &Rope, row: usize) -> usize {
    for r in (row + 1)..text.lines_len() {
        if is_blank_row(text, r) {
            return text.line_start_offset(r);
        }
    }
    text.len()
}

/// `{`: start of the previous blank line strictly above `row`; row 0
/// start when there is none. `d{` deletes `boundary..caret`.
fn para_backward(text: &Rope, row: usize) -> usize {
    for r in (0..row).rev() {
        if is_blank_row(text, r) {
            return text.line_start_offset(r);
        }
    }
    text.line_start_offset(0)
}

/// Byte offset of the `col`-th char of `row`, clamped to the line end.
fn char_offset_in_line(text: &Rope, row: usize, col: usize) -> usize {
    let (start, end) = line_range(text, row);
    let mut offset = start;
    for (n, ch) in text.slice(start..end).chars().enumerate() {
        if n >= col {
            break;
        }
        offset += ch.len_utf8();
    }
    offset
}

/// Chars from the line start to `offset` — the preferred-column metric.
fn char_col(text: &Rope, offset: usize) -> usize {
    let (start, _) = line_range(text, row_of(text, offset));
    text.slice(start..offset).chars().count()
}

/// End of the char starting at `offset` (start of the next char).
fn next_char(text: &Rope, offset: usize) -> usize {
    let len = text.len();
    if offset >= len {
        return len;
    }
    text.ceil_char_boundary(offset + 1).min(len)
}

/// Start of the char before `offset`.
fn prev_char(text: &Rope, offset: usize) -> usize {
    text.floor_char_boundary(offset.saturating_sub(1))
}

/// `w`: next word start. Three-class vim semantics: finish the current
/// class run, then skip blanks (across newlines) to the next keyword or
/// punct run. An empty line is a stop in itself.
fn word_forward(text: &Rope, cursor: usize) -> usize {
    let len = text.len();
    let mut offset = cursor.min(len);
    let mut chars = text.chars_at(offset).peekable();
    if let Some(&c) = chars.peek() {
        let class = class_of(c);
        while chars.peek().is_some_and(|&c| class_of(c) == class) {
            offset += c.len_utf8();
            chars.next();
        }
    }
    while let Some(&c) = chars.peek() {
        if c == '\n' && is_blank_row(text, row_of(text, offset)) {
            return offset;
        }
        if class_of(c) != CharClass::Blank {
            break;
        }
        offset += c.len_utf8();
        chars.next();
    }
    offset.min(len)
}

/// `b`: previous word start. From inside a run: that run's start; from a
/// run start: the previous keyword/punct run's start (vim treats a punct
/// run as its own word). Empty lines are stops — but only when arriving
/// from elsewhere, so `b` on an empty line itself moves on.
fn word_backward(text: &Rope, cursor: usize) -> usize {
    let mut offset = cursor.min(text.len());
    let mut chars = text.chars_at(offset).reversed();
    // The char under the block caret (at `offset`) is NOT part of the
    // backward scan; step past it, then handle whatever precedes.
    let Some(first) = chars.next() else {
        return offset;
    };
    offset -= first.len_utf8();
    if class_of(first) != CharClass::Blank {
        // Caret inside a keyword/punct run: back to that run's start.
        let class = class_of(first);
        while let Some(c) = chars.next() {
            if class_of(c) != class {
                return offset;
            }
            offset -= c.len_utf8();
        }
        return offset;
    }
    // Blank run: skip blanks back, stop on an empty line, then walk the
    // previous run back to its start.
    while let Some(c) = chars.next() {
        if c == '\n' && offset != cursor && is_blank_row(text, row_of(text, offset)) {
            return offset;
        }
        if class_of(c) != CharClass::Blank {
            offset -= c.len_utf8();
            let class = class_of(c);
            while let Some(c) = chars.next() {
                if class_of(c) != class {
                    return offset;
                }
                offset -= c.len_utf8();
            }
            return offset;
        }
        offset -= c.len_utf8();
    }
    offset
}

/// End of the keyword/punct run starting at `offset`, expressed as the
/// offset of the run's LAST char (where a block caret rests after `e`).
fn run_end(text: &Rope, offset: usize) -> usize {
    let mut end = offset;
    let mut chars = text.chars_at(offset).peekable();
    if let Some(&c) = chars.peek() {
        let class = class_of(c);
        while chars.peek().is_some_and(|&c| class_of(c) == class) {
            end += c.len_utf8();
            chars.next();
        }
    }
    prev_char(text, end)
}

/// `e`: end of the current word when the caret sits before its last
/// char; on a run's last char (or blanks) it moves to the END of the
/// NEXT keyword/punct run. Empty lines are stops.
fn word_end(text: &Rope, cursor: usize) -> usize {
    let len = text.len();
    let mut offset = cursor.min(len);
    // The block caret covers the char AT `offset`.
    if offset < len {
        let cur = text.chars_at(offset).next().unwrap();
        let class = class_of(cur);
        if class != CharClass::Blank {
            let mut chars = text.chars_at(offset).peekable();
            let mut end = offset;
            while let Some(&c) = chars.peek() {
                if class_of(c) != class {
                    break;
                }
                end += c.len_utf8();
                chars.next();
            }
            // Strictly inside the run (a same-class char follows the
            // caret's char): rest on the run's last char.
            if end > next_char(text, offset) {
                return prev_char(text, end);
            }
            // On the run's last char: skip past it, find the next run.
            offset = end;
        }
    }
    // Skip blanks to the next non-blank run; an empty line is a stop.
    let mut chars = text.chars_at(offset).peekable();
    while let Some(&c) = chars.peek() {
        if c == '\n' && is_blank_row(text, row_of(text, offset)) {
            return offset;
        }
        if class_of(c) != CharClass::Blank {
            return run_end(text, offset);
        }
        offset += c.len_utf8();
        chars.next();
    }
    offset
}

/// `ge`: end (last char) of the word BEFORE the caret's word. Empty
/// lines are stops.
fn word_end_backward(text: &Rope, cursor: usize) -> usize {
    let mut offset = cursor.min(text.len());
    // Start of the run containing the caret's own char (blank caret →
    // stay; the blank-skip below handles it).
    if let Some(&cur) = text.chars_at(offset).peekable().peek() {
        let class = class_of(cur);
        if class != CharClass::Blank {
            let mut chars = text.chars_at(offset).reversed();
            while let Some(c) = chars.next() {
                if class_of(c) != class {
                    break;
                }
                offset -= c.len_utf8();
            }
        }
    }
    // Back over blanks (empty line = stop at its start) to the previous
    // run's last char.
    let mut chars = text.chars_at(offset).reversed();
    while let Some(c) = chars.next() {
        if c == '\n' {
            let row = row_of(text, offset);
            if is_blank_row(text, row) {
                return text.line_start_offset(row);
            }
        }
        if class_of(c) != CharClass::Blank {
            return offset - c.len_utf8();
        }
        offset -= c.len_utf8();
    }
    offset
}

// ---------------------------------------------------------------------------
// Execution — direct EditorState mutation
// ---------------------------------------------------------------------------

fn goto(state: &mut EditorState, offset: usize, cx: &mut Context<EditorState>) {
    let offset = offset.min(state.text().len());
    state.set_selected_range(offset..offset, cx);
}

/// Normal-mode rest position (ADR-0005): the kit's native block caret at
/// `offset` (vendored `set_block_cursor`); the selection stays collapsed.
/// Insert mode keeps the thin caret, so callers routing mode-dependent
/// rests go through this instead of `goto`.
fn rest(state: &mut EditorState, mode: VimMode, offset: usize, cx: &mut Context<EditorState>) {
    state.set_block_cursor(mode != VimMode::Insert, cx);
    goto(state, offset, cx);
}

/// ADR-0005 block caret, now native to the vendored kit: paint collapses
/// the selection (nothing selected — not even a trailing `\n`) and the
/// editor draws a blinking block over the char at the offset.
fn block_caret(state: &mut EditorState, offset: usize, cx: &mut Context<EditorState>) {
    state.set_block_cursor(true, cx);
    goto(state, offset, cx);
}

fn delete_range(
    state: &mut EditorState,
    range: std::ops::Range<usize>,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) {
    state.set_selected_range(range, cx);
    state.replace("", window, cx);
}

/// The caret this action should act from: the visual moving end in visual
/// mode, else the start of the (possibly block) selection.
fn caret_of(vim: &VimState, state: &EditorState) -> usize {
    if vim.mode == VimMode::Visual {
        vim.visual_cursor
    } else {
        state.selected_range().start
    }
}

/// Execute one action directly against the editor. `dispatch_undo` sends
/// the kit's `Undo` action (the engine's undo is not public API).
pub fn exec(
    action: &VimAction,
    vim: &mut VimState,
    state: &mut EditorState,
    window: &mut Window,
    cx: &mut Context<EditorState>,
    dispatch_undo: &mut dyn FnMut(&mut Window, &mut Context<EditorState>),
) {
    let text = state.text().clone();
    let caret = caret_of(vim, state).min(text.len());
    match action {
        VimAction::Consumed => {}

        VimAction::Move(m) => {
            let (line_start, line_end) = line_range(&text, row_of(&text, caret));
            let target = match m {
                Motion::Left => prev_char(&text, caret).max(line_start),
                // Visual extends across the line break; normal stops at it.
                Motion::Right => {
                    if vim.mode == VimMode::Visual {
                        next_char(&text, caret).min(text.len())
                    } else {
                        next_char(&text, caret).min(line_end)
                    }
                }
                Motion::Down | Motion::Up => {
                    let delta = if *m == Motion::Up { -1isize } else { 1 };
                    let row = row_of(&text, caret) as isize;
                    let last = (text.lines_len() - 1) as isize;
                    let row = (row + delta).clamp(0, last) as usize;
                    char_offset_in_line(&text, row, vim.preferred_col)
                }
                Motion::LineStart => line_start,
                // `^`: first non-blank char; blank-only line falls back to
                // the line start.
                Motion::FirstNonBlank => {
                    let (start, end) = (line_start, line_end);
                    let mut target = start;
                    for ch in text.slice(start..end).chars() {
                        if !ch.is_whitespace() {
                            break;
                        }
                        target += ch.len_utf8();
                    }
                    target
                }
                Motion::LineEnd => prev_char(&text, line_end).max(line_start),
                Motion::DocStart | Motion::DocEnd => {
                    let row = if *m == Motion::DocStart {
                        0
                    } else {
                        text.lines_len() - 1
                    };
                    char_offset_in_line(&text, row, vim.preferred_col)
                }
                Motion::NextWord => word_forward(&text, caret),
                Motion::PrevWord => word_backward(&text, caret),
                Motion::WordEnd => word_end(&text, caret),
                Motion::PrevWordEnd => word_end_backward(&text, caret),
                Motion::ParaForward => para_forward(&text, row_of(&text, caret)),
                Motion::ParaBackward => para_backward(&text, row_of(&text, caret)),
            };
            if vim.mode == VimMode::Visual {
                if vim.visual_linewise {
                    // V: every motion snaps to whole lines — only the
                    // target row matters; column movements within a line
                    // don't change the selection.
                    extend_linewise_row(vim, state, row_of(&text, target), cx);
                } else {
                    extend_selection(vim, state, target, cx);
                }
            } else {
                rest(state, vim.mode, target, cx);
            }
            // rgpui: horizontal moves refresh the preferred column;
            // vertical moves keep the remembered one.
            if matches!(
                m,
                Motion::Left
                    | Motion::Right
                    | Motion::LineStart
                    | Motion::FirstNonBlank
                    | Motion::NextWord
                    | Motion::PrevWord
                    | Motion::WordEnd
                    | Motion::PrevWordEnd
                    | Motion::ParaForward
                    | Motion::ParaBackward
            ) {
                vim.preferred_col = char_col(&text, target);
            }
            cx.notify();
        }

        VimAction::Delete(op) => {
            if let Some(range) = delete_range_for(*op, &text, caret) {
                vim.register = Some(text.slice(range.start..range.end).to_string());
                vim.register_linewise = false;
                let start = range.start;
                delete_range(state, range, window, cx);
                rest(state, vim.mode, start, cx);
            }
            cx.notify();
        }

        VimAction::DeleteSelection => {
            // V-selection deletes whole lines: snap to line boundaries and
            // eat the trailing newline (mirrors dd, so no blank line
            // remains; last line eats the preceding newline).
            if vim.visual_linewise {
                let range = state.selected_range();
                let (first_row, last_row) = (
                    row_of(&text, range.start),
                    row_of(&text, range.end.saturating_sub(1)),
                );
                let start = line_range(&text, first_row).0;
                let end = if last_row + 1 < text.lines_len() {
                    text.line_start_offset(last_row + 1)
                } else {
                    text.len()
                };
                let (del_start, del_end) = if text.lines_len() <= 1 {
                    (0, text.len())
                } else if range.is_empty() {
                    (start, start)
                } else if end == text.len() && start > 0 {
                    (start - 1, text.len())
                } else {
                    (start, end)
                };
                if del_end > del_start {
                    vim.register = Some(text.slice(del_start..del_end).to_string());
                    vim.register_linewise = true;
                    delete_range(state, del_start..del_end, window, cx);
                }
                rest(
                    state,
                    VimMode::Normal,
                    del_start.min(state.text().len()),
                    cx,
                );
            } else {
                let range = state.selected_range();
                if !range.is_empty() {
                    vim.register = Some(text.slice(range.start..range.end).to_string());
                    vim.register_linewise = false;
                    let start = range.start;
                    delete_range(state, range, window, cx);
                    rest(state, vim.mode, start, cx);
                }
            }
            vim.leave_visual();
            cx.notify();
        }

        VimAction::YankSelection => {
            let range = state.selected_range();
            let start = if vim.visual_linewise {
                // V: register gets whole lines (no trailing `\n` —
                // put_register adds it when pasting, same convention as
                // yy) and p pastes them as lines. The selection may be
                // shorter than the snapped lines, so re-derive the span.
                let (first_row, last_row) = (
                    row_of(&text, range.start),
                    row_of(&text, range.end.saturating_sub(1)),
                );
                let start = line_range(&text, first_row).0;
                let end = line_range(&text, last_row).1;
                vim.register = Some(text.slice(start..end).to_string());
                vim.register_linewise = true;
                // Rest on the last yanked line: p pastes below the
                // cursor's line, so the copy lands right after the
                // selection (vim's `Vy p` behavior).
                line_range(&text, last_row).0
            } else {
                vim.register = Some(text.slice(range.start..range.end).to_string());
                vim.register_linewise = false;
                range.start
            };
            vim.leave_visual();
            rest(state, VimMode::Normal, start, cx);
            cx.notify();
        }

        VimAction::YankMotion(op) => {
            if let Some(range) = delete_range_for(*op, &text, caret) {
                vim.register = Some(text.slice(range.start..range.end).to_string());
                vim.register_linewise = false;
            }
            cx.notify();
        }

        VimAction::ChangeThenInsert(op) => {
            if let Some(range) = delete_range_for(*op, &text, caret) {
                let start = range.start;
                delete_range(state, range, window, cx);
                goto(state, start, cx);
            }
            vim.mode = VimMode::Insert;
            cx.notify();
        }

        VimAction::LineOp(op, _) => {
            let row = row_of(&text, caret);
            let (start, end) = line_range(&text, row);
            vim.register = Some(text.slice(start..end).to_string());
            vim.register_linewise = true;
            match op {
                Op::Yank => {}
                Op::Delete => {
                    // rgpui: eat the trailing newline so no blank line
                    // remains; the last line eats the preceding newline;
                    // a single-line buffer empties entirely.
                    let (del_start, del_end) = if text.lines_len() <= 1 {
                        (0, text.len())
                    } else if row + 1 < text.lines_len() {
                        (start, text.line_start_offset(row + 1))
                    } else {
                        (start.saturating_sub(1), text.len())
                    };
                    delete_range(state, del_start..del_end, window, cx);
                    rest(state, vim.mode, del_start.min(state.text().len()), cx);
                }
                Op::Change => {
                    // cc keeps the line (and its newline), clears content.
                    delete_range(state, start..end, window, cx);
                    goto(state, start, cx);
                    vim.mode = VimMode::Insert;
                }
            }
            cx.notify();
        }

        VimAction::EnterInsert {
            append,
            open_below,
            open_above,
        } => {
            let mut at = caret;
            if *append {
                let (_, line_end) = line_range(&text, row_of(&text, at));
                at = next_char(&text, at).min(line_end).max(at);
            }
            if *open_below || *open_above {
                let row = row_of(&text, at);
                let offset = if *open_below {
                    line_range(&text, row).1 // before the `\n`
                } else {
                    line_range(&text, row).0
                };
                goto(state, offset, cx);
                state.insert("\n", window, cx);
                if *open_above {
                    goto(state, offset, cx); // caret on the new upper line
                }
            } else {
                goto(state, at, cx);
            }
            state.set_block_cursor(false, cx); // insert mode: thin caret only
            vim.mode = VimMode::Insert;
            cx.notify();
        }

        VimAction::EnterNormal => {
            // Esc from insert: block-select the char at the insert caret
            // (no left shift — pinned by the pipeline test).
            block_caret(state, caret, cx);
            vim.preferred_col = char_col(&text, caret);
            cx.notify();
        }

        VimAction::EnterVisual => {
            // Grammar already set mode=Visual; anchor at the real caret
            // (caret_of would return the stale visual_cursor).
            let caret = state.selected_range().start;
            vim.begin_visual(caret);
            extend_selection(vim, state, caret, cx);
            cx.notify();
        }

        VimAction::EnterVisualLine => {
            // Entering linewise (or upgrading from charwise): re-anchor at
            // the caret and select its whole line. Re-entering linewise
            // from charwise keeps the current selection's end as the new
            // moving cursor, so V after v+j selects both lines.
            let caret = if vim.visual_linewise
                && vim.mode == VimMode::Visual
                && state.selected_range().len() > 0
            {
                caret_of(vim, state)
            } else {
                state.selected_range().start
            };
            vim.begin_visual(caret);
            extend_linewise(state, caret, cx);
            cx.notify();
        }

        VimAction::LeaveVisual => {
            let start = state.selected_range().start;
            rest(state, vim.mode, start, cx);
            cx.notify();
        }

        VimAction::Paste => {
            put_register(vim, state, window, cx);
            cx.notify();
        }

        VimAction::Undo => dispatch_undo(window, cx),

        VimAction::Command(_) => {} // the shell maps Write/Quit itself
    }
}

/// Byte range a delete op covers at `caret`; None when it is empty
/// (e.g. `x` past the last char of the line).
fn delete_range_for(op: DeleteOp, text: &Rope, caret: usize) -> Option<std::ops::Range<usize>> {
    let (line_start, line_end) = line_range(text, row_of(text, caret));
    Some(match op {
        DeleteOp::Char => {
            // `x` at the line end (block caret sits past the last char)
            // deletes leftward, as in vim; an empty line deletes nothing.
            if caret >= line_end {
                if line_end > line_start {
                    prev_char(text, line_end)..line_end
                } else {
                    return None;
                }
            } else {
                caret..next_char(text, caret).min(line_end)
            }
        }
        // `dw`: vim's exclusive-motion rule — never eat past this line's
        // end, so deleting the last word never eats the newline.
        DeleteOp::ToNextWord => {
            let end = word_forward(text, caret);
            caret..end.min(line_end)
        }
        DeleteOp::ToWordEnd => caret..next_char(text, word_end(text, caret)),
        DeleteOp::ToLineStart => line_start..caret,
        DeleteOp::ToFirstNonBlank => {
            let mut first = line_start;
            for ch in text.slice(line_start..caret).chars() {
                if !ch.is_whitespace() {
                    break;
                }
                first += ch.len_utf8();
            }
            first..caret
        }
        DeleteOp::ToLineEnd => caret..line_end,
        DeleteOp::ToParaForward => caret..para_forward(text, row_of(text, caret)),
        DeleteOp::ToParaBackward => para_backward(text, row_of(text, caret))..caret,
    })
}

/// rgpui `extend_to` + block caret: selection spans `anchor..target+1`
/// (whichever end is greater gets the +1); the moving caret is `target`.
fn extend_selection(
    vim: &mut VimState,
    state: &mut EditorState,
    target: usize,
    cx: &mut Context<EditorState>,
) {
    let anchor = vim.visual_anchor.unwrap_or(target);
    vim.visual_cursor = target;
    let (start, end) = if anchor <= target {
        (anchor, next_char(state.text(), target))
    } else {
        (target, next_char(state.text(), anchor))
    };
    state.set_selected_range(start..end, cx);
}

/// `V` selection snap: rows `anchor_row..=target_row` as one byte range
/// (line starts to the boundary after the last selected line — i.e. its
/// trailing `\n`, or the buffer end).
fn linewise_range(text: &Rope, anchor_row: usize, target_row: usize) -> std::ops::Range<usize> {
    let (lo, hi) = (anchor_row.min(target_row), anchor_row.max(target_row));
    let start = text.line_start_offset(lo);
    let end = if hi + 1 < text.lines_len() {
        text.line_start_offset(hi + 1)
    } else {
        text.len()
    };
    start..end
}

/// Linewise visual: anchor row = `caret`'s row; select that whole row.
fn extend_linewise(state: &mut EditorState, caret: usize, cx: &mut Context<EditorState>) {
    let row = row_of(state.text(), caret);
    let range = linewise_range(state.text(), row, row);
    state.set_selected_range(range, cx);
}

/// Linewise visual motion: keep the anchor row, move the cursor to
/// `target_row`, select `anchor_row..=target_row`.
fn extend_linewise_row(
    vim: &mut VimState,
    state: &mut EditorState,
    target_row: usize,
    cx: &mut Context<EditorState>,
) {
    let anchor_row = vim
        .visual_anchor
        .map(|a| row_of(state.text(), a))
        .unwrap_or(target_row);
    let range = linewise_range(state.text(), anchor_row, target_row);
    vim.visual_cursor = range.end.saturating_sub(1).min(range.end);
    state.set_selected_range(range, cx);
}

/// rgpui `put_register`: linewise pastes a new line below; charwise
/// inserts at the caret.
fn put_register(
    vim: &VimState,
    state: &mut EditorState,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) {
    let Some(reg) = vim.register.as_ref() else {
        return;
    };
    let text = state.text().clone();
    if vim.register_linewise {
        if text.len() == 0 {
            state.insert(reg.clone(), window, cx);
            rest(state, vim.mode, 0, cx);
            return;
        }
        let row = row_of(&text, state.cursor());
        let at = line_range(&text, row).1; // before the `\n`
        goto(state, at, cx);
        state.insert(format!("\n{reg}"), window, cx);
        rest(state, vim.mode, at + 1, cx); // first char of the pasted line
    } else {
        let at = state.cursor().min(text.len());
        goto(state, at, cx);
        state.insert(reg.clone(), window, cx);
        // p leaves the caret on the last pasted char.
        rest(state, vim.mode, prev_char(state.text(), state.cursor()), cx);
    }
}

/// Feed one key and execute it. Returns the completed `:` command, if any
/// (the shell owns save/quit). Always "consumed" — the caller stops
/// propagation; unknown keys must never reach the editor as text.
pub fn consume(
    vim: &mut VimState,
    state: &mut EditorState,
    key: &str,
    key_char: Option<char>,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) -> Option<Cmd> {
    let action = vim.handle_key(key, key_char);
    let cmd = if let VimAction::Command(c) = action {
        Some(c)
    } else {
        None
    };
    if !matches!(action, VimAction::Consumed) {
        exec(&action, vim, state, window, cx, &mut |window, cx| {
            // Undo is engine-internal; dispatch the kit action instead.
            // Deferred by gpui, so it cannot re-enter this call.
            window.dispatch_action(Box::new(gpui_kit::component::input::Undo), cx);
        });
    }
    cmd
}

// ---------------------------------------------------------------------------
// Tests — grammar is pure; execution is covered end-to-end in
// app_shell.rs (kit `EditorState` needs a windowed context).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn run(state: &mut VimState, key: &str) -> VimAction {
        state.handle_key(key, key.chars().next())
    }

    #[test]
    fn motions_in_normal() {
        let mut v = VimState::new();
        assert_eq!(run(&mut v, "h"), VimAction::Move(Motion::Left));
        assert_eq!(run(&mut v, "l"), VimAction::Move(Motion::Right));
        assert_eq!(run(&mut v, "0"), VimAction::Move(Motion::LineStart));
        assert_eq!(run(&mut v, "$"), VimAction::Move(Motion::LineEnd));
        assert_eq!(run(&mut v, "G"), VimAction::Move(Motion::DocEnd));
        assert_eq!(run(&mut v, "enter"), VimAction::Move(Motion::Down));
    }

    #[test]
    fn gg_is_two_keys() {
        let mut v = VimState::new();
        assert_eq!(run(&mut v, "g"), VimAction::Consumed);
        assert_eq!(run(&mut v, "g"), VimAction::Move(Motion::DocStart));
        // ge: previous word end
        run(&mut v, "g");
        assert_eq!(run(&mut v, "e"), VimAction::Move(Motion::PrevWordEnd));
        // g followed by something else voids the pending g (x is swallowed)
        assert_eq!(run(&mut v, "g"), VimAction::Consumed);
        assert_eq!(run(&mut v, "x"), VimAction::Consumed);
        // next x works normally
        assert_eq!(run(&mut v, "x"), VimAction::Delete(DeleteOp::Char));
    }

    #[test]
    fn dd_yy_operator_doubling() {
        let mut v = VimState::new();
        assert_eq!(run(&mut v, "d"), VimAction::Consumed);
        assert_eq!(run(&mut v, "d"), VimAction::LineOp(Op::Delete, usize::MAX));
        let mut v = VimState::new();
        assert_eq!(run(&mut v, "y"), VimAction::Consumed);
        assert_eq!(run(&mut v, "y"), VimAction::LineOp(Op::Yank, usize::MAX));
        // dw deletes to word end
        let mut v = VimState::new();
        assert_eq!(run(&mut v, "d"), VimAction::Consumed);
        assert_eq!(run(&mut v, "w"), VimAction::Delete(DeleteOp::ToNextWord));
        // d0 / d$ / y$ / cw
        let mut v = VimState::new();
        run(&mut v, "d");
        assert_eq!(run(&mut v, "0"), VimAction::Delete(DeleteOp::ToLineStart));
        run(&mut v, "d");
        assert_eq!(run(&mut v, "$"), VimAction::Delete(DeleteOp::ToLineEnd));
        run(&mut v, "y");
        assert_eq!(run(&mut v, "$"), VimAction::YankMotion(DeleteOp::ToLineEnd));
        run(&mut v, "c");
        assert_eq!(
            run(&mut v, "w"),
            VimAction::ChangeThenInsert(DeleteOp::ToNextWord)
        );
        // de deletes to word end (inclusive)
        run(&mut v, "d");
        assert_eq!(run(&mut v, "e"), VimAction::Delete(DeleteOp::ToWordEnd));
    }

    #[test]
    fn insert_entry_and_exit() {
        let mut v = VimState::new();
        assert_eq!(
            run(&mut v, "i"),
            VimAction::EnterInsert {
                append: false,
                open_below: false,
                open_above: false
            }
        );
        v.set_mode(VimMode::Insert);
        v.set_mode(VimMode::Normal);
        assert_eq!(
            run(&mut v, "a"),
            VimAction::EnterInsert {
                append: true,
                open_below: false,
                open_above: false
            }
        );
        v.set_mode(VimMode::Normal);
        assert_eq!(
            run(&mut v, "o"),
            VimAction::EnterInsert {
                append: false,
                open_below: true,
                open_above: false
            }
        );
        v.set_mode(VimMode::Normal);
        assert_eq!(
            run(&mut v, "O"),
            VimAction::EnterInsert {
                append: false,
                open_below: false,
                open_above: true
            }
        );
    }

    #[test]
    fn insert_escape_returns_to_normal() {
        let mut v = VimState::new();
        v.set_mode(VimMode::Insert);
        assert_eq!(run(&mut v, "escape"), VimAction::EnterNormal);
        assert_eq!(v.mode, VimMode::Normal);
    }

    #[test]
    fn normal_mode_enter_delete_backspace() {
        let mut v = VimState::new();
        // Enter in normal moves down a line (rgpui semantics).
        assert_eq!(run(&mut v, "enter"), VimAction::Move(Motion::Down));
        assert_eq!(run(&mut v, "delete"), VimAction::Delete(DeleteOp::Char));
        assert_eq!(run(&mut v, "backspace"), VimAction::Move(Motion::Left));
    }

    #[test]
    fn visual_motions_and_exit() {
        let mut v = VimState::new();
        assert_eq!(run(&mut v, "v"), VimAction::EnterVisual);
        assert_eq!(v.mode, VimMode::Visual);
        assert_eq!(run(&mut v, "l"), VimAction::Move(Motion::Right));
        assert_eq!(run(&mut v, "y"), VimAction::YankSelection);
        // Grammar only: mode/anchor flips happen in exec.
        assert_eq!(run(&mut v, "escape"), VimAction::LeaveVisual);
        assert_eq!(v.mode, VimMode::Normal);
        assert_eq!(run(&mut v, "v"), VimAction::EnterVisual);
        assert_eq!(run(&mut v, "d"), VimAction::DeleteSelection);
        assert_eq!(run(&mut v, "escape"), VimAction::LeaveVisual);
    }

    #[test]
    fn visual_line_toggle() {
        let mut v = VimState::new();
        assert_eq!(run(&mut v, "V"), VimAction::EnterVisualLine);
        assert_eq!(v.mode, VimMode::Visual);
        assert!(v.visual_linewise);
        // V inside linewise leaves; v drops to charwise.
        assert_eq!(run(&mut v, "V"), VimAction::LeaveVisual);
        assert_eq!(v.mode, VimMode::Normal);
        run(&mut v, "V");
        assert_eq!(run(&mut v, "v"), VimAction::EnterVisual);
        assert!(!v.visual_linewise);
        // v inside charwise upgrades nothing — V does.
        assert_eq!(run(&mut v, "V"), VimAction::EnterVisualLine);
        assert!(v.visual_linewise);
        assert_eq!(run(&mut v, "escape"), VimAction::LeaveVisual);
        assert!(!v.visual_linewise);
    }

    #[test]
    fn cmdline_w_and_q() {
        let mut v = VimState::new();
        run(&mut v, ":");
        assert_eq!(v.cmdline(), ":");
        assert_eq!(run(&mut v, "w"), VimAction::Consumed);
        assert_eq!(v.cmdline(), ":w");
        assert_eq!(run(&mut v, "enter"), VimAction::Command(Cmd::Write));
        assert_eq!(v.cmdline(), "");

        run(&mut v, ":");
        run(&mut v, "q");
        assert_eq!(run(&mut v, "enter"), VimAction::Command(Cmd::Quit));
        // escape cancels
        run(&mut v, ":");
        run(&mut v, "w");
        assert_eq!(run(&mut v, "escape"), VimAction::Consumed);
        assert_eq!(v.cmdline(), "");
    }

    #[test]
    fn word_helpers_cjk_and_lines() {
        let text: Rope = "foo bar\nbaz".into();
        assert_eq!(word_forward(&text, 0), 4);
        assert_eq!(word_forward(&text, 4), 8);
        assert_eq!(word_backward(&text, 8), 4);
        assert_eq!(word_backward(&text, 4), 0);
        assert_eq!(line_range(&text, 0), (0, 7));
        assert_eq!(line_range(&text, 1), (8, 11));
        // CJK: char columns, not bytes
        let cjk: Rope = "你好\n世界".into();
        assert_eq!(next_char(&cjk, 0), 3);
        assert_eq!(prev_char(&cjk, 3), 0);
        assert_eq!(char_offset_in_line(&cjk, 1, 1), 10);
        assert_eq!(char_col(&cjk, 3), 1);
    }

    #[test]
    fn word_motions_punct_stops() {
        // `abc = ef`: w stops at =, then ef (the reported bug).
        let text: Rope = "abc = ef".into();
        assert_eq!(word_forward(&text, 0), 4); // -> `=`
        assert_eq!(word_forward(&text, 4), 6); // -> `ef`
        assert_eq!(word_backward(&text, 6), 4);
        assert_eq!(word_backward(&text, 4), 0);
        // `foo.bar`: punct run is its own word.
        let dots: Rope = "foo.bar".into();
        assert_eq!(word_forward(&dots, 0), 3); // -> `.`
        assert_eq!(word_forward(&dots, 3), 4); // -> `bar`
        assert_eq!(word_backward(&dots, 4), 3);
        // Punct run of several chars is one stop.
        let many: Rope = "a == b".into();
        assert_eq!(word_forward(&many, 0), 2); // -> `==`
        assert_eq!(word_forward(&many, 2), 5); // -> `b`
    }

    #[test]
    fn word_motions_markdown_syntax() {
        // `**bold**` from the start lands at `bold`, not at the second `*`.
        let md: Rope = "**bold** text".into();
        assert_eq!(word_forward(&md, 0), 2); // -> `bold`
        assert_eq!(word_forward(&md, 2), 6); // -> closing `**`
        assert_eq!(word_forward(&md, 6), 9); // -> `text`
                                             // Inline code: backtick run is one stop.
        let code: Rope = "`code` x".into();
        assert_eq!(word_forward(&code, 0), 1); // -> `code`
        assert_eq!(word_forward(&code, 1), 5); // -> closing backtick
    }

    #[test]
    fn word_motions_empty_line_is_a_stop() {
        let text: Rope = "a\n\nb".into();
        // w from `a` stops on the empty line (offset 2), then on `b`.
        assert_eq!(word_forward(&text, 0), 2);
        assert_eq!(word_forward(&text, 2), 3);
        // b mirrors: from `b` back to the empty line, then to `a`.
        assert_eq!(word_backward(&text, 3), 2);
        assert_eq!(word_backward(&text, 2), 0);
        // e stops on the empty line too.
        assert_eq!(word_end(&text, 0), 2);
        // ge from `b` stops on the empty line first (vim: empty lines are
        // word stops for ge too, not skipped).
        assert_eq!(word_end_backward(&text, 3), 2);
    }

    #[test]
    fn word_motions_cjk_one_word() {
        // Strict vim default: a CJK run is ONE word (`你好世界` jumps whole).
        let cjk: Rope = "你好世界 abc".into();
        assert_eq!(word_forward(&cjk, 0), 13); // -> `abc`
        assert_eq!(word_backward(&cjk, 13), 0);
        // e from the CJK start rests on the last CJK char.
        assert_eq!(word_end(&cjk, 0), 9);
        // Mixed run boundary: CJK followed directly by punct.
        let mixed: Rope = "你好,x".into();
        assert_eq!(word_forward(&mixed, 0), 6); // -> `,`
    }

    #[test]
    fn word_end_motions() {
        let text: Rope = "abc = ef".into();
        // e from `a` rests on `c`; again rests on `=`; again on `f`.
        assert_eq!(word_end(&text, 0), 2);
        assert_eq!(word_end(&text, 2), 4);
        assert_eq!(word_end(&text, 4), 7);
        // Mid-word: e goes to this word's end.
        assert_eq!(word_end(&text, 1), 2);
        // ge from `e` (of ef) rests on `=`.
        assert_eq!(word_end_backward(&text, 6), 4);
    }

    #[test]
    fn dw_de_ranges() {
        // dw on `abc` of `abc = ef` deletes only `abc` (to the `=`).
        let text: Rope = "abc = ef".into();
        assert_eq!(delete_range_for(DeleteOp::ToNextWord, &text, 0), Some(0..4));
        // dw on the last word of a line stops at the line end.
        let last: Rope = "ef".into();
        assert_eq!(delete_range_for(DeleteOp::ToNextWord, &last, 0), Some(0..2));
        // de deletes through the end of the next word (inclusive).
        assert_eq!(delete_range_for(DeleteOp::ToWordEnd, &text, 0), Some(0..3));
    }

    #[test]
    fn x_at_line_end_deletes_leftward() {
        // `x` with the caret past the last char deletes that char.
        let text: Rope = "abc".into();
        assert_eq!(delete_range_for(DeleteOp::Char, &text, 3), Some(2..3));
        // On an empty line there is nothing to delete.
        let empty: Rope = "".into();
        assert_eq!(delete_range_for(DeleteOp::Char, &empty, 0), None);
        // Mid-line deletes forward as usual.
        assert_eq!(delete_range_for(DeleteOp::Char, &text, 1), Some(1..2));
    }
}

//! Vim subset (issue 17 / ADR-0004): normal/insert/visual modes over the kit
//! editor, driven by action dispatch. Pure state machine — no GPUI types — so
//! the whole key grammar is unit-testable. The shell wires `handle_key` into
//! `capture_key_down` on the editor container and executes the returned
//! `VimAction`s.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
}

/// Pending operator in normal mode: `d`/`c`/`y` awaiting a motion, or a
/// doubled letter awaiting its own confirmation (`dd`, `yy`, `cc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    None,
    /// d/c/y pressed once; `expect_double` is the same letter (`dd`).
    Operator {
        kind: Op,
        expect_double: bool,
    },
    /// `g` pressed, waiting for the second `g`.
    G,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Delete,
    Change,
    Yank,
}

/// Motions and edits the engine can express as a single GPUI input action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    Home,     // 0
    End,      // $
    Start,    // gg
    Finish,   // G
    NextWord, // w
    PrevWord, // b
}

/// What the shell should execute for one keypress. All variants are cheap;
/// `LineOp` and paste need direct `EditorState` access in the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VimAction {
    Dispatch(Motion), // move cursor (or extend, in visual)
    DispatchDelete(DeleteOp),
    Undo,
    /// `dd` / `yy` / `cc` on the current line.
    LineOp(Op, usize), // (op, current cursor row)
    /// `p` — paste register before/after cursor.
    Paste,
    /// Enter insert mode; if `append`, move cursor right first; `open_below`
    /// / `open_above` are `o` / `O` (newline then insert).
    EnterInsert {
        append: bool,
        open_below: bool,
        open_above: bool,
    },
    EnterVisual {
        linewise: bool,
    },
    /// `cw` / `c$` / `c0`: delete the motion range, then enter insert.
    ChangeThenInsert(DeleteOp),
    /// Leave visual back to normal (keep the cursor where selection started).
    LeaveVisual,
    /// `:w` / `:q` parsed from the command line.
    Command(Cmd),
    /// Key consumed, nothing to do (mode switch already reflected in state).
    Consumed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteOp {
    Char,          // x
    ToNextWordEnd, // dw / cw
    ToLineStart,   // d0 / c0
    ToLineEnd,     // d$ / c$ (from cursor to end of line)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Write,
    Quit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VimState {
    pub mode: VimMode,
    pending: Pending,
    /// `:` command line being typed (`:w`, `:q`).
    cmdline: String,
    /// Register for `p` (set by `yy`/`dd` line ops from the shell).
    pub register: Option<String>,
    /// Visual anchor: selection stays anchored while motions extend it.
    pub visual_anchor: Option<usize>,
}

impl Default for VimState {
    fn default() -> Self {
        Self {
            mode: VimMode::Normal,
            pending: Pending::None,
            cmdline: String::new(),
            register: None,
            visual_anchor: None,
        }
    }
}

impl VimState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handle one bare keypress (callers must filter modifiers + focus
    /// themselves; insert mode passes everything through before this).
    /// `key` is the GPUI keystroke key ("h", "d", ":", "enter", …) with
    /// `key_char` when available.
    pub fn handle_key(&mut self, key: &str, key_char: Option<char>) -> VimAction {
        if !self.cmdline.is_empty() {
            return self.cmdline_key(key, key_char);
        }
        match self.mode {
            VimMode::Insert => VimAction::Consumed, // caller never routes insert keys here
            VimMode::Normal => self.normal_key(key, key_char),
            VimMode::Visual => self.visual_key(key, key_char),
        }
    }

    fn cmdline_key(&mut self, key: &str, key_char: Option<char>) -> VimAction {
        match key {
            "escape" => {
                self.cmdline.clear();
                VimAction::Consumed
            }
            "enter" => {
                let cmd = self.cmdline.trim().to_string();
                self.cmdline.clear();
                match cmd.as_str() {
                    ":w" | ":wq" | ":x" => VimAction::Command(Cmd::Write),
                    ":q" => VimAction::Command(Cmd::Quit),
                    _ => VimAction::Consumed,
                }
            }
            "backspace" => {
                self.cmdline.pop();
                VimAction::Consumed
            }
            _ => {
                // Printable char: append to the command line.
                if let Some(c) = key_char.filter(|c| c.is_ascii_graphic() && self.cmdline.len() < 8)
                {
                    self.cmdline.push(c);
                }
                VimAction::Consumed
            }
        }
    }

    #[cfg(test)]
    pub fn cmdline(&self) -> &str {
        &self.cmdline
    }

    fn normal_key(&mut self, key: &str, key_char: Option<char>) -> VimAction {
        // Printable input routes through key_char (GPUI gives key_char for
        // bare letters; key is also the letter there).
        let c = key_char.unwrap_or(' ');
        match self.pending {
            Pending::G => {
                self.pending = Pending::None;
                return if key == "g" {
                    VimAction::Dispatch(Motion::Start)
                } else {
                    VimAction::Consumed
                };
            }
            Pending::Operator {
                kind,
                expect_double,
            } => {
                self.pending = Pending::None;
                // doubled letter: dd/yy/cc
                let doubled = matches!(
                    (kind, key),
                    (Op::Delete, "d") | (Op::Yank, "y") | (Op::Change, "c")
                );
                if expect_double && doubled {
                    return VimAction::LineOp(kind, usize::MAX); // row filled in by shell
                }
                return match key {
                    "w" => match kind {
                        Op::Delete => VimAction::DispatchDelete(DeleteOp::ToNextWordEnd),
                        Op::Change => VimAction::ChangeThenInsert(DeleteOp::ToNextWordEnd),
                        Op::Yank => VimAction::Dispatch(Motion::NextWord),
                    },
                    "0" => match kind {
                        Op::Yank => VimAction::Dispatch(Motion::Home),
                        Op::Change => VimAction::ChangeThenInsert(DeleteOp::ToLineStart),
                        Op::Delete => VimAction::DispatchDelete(DeleteOp::ToLineStart),
                    },
                    "$" => match kind {
                        Op::Yank => VimAction::Dispatch(Motion::End),
                        Op::Change => VimAction::ChangeThenInsert(DeleteOp::ToLineEnd),
                        Op::Delete => VimAction::DispatchDelete(DeleteOp::ToLineEnd),
                    },
                    _ => VimAction::Consumed,
                };
            }
            Pending::None => {}
        }

        match key {
            "escape" => VimAction::Consumed,
            "h" => VimAction::Dispatch(Motion::Left),
            "j" => VimAction::Dispatch(Motion::Down),
            "k" => VimAction::Dispatch(Motion::Up),
            "l" => VimAction::Dispatch(Motion::Right),
            "0" => VimAction::Dispatch(Motion::Home),
            "$" => VimAction::Dispatch(Motion::End),
            "g" => {
                self.pending = Pending::G;
                VimAction::Consumed
            }
            "G" => VimAction::Dispatch(Motion::Finish),
            "w" => VimAction::Dispatch(Motion::NextWord),
            "b" => VimAction::Dispatch(Motion::PrevWord),
            "x" => VimAction::DispatchDelete(DeleteOp::Char),
            "u" => VimAction::Undo,
            "d" | "c" | "y" => {
                let kind = match key {
                    "d" => Op::Delete,
                    "c" => Op::Change,
                    _ => Op::Yank,
                };
                self.pending = Pending::Operator {
                    kind,
                    expect_double: true,
                };
                VimAction::Consumed
            }
            "p" => VimAction::Paste,
            "i" => {
                self.mode = VimMode::Insert;
                VimAction::EnterInsert {
                    append: false,
                    open_below: false,
                    open_above: false,
                }
            }
            "a" => {
                self.mode = VimMode::Insert;
                VimAction::EnterInsert {
                    append: true,
                    open_below: false,
                    open_above: false,
                }
            }
            "o" => {
                self.mode = VimMode::Insert;
                VimAction::EnterInsert {
                    append: false,
                    open_below: true,
                    open_above: false,
                }
            }
            "O" => {
                self.mode = VimMode::Insert;
                VimAction::EnterInsert {
                    append: false,
                    open_below: false,
                    open_above: true,
                }
            }
            "v" => {
                self.mode = VimMode::Visual;
                VimAction::EnterVisual { linewise: false }
            }
            "V" => {
                self.mode = VimMode::Visual;
                VimAction::EnterVisual { linewise: true }
            }
            ":" => {
                self.cmdline.push(':');
                VimAction::Consumed
            }
            "enter" => VimAction::Dispatch(Motion::Down),
            _ => {
                let _ = c;
                VimAction::Consumed
            }
        }
    }

    fn visual_key(&mut self, key: &str, _key_char: Option<char>) -> VimAction {
        match key {
            "escape" => {
                self.mode = VimMode::Normal;
                VimAction::LeaveVisual
            }
            "h" => VimAction::Dispatch(Motion::Left),
            "j" => VimAction::Dispatch(Motion::Down),
            "k" => VimAction::Dispatch(Motion::Up),
            "l" => VimAction::Dispatch(Motion::Right),
            "0" => VimAction::Dispatch(Motion::Home),
            "$" => VimAction::Dispatch(Motion::End),
            "w" => VimAction::Dispatch(Motion::NextWord),
            "b" => VimAction::Dispatch(Motion::PrevWord),
            "d" | "x" => VimAction::DispatchDelete(DeleteOp::Char), // shell maps to selection delete
            "y" => {
                self.mode = VimMode::Normal;
                VimAction::Consumed // shell yanks the selection itself
            }
            _ => VimAction::Consumed,
        }
    }

    pub fn set_mode(&mut self, mode: VimMode) {
        self.mode = mode;
        self.pending = Pending::None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(state: &mut VimState, key: &str) -> VimAction {
        state.handle_key(key, key.chars().next())
    }

    #[test]
    fn motions_in_normal() {
        let mut v = VimState::new();
        assert_eq!(run(&mut v, "h"), VimAction::Dispatch(Motion::Left));
        assert_eq!(run(&mut v, "l"), VimAction::Dispatch(Motion::Right));
        assert_eq!(run(&mut v, "0"), VimAction::Dispatch(Motion::Home));
        assert_eq!(run(&mut v, "$"), VimAction::Dispatch(Motion::End));
        assert_eq!(run(&mut v, "G"), VimAction::Dispatch(Motion::Finish));
    }

    #[test]
    fn gg_is_two_keys() {
        let mut v = VimState::new();
        assert_eq!(run(&mut v, "g"), VimAction::Consumed);
        assert_eq!(run(&mut v, "g"), VimAction::Dispatch(Motion::Start));
        // g followed by something else cancels the pending g (x is swallowed)
        assert_eq!(run(&mut v, "g"), VimAction::Consumed);
        assert_eq!(run(&mut v, "x"), VimAction::Consumed);
        // next x works normally
        assert_eq!(run(&mut v, "x"), VimAction::DispatchDelete(DeleteOp::Char));
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
        assert_eq!(
            run(&mut v, "w"),
            VimAction::DispatchDelete(DeleteOp::ToNextWordEnd)
        );
        // d0 / d$
        let mut v = VimState::new();
        run(&mut v, "d");
        assert_eq!(
            run(&mut v, "0"),
            VimAction::DispatchDelete(DeleteOp::ToLineStart)
        );
        run(&mut v, "d");
        assert_eq!(
            run(&mut v, "$"),
            VimAction::DispatchDelete(DeleteOp::ToLineEnd)
        );
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
        // caller routes insert keys to the editor, not here
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
    fn visual_motions_and_exit() {
        let mut v = VimState::new();
        run(&mut v, "v");
        assert_eq!(v.mode, VimMode::Visual);
        assert_eq!(run(&mut v, "l"), VimAction::Dispatch(Motion::Right));
        assert_eq!(run(&mut v, "escape"), VimAction::LeaveVisual);
        v.set_mode(VimMode::Normal);
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
}

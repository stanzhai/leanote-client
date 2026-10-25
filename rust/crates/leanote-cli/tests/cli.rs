//! CLI subprocess tests: byte-compare stdout/exit against the JS CLI where the
//! JS CLI can run offline (it needs no Electron runtime — it reads NeDB files;
//! we force its HTTP probe offline via LEANOTE_TEST env in the harness script).
//!
//! The fixture-derivation tests below encode outputs derived from reading
//! bin/leanote.js (== src/leanote_cli.js, verified identical) so they run
//! without node installed.

use std::io::Write;
use std::process::{Command, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_leanote-cli")
}

fn datadir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("leanote-cli-test-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn run(dir: &std::path::PathBuf, args: &[&str], stdin: Option<&str>) -> (String, String, i32) {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("LEANOTE_DATA", dir)
        .env("LEANOTE_NO_SHIM_SWITCH", "1")
        .env("LEANOTE_PORT", "8913")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn().unwrap();
    if let Some(s) = stdin {
        {
            let stdin_h = child.stdin.as_mut().unwrap();
            stdin_h.write_all(s.as_bytes()).unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn add_via_stdin_pipe() {
    let d = datadir("stdin");
    let (out, err, code) = run(
        &d,
        &["add", "-t", "Piped", "-g", "a, b"],
        Some("# Body\nline2"),
    );
    assert_eq!(code, 0, "stdout={} err={}", out, err);
    assert_eq!(out, "Added: \"Piped\" -> AI札 [a, b]\n");
    let (out2, _, _) = run(&d, &["list", "-v"], None);
    assert!(out2.contains("Piped"), "{}", out2);
    assert!(out2.contains("Body line2"), "desc joins newlines: {}", out2);
}

#[test]
fn add_flags_stdin_without_c() {
    let d = datadir("stdin2");
    let (out, _, code) = run(&d, &["-t", "Default"], Some("content-here"));
    assert_eq!(code, 0);
    assert_eq!(out, "Added: \"Default\" -> AI札\n");
}

#[test]
fn add_missing_title_or_content_errors_match_js() {
    // Derived from cmdAddInternal in leanote_cli.js
    let d = datadir("errs");
    let (_, err, code) = run(&d, &["add", "-c", "x"], None);
    assert_eq!((code, err.as_str()), (1, "Error: --title is required.\n"));
    let (_, err, code) = run(&d, &["add", "-t", "x"], None);
    assert_eq!(
        (code, err.as_str()),
        (1, "Error: --content is required (or pipe via stdin).\n")
    );
}

#[test]
fn show_prefix_semantics_match_js() {
    // Derived from cmdShow: exact/prefix match against newest-first list,
    // first match wins on ambiguity (no ambiguity error in JS show),
    // 'Error: Note "X" not found.' + exit 1 when none.
    let d = datadir("show");
    run(&d, &["add", "-t", "A", "-c", "aa"], None);
    let (l, _, _) = run(&d, &["list"], None);
    let id = l
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    // unique prefix
    let (out, _, code) = run(&d, &["show", &id[..6]], None);
    assert_eq!(code, 0);
    assert!(out.starts_with(&format!("ID:       {}", id)));
    assert!(out.contains("---\naa\n"));
    // no match
    let (_, err, code) = run(&d, &["show", "zzzzzz"], None);
    assert_eq!(
        (code, err.as_str()),
        (1, "Error: Note \"zzzzzz\" not found.\n")
    );
    // missing arg
    let (_, err, code) = run(&d, &["show"], None);
    assert_eq!((code, err.as_str()), (1, "Error: <note-id> is required.\n"));
    // empty string arg: JS parseShowArgs sets noteId only for non-dash args,
    // so an empty positional leaves noteId "" -> same required error
    let (_, err, code) = run(&d, &["show", ""], None);
    assert_eq!((code, err.as_str()), (1, "Error: <note-id> is required.\n"));
}

#[test]
fn show_ambiguous_prefix_takes_first_newest_match_like_js() {
    let d = datadir("show-amb");
    run(&d, &["add", "-t", "first", "-c", "one"], None);
    run(&d, &["add", "-t", "second", "-c", "two"], None);
    // rewrite both ids to share prefix 'dddd' (same insertion+time semantics)
    let db = d.join("leanote.db");
    let c = rusqlite::Connection::open(&db).unwrap();
    c.execute("UPDATE notes SET NoteId='dddd11111111111111111111', UpdatedTime=1774000000000 WHERE rowid=1", []).unwrap();
    c.execute("UPDATE notes SET NoteId='dddd22111111111111111111', UpdatedTime=1774000000000 WHERE rowid=2", []).unwrap();
    drop(c);
    // JS: loadNotes stable-sorts equal UpdatedTime in file order (insertion),
    // show takes the FIRST prefix hit = the earlier-inserted note.
    let (out, _, code) = run(&d, &["show", "dddd"], None);
    assert_eq!(code, 0);
    assert!(
        out.contains("ID:       dddd11111111111111111111"),
        "{}",
        out
    );
    assert!(out.contains("Title:    first"), "{}", out);
}

#[test]
fn list_output_shape_matches_js() {
    let d = datadir("list");
    run(&d, &["add", "-t", "T1", "-c", "body one", "-g", "x"], None);
    let (out, _, _) = run(&d, &["list", "-v"], None);
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines[0].contains("  T1"), "{}", out);
    assert_eq!(lines[1], "                         AI札 [x]");
    assert_eq!(lines[2], "       body one");
    // not found notebook message
    let (out, _, code) = run(&d, &["list", "-n", "Nope"], None);
    assert_eq!((code, out.as_str()), (0, "Notebook \"Nope\" not found.\n"));
    // no notes at all
    let d2 = datadir("list-empty");
    let (out, _, _) = run(&d2, &["list"], None);
    assert_eq!(out, "No notes found.\n");
}

#[test]
fn list_limit_more_message_matches_js() {
    let d = datadir("limit");
    for i in 0..3 {
        run(&d, &["add", "-t", &format!("n{}", i), "-c", "c"], None);
    }
    let (out, _, _) = run(&d, &["list", "-N", "2"], None);
    assert!(
        out.ends_with("... (1 more, use -N to show more)\n"),
        "{}",
        out
    );
}

#[test]
fn store_dir_is_leanote_never_nedb() {
    // ADR-0008: default (no LEANOTE_DATA) path is `leanote` — the same dir
    // the Electron app used — but only our SQLite file may appear there.
    // Run without the env override in a HOME sandbox and inspect.
    let home = datadir("home");
    std::fs::create_dir_all(&home).unwrap();
    let out = Command::new(bin())
        .args(["list"])
        .env_clear()
        .env("HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    let appsupport = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/leanote")
    } else {
        home.join(".config/leanote")
    };
    assert!(
        appsupport.join("leanote.db").exists(),
        "expected leanote.db"
    );
    assert!(
        !appsupport.join("nedb55").exists(),
        "must never create NeDB files"
    );
    let _ = String::from_utf8_lossy(&out.stdout);
}

#[test]
fn help_outputs_match_js_verbatim() {
    let (out, _, _) = run(&datadir("h"), &["--help"], None);
    assert!(out.starts_with("Usage: leanote <command> [options]\n\nCommands:\n"));
    assert!(out.contains("Run \"leanote <command> --help\" for command-specific help."));
    let (out, _, _) = run(&datadir("h"), &["add", "--help"], None);
    assert!(out.contains("  -n, --notebook <notebook>  Target notebook (default: \"AI札\")"));
    let (out, _, _) = run(&datadir("h"), &["list", "--help"], None);
    assert!(out.contains("  -N, --limit <n>          Max results (default: 20)"));
    let (out, _, _) = run(&datadir("h"), &["show", "--help"], None);
    assert!(out.contains("Show full content of a note. <note-id> can be a prefix."));
}

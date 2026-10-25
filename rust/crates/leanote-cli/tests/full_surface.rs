//! Ticket 07: search/edit output parity, dual-mode (HTTP probe vs offline
//! direct-write) invisibility, first-run shim switch behavior.
//!
//! Dual mode is exercised for real: `leanote serve` is spawned as a
//! subprocess on the default port (8912) for the online legs and killed for
//! the offline legs — no mocking of the probe.

use std::io::Write;
/// Port 8912 is process-global: cargo runs #[test] fns in parallel threads,
/// so every test that probes the API must hold this lock for its lifetime.
static PORT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct PortGuard(std::sync::MutexGuard<'static, ()>);
impl PortGuard {
    fn take() -> Self {
        // Safe: the lock is process-global and outlives the test harness.
        PortGuard(unsafe { std::mem::transmute(PORT_LOCK.lock().unwrap()) })
    }
}

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_leanote-cli")
}

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "leanote-t07-{}-{}-{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn home(name: &str) -> PathBuf {
    let h = tmpdir(name);
    std::fs::create_dir_all(h.join("bin")).unwrap();
    h
}

fn run(home_dir: &PathBuf, args: &[&str], stdin: Option<&str>) -> (String, String, i32) {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("HOME", home_dir)
        .env("LEANOTE_DATA", home_dir.join("data"))
        .env("LEANOTE_PORT", "8913")
        .env("TZ", "Asia/Shanghai")
        .env_remove("LEANOTE_NO_SHIM_SWITCH")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn().unwrap();
    if let Some(s) = stdin {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(s.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

fn quiet(home_dir: &PathBuf, args: &[&str], stdin: Option<&str>) -> (String, String, i32) {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("HOME", home_dir)
        .env("LEANOTE_DATA", home_dir.join("data"))
        .env("LEANOTE_NO_SHIM_SWITCH", "1")
        .env("LEANOTE_PORT", "8913")
        .env("TZ", "Asia/Shanghai")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn().unwrap();
    if let Some(s) = stdin {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(s.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Spawn `leanote serve` on the default port against `home`'s data dir.
fn spawn_server(home_dir: &PathBuf) -> std::process::Child {
    // A leaked `serve` from an earlier failed test would shadow ours
    // (bind -> AddrInUse -> silent exit). Clear the port first.
    let _ = Command::new("bash")
        .args(["-c", "pkill -f leanote\\ serve || true"])
        .status();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let child = Command::new(bin())
        .args(["serve", "-p", "8913"])
        .env("HOME", home_dir)
        .env("LEANOTE_DATA", home_dir.join("data"))
        .env("LEANOTE_NO_SHIM_SWITCH", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Wait for /api/ping to answer (bounded).
    for _ in 0..100 {
        if std::net::TcpStream::connect(("127.0.0.1", 8913)).is_ok() {
            return child;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("serve never came up on 8913");
}

fn seed(home_dir: &PathBuf) -> String {
    // two notes with deterministic ids via sqlite surgery
    quiet(
        home_dir,
        &[
            "add",
            "-t",
            "Meeting notes",
            "-c",
            "# Agenda\nline two with keyword",
        ],
        None,
    );
    quiet(home_dir, &["add", "-t", "Other", "-c", "zzz body"], None);
    let db = home_dir.join("data").join("leanote.db");
    let c = rusqlite::Connection::open(&db).unwrap();
    c.execute("UPDATE notes SET NoteId='aaaa11111111111111111111', UpdatedTime=1774000000000 WHERE Title='Meeting notes'", []).unwrap();
    c.execute("UPDATE notes SET NoteId='cccc22222222222222222222', UpdatedTime=1773999900000 WHERE Title='Other'", []).unwrap();
    drop(c);
    "aaaa11111111111111111111".to_string()
}

// --- search output parity ---

#[test]
fn search_output_matches_js_format() {
    let h = home("search");
    let _guard = PortGuard::take();
    let id = seed(&h);
    let id2 = "cccc22222222222222222222";

    // title+content hit with excerpt window (content match: excerpt shows
    // 30 units before .. kw+50 after; here keyword is near the start)
    let (out, err, code) = quiet(&h, &["search", "keyword"], None);
    assert_eq!(code, 0, "{}{}", out, err);
    assert_eq!(
        out,
        "{id}  2026-03-20 17:46  Meeting notes\n       # Agenda line two with keyword\n       AI札\n"
            .replace("{id}", &id),
        "{}",
        out
    );

    // tag/title miss, desc fallback excerpt for title-only hit
    let (out, _, code) = quiet(&h, &["search", "Other"], None);
    assert_eq!(code, 0);
    assert_eq!(
        out,
        format!("{id2}  2026-03-20 17:45  Other\n       zzz body\n       AI札\n"),
        "{}",
        out
    );

    // no match
    let (out, _, code) = quiet(&h, &["search", "nomatch"], None);
    assert_eq!(
        (code, out.as_str()),
        (0, "No notes matching \"nomatch\".\n")
    );

    // keyword required
    let (out, err, code) = quiet(&h, &["search"], None);
    assert_eq!((code, out.as_str()), (1, ""));
    assert_eq!(err, "Error: <keyword> is required.\n");

    // help parity
    let (out, _, code) = quiet(&h, &["search", "--help"], None);
    assert_eq!(code, 0);
    assert!(
        out.starts_with("Usage: leanote search <keyword> [options]\n"),
        "{}",
        out
    );
}

#[test]
fn search_limit_matches_js_more_message() {
    let h = home("search-limit");
    let _guard = PortGuard::take();
    seed(&h);
    let (out, _, _) = quiet(&h, &["search", "e", "-N", "1"], None);
    // "e" matches both titles + one content; -N 1 shows one + more-message
    assert!(
        out.ends_with("\n... (1 more, use -N to show more)\n")
            || out.ends_with("\n... (0 more, use -N to show more)\n"),
        "{}",
        out
    );
}

// --- edit parity ---

#[test]
fn edit_flags_and_errors_match_js() {
    let h = home("edit");
    let _guard = PortGuard::take();
    let id = seed(&h);

    let (out, _, code) = quiet(&h, &["edit", "aaaa1", "-t", "Renamed"], None);
    assert_eq!((code, out.as_str()), (0, "Updated: aaaa1...\n"));

    // stdin replaces content unconditionally (JS edit path)
    let (out, _, code) = quiet(&h, &["edit", "aaaa1"], Some("piped body"));
    assert_eq!((code, out.as_str()), (0, "Updated: aaaa1...\n"));

    // prefix + -g; ambiguity resolves to first newest match like show
    let (out, _, code) = quiet(&h, &["edit", "cccc2", "-g", "a , b"], None);
    assert_eq!((code, out.as_str()), (0, "Updated: cccc2...\n"));

    // no fields
    let (out, err, code) = quiet(&h, &["edit", &id], None);
    assert_eq!(
        (code, out.as_str(), err.as_str()),
        (1, "", "Error: No fields to update.\n")
    );

    // missing id
    let (out, err, code) = quiet(&h, &["edit"], None);
    assert_eq!((code, err.as_str()), (1, "Error: <note-id> is required.\n"));

    // unknown note
    let (out, err, code) = quiet(&h, &["edit", "zzzz", "-t", "x"], None);
    assert_eq!(
        (code, err.as_str()),
        (1, "Error: Note \"zzzz\" not found.\n")
    );

    // unknown notebook
    let (out, err, code) = quiet(&h, &["edit", "aaaa1", "-n", "Nope"], None);
    assert_eq!(
        (code, err.as_str()),
        (1, "Error: Notebook \"Nope\" not found.\n")
    );

    // help parity
    let (out, _, code) = quiet(&h, &["edit", "--help"], None);
    assert_eq!(code, 0);
    assert!(
        out.starts_with("Usage: leanote edit <note-id> [options]\n"),
        "{}",
        out
    );
    let (out, _, _) = quiet(&h, &["help", "edit"], None);
    assert_eq!(out, quiet(&h, &["edit", "-h"], None).0);
}

#[test]
fn edit_offline_marks_dirty_and_recomputes_desc() {
    let h = home("edit-dirty");
    let _guard = PortGuard::take();
    let id = seed(&h);
    quiet(
        &h,
        &["edit", "aaaa1", "-c", "# Fresh\nbody with hash"],
        None,
    );
    let db = h.join("data").join("leanote.db");
    let c = rusqlite::Connection::open(&db).unwrap();
    let (dirty, desc, abstract_, content): (i64, String, String, String) = c
        .query_row(
            "SELECT IsDirty, \"Desc\", Abstract, Content FROM notes WHERE NoteId=?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    drop(c);
    assert_eq!(dirty, 1, "offline edit must set IsDirty for later push");
    assert_eq!(content, "# Fresh\nbody with hash");
    assert_eq!(
        desc, "Fresh body with hash",
        "JS: newlines->space, # stripped, [:50] trim"
    );
    assert_eq!(abstract_, "# Fresh\nbody with hash");

    // -n moves the note (JS persists NotebookId)
    let c = rusqlite::Connection::open(&db).unwrap();
    c.execute(
        "INSERT INTO notebooks VALUES('nbw1','user1','Work','',1,0,0,0,0,0,0,NULL)",
        [],
    )
    .unwrap();
    drop(c);
    quiet(&h, &["edit", "aaaa1", "-n", "Work"], None);
    let c = rusqlite::Connection::open(&db).unwrap();
    let nb: String = c
        .query_row("SELECT NotebookId FROM notes WHERE NoteId=?1", [&id], |r| {
            r.get(0)
        })
        .unwrap();
    drop(c);
    assert_eq!(nb, "nbw1");
}

// --- dual mode: identical stdout whether the local server is up or not ---

#[test]
fn dual_mode_output_is_mode_invisible() {
    let h_offline = home("dual-off");
    let _guard = PortGuard::take();
    let h_online = home("dual-on");
    seed(&h_offline);
    seed(&h_online);

    let cases: Vec<(&[&str], Option<&str>)> = vec![
        (&["list", "-v"], None),
        (&["show", "aaaa1"], None),
        (&["search", "keyword"], None),
        (&["search", "zzz"], None),
        (&["edit", "aaaa1", "-t", "Renamed"], None),
        (&["edit", "aaaa1"], Some("piped edit body")),
        (&["add", "-t", "Added both", "-c", "body"], None),
        (&["list"], None),
    ];
    // add generates a random id + current timestamp per side; normalize both.
    let norm = |s: String| -> String {
        let mut out = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c.is_ascii_hexdigit() {
                let mut run = c.to_string();
                while let Some(&n) = chars.peek() {
                    if n.is_ascii_hexdigit() {
                        run.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                if run.len() == 24 {
                    out.push_str("<ID>");
                } else {
                    out.push_str(&run);
                }
            } else if c.is_ascii_digit() {
                let mut run = c.to_string();
                while let Some(&n) = chars.peek() {
                    if n.is_ascii_digit() || n == '-' || n == ':' || n == ' ' {
                        run.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                if run.contains(':') {
                    out.push_str("<TIME>");
                } else {
                    out.push_str(&run);
                }
            } else {
                out.push(c);
            }
        }
        out
    };
    // Phase 1 — server DOWN: run every case against the offline store.
    let off: Vec<_> = cases
        .iter()
        .map(|(a, s)| quiet(&h_offline, a, *s))
        .collect();

    // Phase 2 — server UP: same sequence on a fresh identical store.
    let mut server = spawn_server(&h_online);
    for ((args, stdin), off) in cases.iter().zip(&off) {
        let on = quiet(&h_online, args, *stdin);
        eprintln!("CASE {:?} OFF: {:?} ON: {:?}", args, off, on);
        assert_eq!(
            norm(off.0.clone()),
            norm(on.0.clone()),
            "stdout differs for {:?}",
            args
        );
        assert_eq!(off.2, on.2, "exit code differs for {:?}", args);
    }

    // the HTTP-path edit really hit the store through the server
    let db = h_online.join("data").join("leanote.db");
    let c = rusqlite::Connection::open(&db).unwrap();
    let (title, dirty): (String, i64) = c
        .query_row(
            "SELECT Title, IsDirty FROM notes WHERE NoteId='aaaa11111111111111111111'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    drop(c);
    assert_eq!(title, "Renamed");
    assert_eq!(dirty, 1);

    // offline mode also marked its copy dirty
    let db = h_offline.join("data").join("leanote.db");
    let c = rusqlite::Connection::open(&db).unwrap();
    let dirty: i64 = c
        .query_row(
            "SELECT IsDirty FROM notes WHERE NoteId='aaaa11111111111111111111'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(c);
    assert_eq!(dirty, 1);

    server.kill().unwrap();
    let _ = server.wait();

    // after the server dies, the CLI falls back to direct writes seamlessly
    let (out, _, code) = quiet(&h_online, &["edit", "aaaa1", "-t", "After shutdown"], None);
    assert_eq!((code, out.as_str()), (0, "Updated: aaaa1...\n"));
}

#[test]
fn http_mode_error_paths_match_js() {
    // Server up + note that doesn't exist: JS prints "API error: Note not found".
    let _guard = PortGuard::take();
    let h = home("http-err");
    seed(&h);
    let mut server = spawn_server(&h);
    let (out, err, code) = quiet(&h, &["edit", "zzzz9", "-t", "x"], None);
    assert_eq!((code, out.as_str()), (1, ""));
    assert_eq!(err, "API error: Note not found\n");
    server.kill().unwrap();
    let _ = server.wait();

    // Same input offline takes the file path: Note not found.
    let (_, err, code) = quiet(&h, &["edit", "zzzz9", "-t", "x"], None);
    assert_eq!(
        (code, err.as_str()),
        (1, "Error: Note \"zzzz9\" not found.\n")
    );
}

// --- shim switch ---

#[test]
fn shim_switch_rewrites_known_electron_shim_and_creates_when_absent() {
    // absent -> created, executable, points at this binary
    let h = home("shim-absent");
    run(&h, &["list"], None);
    let shim = h.join("bin").join("leanote");
    let body = std::fs::read_to_string(&shim).unwrap();
    assert!(body.starts_with("#!/bin/sh\n"), "{}", body);
    assert!(body.contains("exec \""), "{}", body);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&shim).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "shim must be executable");
    }
    // `which leanote` resolution: the shim must run and produce CLI output
    let out = Command::new(shim.to_str().unwrap())
        .arg("list")
        .env("HOME", &h)
        .env("LEANOTE_DATA", h.join("data"))
        .output()
        .unwrap();
    assert!(out.status.success(), "shim must exec the Rust binary");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("Meeting notes")
            || String::from_utf8_lossy(&out.stdout).contains("No notes")
    );

    // one-shot: a second run does not touch it again
    let before = std::fs::read_to_string(&shim).unwrap();
    run(&h, &["list"], None);
    assert_eq!(before, std::fs::read_to_string(&shim).unwrap());
    assert!(
        h.join("data").join("shim-switched").exists(),
        "marker written"
    );

    // known Electron form -> replaced
    let h2 = home("shim-known");
    std::fs::write(
        h2.join("bin").join("leanote"),
        "#!/bin/sh\nNODE=\"node\"\nexec \"$NODE\" \"/Users/x/bin/leanote.js\" \"$@\"\n",
    )
    .unwrap();
    let (_, err, _) = run(&h2, &["list"], None);
    assert!(err.contains("Shim updated"), "{}", err);
    let body = std::fs::read_to_string(h2.join("bin").join("leanote")).unwrap();
    assert!(!body.contains("leanote.js"), "{}", body);

    // already switched marker -> untouched
    let h3 = home("shim-marker");
    std::fs::create_dir_all(h3.join("data")).unwrap();
    std::fs::write(h3.join("data").join("shim-switched"), b"").unwrap();
    std::fs::write(h3.join("bin").join("leanote"), "#!/bin/sh\nexec custom\n").unwrap();
    run(&h3, &["list"], None);
    assert_eq!(
        std::fs::read_to_string(h3.join("bin").join("leanote")).unwrap(),
        "#!/bin/sh\nexec custom\n"
    );
}

#[test]
fn shim_switch_refuses_unknown_shim_form() {
    let h = home("shim-unknown");
    std::fs::write(
        h.join("bin").join("leanote"),
        "#!/bin/sh\nexec my-own-wrapper \"$@\"\n",
    )
    .unwrap();
    let (_, err, _) = run(&h, &["list"], None);
    assert!(err.contains("not the known Electron shim"), "{}", err);
    assert_eq!(
        std::fs::read_to_string(h.join("bin").join("leanote")).unwrap(),
        "#!/bin/sh\nexec my-own-wrapper \"$@\"\n",
        "user-owned shim must be untouched"
    );
    // and the refusal is one-shot too (marker written)
    assert!(h.join("data").join("shim-switched").exists());
}

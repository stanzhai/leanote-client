//! Subcommand implementations mirroring bin/leanote.js exactly.
//! Output-format authority: src/leanote_cli.js == bin/leanote.js (verified identical).

use leanote_core::{NewNote, PrefixError, Store};
use std::io::{Read, Write};

pub fn run(argv: Vec<String>) -> i32 {
    shim_switch(); // first-run PATH switch to this binary (no-op afterwards)
    // JS: first arg that is a known subcommand wins; default "add".
    // `serve` is Rust-only (dev helper to run the local HTTP server).
    const SUBCOMMANDS: [&str; 8] =
        ["add", "list", "show", "search", "edit", "sync", "help", "serve"];
    let mut cmd = "add";
    let mut cmd_index: Option<usize> = None;
    for (i, a) in argv.iter().enumerate() {
        if SUBCOMMANDS.contains(&a.as_str()) {
            cmd = a.as_str();
            cmd_index = Some(i);
            break;
        }
    }

    if cmd == "help" {
        if argv.len() > 1 && SUBCOMMANDS.contains(&argv[1].as_str()) {
            print_help(&argv[1]);
        } else {
            print_global_help();
        }
        return 0;
    }

    if argv.iter().any(|a| a == "--help" || a == "-h") {
        match cmd_index {
            Some(_) => print_help(cmd),
            None => print_global_help(),
        }
        return 0;
    }

    let cmd_args: Vec<String> = match cmd_index {
        Some(i) => argv[i + 1..].to_vec(),
        None => argv.clone(),
    };

    // Every subcommand probes the local HTTP API first (JS checkAPI, ~300ms);
    // sync/serve never use it.
    let api = if cmd == "sync" || cmd == "serve" {
        Api::Offline
    } else {
        probe_api()
    };

    // Read stdin when piped (JS: !process.stdin.isTTY).
    let piped = if !atty_stdin() {
        read_stdin()
    } else {
        String::new()
    };

    match cmd {
        "add" => cmd_add(&cmd_args, &piped, api),
        "list" => cmd_list(&cmd_args),
        "show" => cmd_show(&cmd_args),
        "search" => cmd_search(&cmd_args),
        "sync" => cmd_sync(&cmd_args),
        "serve" => cmd_serve(&cmd_args),
        "edit" => cmd_edit(&cmd_args, &piped, api),
        _ => {
            eprintln!("Unknown command: {}", cmd);
            print_global_help();
            1
        }
    }
}


/// One-shot `GET /api/ping` against the local app (300ms) — invisible: the
/// mode only routes writes; stdout is identical either way.
enum Api {
    Online(ureq::Agent),
    Offline,
}

fn api_port() -> u16 {
    // LEANOTE_PORT moves the local API off the default 8912 — the user's real
    // Electron app legitimately owns 8912; tests use this for isolation.
    std::env::var("LEANOTE_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(leanote_http::DEFAULT_PORT)
}

fn probe_api() -> Api {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_millis(300))
        .build();
    match agent.get(&format!("http://{}:{}/api/ping", leanote_http::HOST, api_port())).call() {
        Ok(_) => Api::Online(agent),
        Err(_) => Api::Offline,
    }
}

/// POST JSON like the JS callAPI; returns the parsed body.
fn api_post(agent: &ureq::Agent, path: &str, body: &serde_json::Value) -> Option<serde_json::Value> {
    let resp = match agent
        .post(&format!("http://{}:{}{}", leanote_http::HOST, api_port(), path))
        .set("Content-Type", "application/json")
        .send_string(&body.to_string())
    {
        Ok(r) => r,
        // JS callAPI resolves the parsed body even on non-2xx (API error path).
        Err(ureq::Error::Status(_, r)) => r,
        Err(_) => return None,
    };
    resp.into_json().ok()
}

fn atty_stdin() -> bool {
    // No libc dependency: poll stdin. A pipe/file always reports readiness
    // (EOF counts); a tty does too, so use the FIONREAD-ish trick via /dev/tty?
    // Simplest std-only reliable check: fstat char-device on fd 0.
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        match std::fs::metadata("/dev/stdin") {
            Ok(md) => md.file_type().is_char_device(),
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn read_stdin() -> String {
    let mut s = String::new();
    let _ = std::io::stdin().read_to_string(&mut s);
    s.trim().to_string()
}

// --- add ---

struct AddOpts {
    title: String,
    content: String,
    tags: Vec<String>,
    notebook: String,
    help: bool,
}

fn parse_add_args(argv: &[String]) -> AddOpts {
    let mut o = AddOpts {
        title: String::new(),
        content: String::new(),
        tags: Vec::new(),
        notebook: String::new(),
        help: false,
    };
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        match a {
            "--title" | "-t" => {
                i += 1;
                o.title = argv.get(i).cloned().unwrap_or_default();
            }
            "--content" | "-c" => {
                i += 1;
                o.content = argv.get(i).cloned().unwrap_or_default();
            }
            "--tags" | "-g" => {
                i += 1;
                let raw = argv.get(i).cloned().unwrap_or_default();
                o.tags = parse_tags(&raw);
            }
            "--notebook" | "-n" => {
                i += 1;
                o.notebook = argv.get(i).cloned().unwrap_or_default();
            }
            "--help" | "-h" => o.help = true,
            _ => {}
        }
        i += 1;
    }
    o
}

/// JS: str.split(",").map(trim).filter(Boolean)
fn parse_tags(s: &str) -> Vec<String> {
    s.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()).map(String::from).collect()
}

fn cmd_add(args: &[String], piped: &str, api: Api) -> i32 {
    let mut opts = parse_add_args(args);
    if opts.help {
        print_help("add");
        return 0;
    }

    // JS main(): piped content wins unless -c was given.
    if !piped.is_empty() && !args.iter().any(|a| a == "-c" || a == "--content") {
        opts.content = piped.to_string();
    } else if opts.content.is_empty() {
        opts.content = piped.to_string();
    }

    if opts.title.is_empty() {
        eprintln!("Error: --title is required.");
        return 1;
    }
    if opts.content.is_empty() {
        eprintln!("Error: --content is required (or pipe via stdin).");
        return 1;
    }

    let store = match open_store() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };
    let target_notebook = if opts.notebook.is_empty() { "AI札".to_string() } else { opts.notebook.clone() };
    let tags_str = if !opts.tags.is_empty() {
        format!(" [{}]", opts.tags.join(", "))
    } else {
        String::new()
    };

    // HTTP mode: POST /api/note/addNote (JS addNote wrapper). Identical stdout;
    // transport failure falls through to the direct store write below.
    if let Api::Online(agent) = &api {
        let mut payload = serde_json::json!({
            "title": opts.title,
            "content": opts.content,
            "tags": opts.tags,
        });
        if !opts.notebook.is_empty() {
            payload["notebook"] = serde_json::Value::String(opts.notebook.clone());
        }
        if let Some(res) = api_post(agent, "/api/note/addNote", &payload) {
            if res.get("success").and_then(|s| s.as_bool()).unwrap_or(false) {
                println!("Added: \"{}\" -> {}{}", opts.title, target_notebook, tags_str);
                return 0;
            }
            eprintln!("API error: {}", res.get("error").and_then(|e| e.as_str()).unwrap_or(""));
            return 1;
        }
    }

    match store.add_note(NewNote {
        title: opts.title.clone(),
        content: opts.content.clone(),
        tags: opts.tags.clone(),
        notebook_title: target_notebook.clone(),
        is_markdown: true,
    }) {
        Ok(_) => {
            println!("Added: \"{}\" -> {}{}", opts.title, target_notebook, tags_str);
            0
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    }
}

fn open_store() -> Result<Store, String> {
    let dir = std::env::var("LEANOTE_RUST_DATA").unwrap_or_else(|_| {
        leanote_core::app_data_dir().to_string_lossy().into_owned()
    });
    Store::open(std::path::PathBuf::from(dir), "user1".to_string()).map_err(|e| e.to_string())
}

// --- sync ---

/// `leanote sync --host <url> --email <e> --password <p>`: login + sync.
/// Fresh login (LastSyncUsn<0) does a full pass; later runs do the
/// incremental pull+push round, like JS `incrSync`, so offline edits are
/// pushed instead of reverted by a re-pull.
/// Bad credentials exit 1 with the server's own Msg (e.g. WrongPassword).
fn cmd_sync(args: &[String]) -> i32 {
    let mut host = String::new();
    let mut email = String::new();
    let mut password = String::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--host" if i + 1 < args.len() => {
                host = args[i + 1].clone();
                i += 2;
            }
            "--email" if i + 1 < args.len() => {
                email = args[i + 1].clone();
                i += 2;
            }
            "--password" if i + 1 < args.len() => {
                password = args[i + 1].clone();
                i += 2;
            }
            _ => {
                eprintln!("Error: unknown sync argument: {}", args[i]);
                return 1;
            }
        }
    }
    if host.is_empty() || email.is_empty() || password.is_empty() {
        eprintln!("Error: sync requires --host, --email and --password");
        return 1;
    }
    let store = match open_store() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };
    let account = match leanote_core::sync::Api::login(&host, &email, &password, &store) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };
    println!("Logged in as {} <{}>", account.username, account.email);
    let api = leanote_core::sync::Api::for_account(&account);
    let run: fn(
        &leanote_core::sync::Api,
        &Store,
        &dyn leanote_core::sync::SyncProgress,
    ) -> Result<leanote_core::sync::SyncStats, leanote_core::sync::SyncError> =
        if store.last_sync_usn().unwrap_or(-1) >= 0 {
            leanote_core::sync::incr_sync
        } else {
            leanote_core::sync::full_sync
        };
    match run(&api, &store, &PrintProgress) {
        Ok(stats) => {
            println!(
                "Synced {} notebooks, {} notes, {} tags; {} contents fetched; LastSyncUsn={}",
                stats.notebooks, stats.notes, stats.tags, stats.contents,
                store.last_sync_usn().unwrap_or(-1)
            );
            0
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    }
}

struct PrintProgress;

impl leanote_core::sync::SyncProgress for PrintProgress {
    fn note(&self, msg: String) {
        println!("{msg}");
    }
}

// --- serve (Rust-only: run the local HTTP /api server; dev/test helper) ---

fn cmd_serve(args: &[String]) -> i32 {
    let mut port: u16 = api_port();
    let mut i = 0;
    while i < args.len() {
        if (args[i] == "-p" || args[i] == "--port") && i + 1 < args.len() {
            i += 1;
            match args[i].parse() {
                Ok(p) => port = p,
                Err(_) => {
                    eprintln!("Error: invalid port {}", args[i]);
                    return 1;
                }
            }
        }
        i += 1;
    }
    let store = match open_store() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };
    match leanote_http::serve_blocking(store, port) {
        Ok(()) => 0,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                eprintln!("[leanote] Port {} in use, CLI will use file I/O", port);
            } else {
                eprintln!("[leanote] Local server error: {}", e);
            }
            1
        }
    }
}

// --- list ---

struct ListOpts {
    notebook: String,
    tag: String,
    limit: i64,
    verbose: bool,
    help: bool,
}

fn parse_list_args(argv: &[String]) -> ListOpts {
    let mut o = ListOpts {
        notebook: String::new(),
        tag: String::new(),
        limit: 20,
        verbose: false,
        help: false,
    };
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        match a {
            "--notebook" | "-n" => {
                i += 1;
                o.notebook = argv.get(i).cloned().unwrap_or_default();
            }
            "--tag" | "-g" => {
                i += 1;
                o.tag = argv.get(i).cloned().unwrap_or_default();
            }
            "--limit" | "-N" => {
                i += 1;
                let raw = argv.get(i).cloned().unwrap_or_default();
                o.limit = raw.parse::<i64>().unwrap_or(20);
            }
            "--verbose" | "-v" => o.verbose = true,
            "--help" | "-h" => o.help = true,
            _ => {}
        }
        i += 1;
    }
    o
}

fn cmd_list(args: &[String]) -> i32 {
    let opts = parse_list_args(args);
    if opts.help {
        print_help("list");
        return 0;
    }

    let store = match open_store() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };

    let mut nb_id: Option<String> = None;
    if !opts.notebook.is_empty() {
        // JS: match by Title only, first match.
        let found = store
            .notebooks()
            .unwrap_or_default()
            .into_iter()
            .find(|nb| nb.title == opts.notebook)
            .map(|nb| nb.notebook_id.to_string());
        match found {
            Some(id) => nb_id = Some(id),
            None => {
                println!("Notebook \"{}\" not found.", opts.notebook);
                return 0;
            }
        }
    }

    let all = store.list_notes().unwrap_or_default();
    let notes: Vec<_> = all
        .into_iter()
        .filter(|n| nb_id.as_ref().map_or(true, |id| &n.notebook_id.to_string() == id))
        .filter(|n| opts.tag.is_empty() || n.tags.iter().any(|t| t == &opts.tag))
        .collect();

    let limit = if opts.limit > 0 { opts.limit } else { 20 };
    let mut count = 0;
    for n in notes.iter().take(limit as usize) {
        count += 1;
        let notebook = store.notebook_title(&n.notebook_id).unwrap_or_else(|_| n.notebook_id.to_string());
        let tags = if !n.tags.is_empty() {
            format!(" [{}]", n.tags.join(", "))
        } else {
            String::new()
        };
        let mut title = if n.title.is_empty() { "(untitled)".to_string() } else { n.title.clone() };
        // JS: substring by UTF-16 units — titles are typically short CJK/latin.
        let t16: String = title.chars().take(50).collect();
        if chars_len(&title) > 50 {
            let cut: String = title.chars().take(47).collect();
            title = format!("{}...", cut);
        } else {
            title = t16;
        }
        println!("{}  {}  {}", n.note_id, format_time(n.updated_time), title);
        println!("                         {}{}", notebook, tags);
        if opts.verbose {
            let preview: String = n.desc.chars().take(80).collect();
            println!("       {}", preview);
        }
    }

    if count == 0 {
        println!("No notes found.");
    } else if notes.len() as i64 > limit {
        println!("... ({} more, use -N to show more)", notes.len() as i64 - limit);
    }
    0
}

fn chars_len(s: &str) -> usize {
    s.chars().count()
}

// --- show ---

struct ShowOpts {
    note_id: String,
    help: bool,
}

fn parse_show_args(argv: &[String]) -> ShowOpts {
    let mut o = ShowOpts { note_id: String::new(), help: false };
    for a in argv {
        if a == "--help" || a == "-h" {
            o.help = true;
        } else if !a.starts_with('-') {
            o.note_id = a.clone();
        }
    }
    o
}

fn cmd_show(args: &[String]) -> i32 {
    let opts = parse_show_args(args);
    if opts.help {
        print_help("show");
        return 0;
    }
    if opts.note_id.is_empty() {
        eprintln!("Error: <note-id> is required.");
        return 1;
    }

    let store = match open_store() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };

    let resolved = match store.resolve_prefix(&opts.note_id) {
        Ok(Ok(note)) => note,
        Ok(Err(PrefixError::NotFound)) => {
            eprintln!("Error: Note \"{}\" not found.", opts.note_id);
            return 1;
        }
        Ok(Err(PrefixError::Ambiguous(_))) => {
            // JS show() takes the FIRST prefix match; no ambiguity error in show.
            let notes = store.list_notes().unwrap_or_default();
            match notes.iter().find(|n| n.note_id.starts_with(opts.note_id.as_str())) {
                Some(n) => n.clone(),
                None => {
                    eprintln!("Error: Note \"{}\" not found.", opts.note_id);
                    return 1;
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };

    let notebook = store.notebook_title(&resolved.notebook_id).unwrap_or_else(|_| resolved.notebook_id.to_string());
    let tags = if resolved.tags.is_empty() { "(none)".to_string() } else { resolved.tags.join(", ") };

    println!("ID:       {}", resolved.note_id);
    println!("Title:    {}", if resolved.title.is_empty() { "(untitled)".to_string() } else { resolved.title.clone() });
    println!("Notebook: {}", notebook);
    println!("Tags:     {}", tags);
    println!("Created:  {}", format_time(resolved.created_time));
    println!("Updated:  {}", format_time(resolved.updated_time));
    println!("---");
    if resolved.content.is_empty() {
        println!("(empty)");
    } else {
        let mut out = std::io::stdout();
        let _ = out.write_all(resolved.content.as_bytes());
        let _ = out.write_all(b"\n");
    }
    0
}

// --- search ---

struct SearchOpts {
    keyword: String,
    limit: i64,
    help: bool,
}

fn parse_search_args(argv: &[String]) -> SearchOpts {
    let mut o = SearchOpts { keyword: String::new(), limit: 20, help: false };
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--limit" | "-N" => {
                i += 1;
                let raw = argv.get(i).cloned().unwrap_or_default();
                o.limit = raw.parse::<i64>().unwrap_or(20);
            }
            "--help" | "-h" => o.help = true,
            a if !a.starts_with('-') => o.keyword = a.to_string(),
            _ => {}
        }
        i += 1;
    }
    o
}

fn cmd_search(args: &[String]) -> i32 {
    let opts = parse_search_args(args);
    if opts.help {
        print_help("search");
        return 0;
    }
    if opts.keyword.is_empty() {
        eprintln!("Error: <keyword> is required.");
        return 1;
    }

    let store = match open_store() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };

    let hits = match store.search(&opts.keyword, usize::MAX) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };
    let limit = if opts.limit > 0 { opts.limit as usize } else { 20 };
    let mut count = 0;
    for h in hits.iter().take(limit) {
        count += 1;
        let n = &h.note;
        let notebook = store
            .notebook_title(&n.notebook_id)
            .unwrap_or_else(|_| n.notebook_id.to_string());
        let tags = if !n.tags.is_empty() { format!(" [{}]", n.tags.join(", ")) } else { String::new() };
        let title = if n.title.is_empty() { "(untitled)".to_string() } else { n.title.clone() };
        println!("{}  {}  {}", n.note_id, format_time(n.updated_time), title);
        println!("       {}", h.excerpt);
        println!("       {}{}", notebook, tags);
    }
    if count == 0 {
        println!("No notes matching \"{}\".", opts.keyword);
    } else if hits.len() > limit {
        println!("... ({} more, use -N to show more)", hits.len() - limit);
    }
    0
}

// --- edit ---

struct EditOpts {
    note_id: String,
    title: Option<String>,
    content: Option<String>,
    tags: Option<Vec<String>>,
    notebook: Option<String>,
    help: bool,
}

fn parse_edit_args(argv: &[String]) -> EditOpts {
    let mut o = EditOpts {
        note_id: String::new(),
        title: None,
        content: None,
        tags: None,
        notebook: None,
        help: false,
    };
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        match a {
            "--title" | "-t" => {
                i += 1;
                o.title = Some(argv.get(i).cloned().unwrap_or_default());
            }
            "--content" | "-c" => {
                i += 1;
                o.content = Some(argv.get(i).cloned().unwrap_or_default());
            }
            "--tags" | "-g" => {
                i += 1;
                let raw = argv.get(i).cloned().unwrap_or_default();
                o.tags = Some(parse_tags(&raw));
            }
            "--notebook" | "-n" => {
                i += 1;
                o.notebook = Some(argv.get(i).cloned().unwrap_or_default());
            }
            "--help" | "-h" => o.help = true,
            a if !a.starts_with('-') => o.note_id = a.to_string(),
            _ => {}
        }
        i += 1;
    }
    o
}

/// JS cmdEditInternal: notebook lookup happens before the "no fields" check,
/// unknown title is fatal; piped content replaces -c unconditionally.
fn cmd_edit(args: &[String], piped: &str, api: Api) -> i32 {
    let mut opts = parse_edit_args(args);
    if opts.help {
        print_help("edit");
        return 0;
    }
    if !piped.is_empty() {
        opts.content = Some(piped.to_string());
    }

    if opts.note_id.is_empty() {
        eprintln!("Error: <note-id> is required.");
        return 1;
    }

    // JS: -n resolves to a NotebookId first; unknown title is fatal.
    let notebook_id = match opts.notebook.as_deref() {
        Some(nb) => {
            let store = match open_store() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    return 1;
                }
            };
            match store.notebooks().unwrap_or_default().into_iter().find(|n| n.title == nb) {
                Some(found) => Some(found.notebook_id.to_string()),
                None => {
                    eprintln!("Error: Notebook \"{}\" not found.", nb);
                    return 1;
                }
            }
        }
        None => None,
    };

    if opts.title.is_none()
        && opts.content.is_none()
        && opts.tags.is_none()
        && opts.notebook.is_none()
    {
        eprintln!("Error: No fields to update.");
        return 1;
    }

    // HTTP mode: POST /api/note/updateNote (JS editNote wrapper). The server
    // does prefix resolution + dirty marking; stdout identical to offline.
    if let Api::Online(agent) = &api {
        let mut payload = serde_json::json!({ "noteId": opts.note_id });
        if let Some(t) = &opts.title {
            payload["title"] = serde_json::Value::String(t.clone());
        }
        if let Some(c) = &opts.content {
            payload["content"] = serde_json::Value::String(c.clone());
        }
        if let Some(g) = &opts.tags {
            payload["tags"] = serde_json::json!(g);
        }
        if let Some(nb) = &opts.notebook {
            payload["notebook"] = serde_json::Value::String(nb.clone());
        }
        if let Some(res) = api_post(agent, "/api/note/updateNote", &payload) {
            if res.get("success").and_then(|s| s.as_bool()).unwrap_or(false) {
                println!("Updated: {}...", prefix8(&opts.note_id));
                return 0;
            }
            eprintln!("API error: {}", res.get("error").and_then(|e| e.as_str()).unwrap_or(""));
            return 1;
        }
        // transport failure -> fall through to direct store write
    }

    let store = match open_store() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };

    // JS updateNDB predicate: first note (newest-first list) whose id equals
    // or starts with the argument — no ambiguity error.
    let notes = match store.list_notes() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };
    let Some(target) = notes
        .iter()
        .find(|n| *n.note_id == opts.note_id || n.note_id.starts_with(opts.note_id.as_str()))
        .map(|n| n.note_id.clone())
    else {
        eprintln!("Error: Note \"{}\" not found.", opts.note_id);
        return 1;
    };

    match store.update_note(
        &leanote_core::Id(target.to_string()),
        leanote_core::NoteUpdate {
            title: opts.title.clone(),
            content: opts.content.clone(),
            tags: opts.tags.clone(),
            // NoteUpdate.notebook_title is a title in the http handler; here
            // we already hold the resolved id, so hand the id through.
            notebook_title: notebook_id,
        },
    ) {
        Ok(Some(_)) => {
            println!("Updated: {}...", prefix8(&opts.note_id));
            0
        }
        Ok(None) => {
            eprintln!("Error: Note \"{}\" not found.", opts.note_id);
            1
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    }
}

/// JS substring(0, 8) over the user-provided id argument (never the resolved id).
fn prefix8(s: &str) -> String {
    s.chars().take(8).collect()
}

// --- first-run shim switch (spec: 首启把 ~/bin/leanote 指向 Rust 二进制) ---

/// Only rewrite `~/bin/leanote` when it is absent or is the known Electron
/// shim (references leanote.js); a user-owned script is refused. One-shot via
/// a marker file in the data dir.
fn shim_switch() {
    if std::env::var_os("LEANOTE_NO_SHIM_SWITCH").is_some() {
        return;
    }
    let Ok(home) = std::env::var("HOME") else { return };
    if home.is_empty() {
        return;
    }
    let data_dir = std::env::var("LEANOTE_RUST_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| leanote_core::app_data_dir());
    let marker = data_dir.join("shim-switched");
    if marker.exists() {
        return;
    }
    let Ok(bin) = std::env::current_exe() else { return };
    let shim = std::path::Path::new(&home).join("bin").join("leanote");

    if let Ok(current) = std::fs::read_to_string(&shim) {
        if !current.contains("leanote.js") {
            eprintln!(
                "[leanote] {} is not the known Electron shim; leaving it untouched",
                shim.display()
            );
            let _ = std::fs::create_dir_all(&data_dir);
            let _ = std::fs::write(&marker, b"");
            return;
        }
    }

    let script = format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", bin.display());
    match std::fs::write(&shim, script) {
        Ok(()) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755));
            }
            eprintln!("[leanote] Shim updated: {} -> {}", shim.display(), bin.display());
        }
        Err(e) => eprintln!("[leanote] Could not update shim {}: {}", shim.display(), e),
    }
    let _ = std::fs::create_dir_all(&data_dir);
    let _ = std::fs::write(&marker, b"");
}


// --- formatting (mirrors JS formatTime: local time, ms timestamps) ---

fn format_time(ts: i64) -> String {
    if ts == 0 {
        return "----".to_string();
    }
    // Local-time conversion without chrono: use libc localtime via date cmd?
    // ponytail: shells out — no. Implement via TZ from /etc/localtime is heavy.
    // Use the platform `date` binary is fragile; instead do the arithmetic
    // against the local offset obtained from std (not available). So: call
    // libc::localtime through the `libc` crate? Avoid new dep — use
    // the C library via extern "C" declarations (no crate needed on unix).
    unsafe {
        let t: LibcTimeT = ts as LibcTimeT / 1000;
        let tm = libc_localtime(&t);
        if tm.is_null() {
            return "----".to_string();
        }
        let tm = &*tm;
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            tm.tm_year as i64 + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min
        )
    }
}

#[cfg(unix)]
extern "C" {
    #[link_name = "localtime"]
    fn libc_localtime(t: *const i64) -> *mut Tm;
}

#[cfg(unix)]
#[repr(C)]
struct Tm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    tm_gmtoff: i64,
    tm_zone: *const u8,
}

#[cfg(unix)]
type LibcTimeT = i64;

// --- help (verbatim from printHelp/printGlobalHelp) ---

fn print_help(cmd: &str) {
    match cmd {
        "add" | "" => {
            println!("Usage: leanote add [options]");
            println!("       leanote [options]                  (add is the default)");
            println!();
            println!("Add a markdown note to Leanote.");
            println!();
            println!("Options:");
            println!("  -t, --title <title>       Note title (required)");
            println!("  -c, --content <content>   Markdown content (use -c or pipe from stdin)");
            println!("  -g, --tags <tags>         Comma-separated tags");
            println!("  -n, --notebook <notebook>  Target notebook (default: \"AI札\")");
            println!("  -h, --help                Show this help");
            println!();
            println!("Examples:");
            println!("  leanote add -t \"Meeting\" -c \"# Notes\" -g \"work\"");
            println!("  leanote -t \"Quick\" -c \"content\"");
            println!("  echo \"# Hello\" | leanote -t \"Pipe test\"");
        }
        "list" => {
            println!("Usage: leanote list [options]");
            println!();
            println!("List recent notes.");
            println!();
            println!("Options:");
            println!("  -n, --notebook <name>    Filter by notebook");
            println!("  -g, --tag <tag>          Filter by tag");
            println!("  -N, --limit <n>          Max results (default: 20)");
            println!("  -v, --verbose            Show content preview");
            println!("  -h, --help               Show this help");
            println!();
            println!("Examples:");
            println!("  leanote list");
            println!("  leanote list -n \"AI札\" -v");
            println!("  leanote list -g \"work\" -N 5");
        }
        "show" => {
            println!("Usage: leanote show <note-id>");
            println!();
            println!("Show full content of a note. <note-id> can be a prefix.");
            println!();
            println!("Examples:");
            println!("  leanote show a1b2c3d4");
            println!("  leanote show a1b2");
        }
        "search" => {
            println!("Usage: leanote search <keyword> [options]");
            println!();
            println!("Search notes by keyword (title, content, tags).");
            println!();
            println!("Options:");
            println!("  -N, --limit <n>          Max results (default: 20)");
            println!("  -h, --help               Show this help");
            println!();
            println!("Examples:");
            println!("  leanote search \"meeting\"");
            println!("  leanote search \"project\" -N 10");
        }
        "edit" => {
            println!("Usage: leanote edit <note-id> [options]");
            println!();
            println!("Edit an existing note. <note-id> can be a prefix.");
            println!("At least one option must be provided.");
            println!();
            println!("Options:");
            println!("  -t, --title <title>       New title");
            println!("  -c, --content <content>   New markdown content (use -c or pipe from stdin)");
            println!("  -g, --tags <tags>         New comma-separated tags (replaces existing)");
            println!("  -n, --notebook <name>     Move to notebook");
            println!("  -h, --help                Show this help");
            println!();
            println!("Examples:");
            println!("  leanote edit a1b2c3d4 -t \"New Title\"");
            println!("  leanote edit a1b2 -g \"urgent,work\"");
            println!("  echo \"Updated content\" | leanote edit a1b2 -c -");
        }
        _ => print_global_help(),
    }
}

fn print_global_help() {
    println!("Usage: leanote <command> [options]");
    println!();
    println!("Commands:");
    println!("  add       Add a new note (default if no command given)");
    println!("  list      List recent notes");
    println!("  show      Show full note content by ID");
    println!("  search    Search notes by keyword");
    println!("  edit      Edit an existing note");
    println!();
    println!("Run \"leanote <command> --help\" for command-specific help.");
    println!();
    println!("Examples:");
    println!("  leanote add -t \"Note\" -c \"Content\" -g \"tag1,tag2\"");
    println!("  leanote list -n \"AI札\"");
    println!("  leanote search \"keyword\"");
    println!("  leanote show a1b2c3d4");
    println!("  leanote edit a1b2c3d4 -t \"New title\"");
}

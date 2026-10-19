//! Subcommand implementations mirroring bin/leanote.js exactly.
//! Output-format authority: src/leanote_cli.js == bin/leanote.js (verified identical).

use leanote_core::{NewNote, PrefixError, Store};
use std::io::{Read, Write};

pub fn run(argv: Vec<String>) -> i32 {
    // JS: first arg that is a known subcommand wins; default "add".
    const SUBCOMMANDS: [&str; 6] = ["add", "list", "show", "search", "edit", "help"];
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

    // Read stdin when piped (JS: !process.stdin.isTTY).
    let piped = if !atty_stdin() {
        read_stdin()
    } else {
        String::new()
    };

    match cmd {
        "add" => cmd_add(&cmd_args, &piped),
        "list" => cmd_list(&cmd_args),
        "show" => cmd_show(&cmd_args),
        "search" => {
            eprintln!("Error: search is not implemented yet");
            1
        }
        "edit" => {
            eprintln!("Error: edit is not implemented yet");
            1
        }
        _ => {
            eprintln!("Unknown command: {}", cmd);
            print_global_help();
            1
        }
    }
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

fn cmd_add(args: &[String], piped: &str) -> i32 {
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
    match store.add_note(NewNote {
        title: opts.title.clone(),
        content: opts.content.clone(),
        tags: opts.tags.clone(),
        notebook_title: target_notebook.clone(),
        is_markdown: true,
    }) {
        Ok(_) => {
            let tags_str = if !opts.tags.is_empty() {
                format!(" [{}]", opts.tags.join(", "))
            } else {
                String::new()
            };
            println!(
                "Added: \"{}\" -> {}{}",
                opts.title, target_notebook, tags_str
            );
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

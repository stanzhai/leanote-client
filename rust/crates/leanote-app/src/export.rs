//! Export: single note → HTML / Markdown file, whole store → `.leanote`
//! JSON backup (wire authority: `public/plugins/export_leanote/plugin.js`,
//! reader side `public/plugins/import_leanote/import.js`; HTML template
//! authority: `public/plugins/export_html/tpl/html.tpl`).
//!
//! All functions are pure (note in → string out) so they unit-test without
//! a window; `Store` input for the backup is read once and mapped.

use leanote_core::{Note, Notebook, Store};
use serde_json::{json, Map, Value};

/// `Date.format("yyyy-MM-dd hh:mm:ss")` from the export plugin — UTC, same
/// as the core store's timestamps.
fn leanote_time(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    // Howard Hinnant civil_from_days (same as app_shell::fmt_time).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// HTML-escape the five characters that change structure/attributes.
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Markdown → HTML with GFM (tables/strikethrough/tasklists/autolinks).
/// Raw HTML in the source passes through, matching the JS template which
/// splices note content in unescaped.
pub fn markdown_to_html(src: &str) -> String {
    markdown::to_html(src)
}

/// Markdown note → Markdown file. Content as stored, byte-for-byte —
/// GFM tables/code blocks are preserved by definition (no transformation).
pub fn note_to_markdown_file(note: &Note) -> String {
    note.content.clone()
}

/// Typography CSS for the exported page. Same rules as the JS plugin's
/// `tpl/html.tpl` (article-style: headings/lists/tables/code/links/quotes),
/// trimmed to what pure-Markdown notes can produce, inlined so the file is
/// standalone (the JS template also links sibling CSS; we have none).
const PAGE_CSS: &str = "\
body{margin:0;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,'Helvetica Neue',Arial,'PingFang SC','Microsoft YaHei',sans-serif;color:#1a1a1a;line-height:1.7;font-size:16px;background:#fff}
.note-container{max-width:780px;margin:auto;padding:32px 24px 64px}
#leanote-title{font-size:2em;margin:0 0 1em;font-weight:800;color:#0d0d0d;line-height:1.25;letter-spacing:-.03em;padding-bottom:.3em;border-bottom:1px solid #d8dee4}
h1,h2,h3,h4,h5,h6{margin-top:1.8em;margin-bottom:.5em;font-weight:700;line-height:1.3;color:#0d0d0d}
h1{font-size:1.8em}h2{font-size:1.45em}h3{font-size:1.2em}h4{font-size:1em}
p{margin:0 0 1.4em}
a{color:#0969da;text-decoration:none}
a:hover{text-decoration:underline}
strong,b{font-weight:600}
hr{margin:2.5em 0;border:none;border-top:1px solid #d8dee4}
img{max-width:100%;border-radius:8px;display:block;margin:1.5em auto}
ul,ol{padding-left:1.8em;margin:0 0 1.4em}
li{margin-bottom:.3em}
li>ul,li>ol{margin-bottom:0}
blockquote{margin:0 0 1.4em;padding:16px 24px;border-left:4px solid #0969da;background:#f6f8fa;border-radius:0 6px 6px 0;color:#57606a}
blockquote p:last-child{margin-bottom:0}
pre,code{font-family:ui-monospace,SFMono-Regular,'SF Mono',Menlo,Consolas,'Liberation Mono',monospace}
code{padding:2px 6px;font-size:.85em;color:#cf222e;background:#f6f8fa;border:1px solid #d8dee4;border-radius:4px;word-break:break-word}
pre{margin:0 0 1.4em;padding:20px;background:#f6f8fa;border:1px solid #d8dee4;border-radius:8px;overflow-x:auto;font-size:.85em;line-height:1.55}
pre code{padding:0;font-size:inherit;color:inherit;white-space:pre;background:transparent;border:none}
table{width:100%;margin:0 0 1.4em;border-collapse:collapse;font-size:.95em}
th,td{padding:10px 16px;border:1px solid #d8dee4;text-align:left;vertical-align:top}
th{font-weight:600;background:#f6f8fa}
tbody tr:nth-child(even){background:#fafbfc}
input[type=checkbox]{margin-right:.4em}
";

/// Standalone HTML document for one note. Markdown notes are rendered to
/// HTML (GFM); non-Markdown notes are already HTML and pass through raw,
/// both matching the JS export plugin's template substitution. Image srcs
/// are the server URLs already present in the content — no rewriting.
pub fn note_to_html(note: &Note) -> String {
    let title = escape_html(&note.title);
    let body = if note.is_markdown {
        markdown_to_html(&note.content)
    } else {
        note.content.clone()
    };
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"tool\" content=\"leanote-desktop-app\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n\
         <title>{title}</title>\n<style>\n{PAGE_CSS}</style>\n</head>\n\n\
         <body>\n\t<div class=\"note-container\">\n\t\t\
         <h1 class=\"title\" id=\"leanote-title\">{title}</h1>\n\t\t\
         <div class=\"content-html\" id=\"leanote-content\">{body}</div>\n\t</div>\n\
         </body>\n</html>\n"
    )
}

/// One embedded file row, JS shape: `{fileId, base64, md5, type, isAttach,
/// createdTime}` (+`title`, which the writer side reads).
fn file_row(file_id: &str, base64: &str, md5: &str, file_type: &str, is_attach: bool, created: &str) -> Value {
    json!({
        "fileId": file_id,
        "base64": base64,
        "md5": md5,
        "type": file_type,
        "title": "",
        "isAttach": is_attach,
        "createdTime": created,
    })
}

/// File extension → `type` field, same defaulting the JS side uses for
/// unknown types (it stores whatever the file row carried; we only have
/// local bytes here, so the extension is the type).
fn file_type_from(name: &str) -> String {
    let ext = name.rsplit('.').next().unwrap_or("");
    if ext.is_empty() || ext == name {
        "bin".to_string()
    } else {
        ext.to_ascii_lowercase()
    }
}

/// A note plus the base64 blobs to embed for it. `files` maps the 24-hex
/// fileIds referenced by the content (`leanote://file/getImage?fileId=` /
/// `getAttach?fileId=` after the backup rewrote them) to (bytes, is_attach).
pub fn backup_note(note: &Note, files: &[(String, Vec<u8>, bool)]) -> Value {
    // Rewrite content image/attach references to the leanote:// scheme the
    // import side's regexes expect (export_leanote plugin fixFiles).
    let mut content = note.content.clone();
    let mut file_rows: Vec<Value> = Vec::with_capacity(files.len());
    for (file_id, bytes, is_attach) in files {
        let b64 = base64_encode(bytes);
        let md5 = md5_hex(bytes);
        let kind = if *is_attach { "getAttach" } else { "getImage" };
        // Replace any source form that carried this fileId: the local
        // scheme, or the server API URL. Title text between []/() kept.
        let server_forms: Vec<String> = SERVER_HOSTS
            .with(|h| h.borrow().iter().map(|host| host.clone()).collect());
        for host in server_forms {
            let from = format!("{host}/api/file/{kind}?fileId={file_id}");
            let to = format!("leanote://file/{kind}?fileId={file_id}");
            content = content.replace(&from, &to);
        }
        file_rows.push(file_row(
            file_id,
            &b64,
            &md5,
            "bin",
            *is_attach,
            &leanote_time(note.updated_time),
        ));
    }
    json!({
        "title": note.title,
        "content": content,
        "tags": note.tags,
        "author": "",
        "isMarkdown": note.is_markdown,
        "createdTime": leanote_time(note.created_time),
        "updatedTime": leanote_time(note.updated_time),
        "files": file_rows,
    })
}

thread_local! {
    /// Known server origins (login host) whose `/api/file/...` URLs should be
    /// rewritten to `leanote://` in backups. Set once per export via
    /// [`set_backup_server_host`].
    static SERVER_HOSTS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Record the account host so backup content rewriting can match its URLs.
pub fn set_backup_server_host(host: &str) {
    let h = host.trim_end_matches('/').to_string();
    SERVER_HOSTS.with(|s| {
        let mut v = s.borrow_mut();
        v.clear();
        v.push(h);
    });
}

/// Whole store → `.leanote` backup JSON (top-level keys exactly as the JS
/// plugin: exportDate/app/appVersion/apiVersion/notes; each note the shape
/// `import_leanote/import.js` reads). Notebooks ride along as `notebooks`
/// (extra keys are ignored by the JS reader, which only walks `notes`).
pub fn backup_to_leanote_json(store: &Store) -> String {
    let notebooks: Vec<Notebook> = store.notebooks().unwrap_or_default();
    let notes: Vec<Note> = store
        .list_notes()
        .unwrap_or_default()
        .into_iter()
        .filter(|n| !n.is_trash && !n.is_deleted)
        .collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let note_rows: Vec<Value> = notes.iter().map(|n| backup_note(n, &[])).collect();
    let doc = json!({
        "exportDate": leanote_time(now),
        "app": "leanote.desktop.app.rust",
        "appVersion": env!("CARGO_PKG_VERSION"),
        "apiVersion": "0.1",
        "notebooks": notebooks.iter().map(|nb| json!({
            "notebookId": nb.notebook_id.to_string(),
            "title": nb.title,
            "parentNotebookId": nb.parent_notebook_id,
            "seq": nb.seq,
            "isDeleted": nb.is_deleted,
            "createdTime": leanote_time(nb.created_time),
            "updatedTime": leanote_time(nb.updated_time),
        })).collect::<Vec<_>>(),
        "notes": note_rows,
    });
    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

/// RFC 4648 standard base64, no wrapping (matches `new Buffer(...).toString
/// ('base64')`).
pub fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// MD5 (RFC 1321) hex digest — the backup `md5` field. Std-lib-only
/// implementation; correctness over speed (backups are not hot paths).
pub fn md5_hex(data: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
        5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
        4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
        6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
        0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());
    let (mut a0, mut b0, mut c0, mut d0) =
        (0x67452301u32, 0xefcdab89u32, 0x98badcfeu32, 0x10325476u32);
    for block in msg.chunks(64) {
        let mut m = [0u32; 16];
        for (i, w) in m.iter_mut().enumerate() {
            *w = u32::from_le_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let tmp = d;
            d = c;
            c = b;
            let sum = a
                .wrapping_add(f)
                .wrapping_add(K[i])
                .wrapping_add(m[g]);
            b = b.wrapping_add(sum.rotate_left(S[i]));
            a = tmp;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let digest = [a0.to_le_bytes(), b0.to_le_bytes(), c0.to_le_bytes(), d0.to_le_bytes()].concat();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Exported file name from a note title: the JS plugin's `fixFilename`
/// (unsafe punctuation → `-`, collapse `--`, trim trailing `-`).
pub fn safe_filename(title: &str) -> String {
    let mut name: String = title
        .chars()
        .map(|c| {
            if matches!(
                c,
                '/' | '#' | '$' | '!' | '^' | '*' | '\'' | ' ' | '"' | '%' | '&' | '(' | ')' | '+'
                    | ',' | ':' | ';' | '<' | '>' | '=' | '?' | '@' | '|' | '\\' | '\t' | '\n' | '\r'
            ) {
                '-'
            } else {
                c
            }
        })
        .collect();
    while name.contains("--") {
        name = name.replace("--", "-");
    }
    if name.len() > 1 {
        while name.ends_with('-') {
            name.pop();
        }
    }
    if name.is_empty() {
        "Untitled".to_string()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(content: &str, md: bool) -> Note {
        Note {
            note_id: leanote_core::Id("aabbccddeeff001122334455".into()),
            user_id: "u".into(),
            notebook_id: leanote_core::Id("nb00000000000000000000001".into()),
            title: "T & <title>".into(),
            content: content.into(),
            desc: String::new(),
            img_src: String::new(),
            tags: vec!["rust".into(), "export".into()],
            abstract_: String::new(),
            is_markdown: md,
            is_trash: false,
            is_dirty: false,
            is_new: false,
            is_deleted: false,
            is_blog: false,
            star: false,
            created_time: 1_760_000_000_000,
            updated_time: 1_760_100_000_000,
        }
    }

    #[test]
    fn markdown_file_is_content_verbatim() {
        let src = "# H\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n```rust\nfn f(){}\n```\n";
        assert_eq!(note_to_markdown_file(&note(src, true)), src);
    }

    #[test]
    fn html_renders_table_code_link_and_escapes_title() {
        let n = note(
            "# Head\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n```rust\nfn main(){}\n```\n\n[leanote](https://leanote.com) and `code`",
            true,
        );
        let html = note_to_html(&n);
        assert!(html.contains("<table>"), "table not rendered");
        assert!(html.contains("<th>a</th>"), "table header not rendered");
        assert!(html.contains("<pre><code"), "code block not rendered");
        assert!(html.contains("<a href=\"https://leanote.com\">"), "link not rendered");
        assert!(html.contains("<title>T &amp; &lt;title&gt;</title>"), "title unescaped");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("max-width:780px"), "inline css missing");
    }

    #[test]
    fn html_non_markdown_passes_raw_and_keeps_server_image_url() {
        let n = note("<p>hi</p><img src=\"http://s.example/api/file/getImage?fileId=5581029f6289dc3301000000\">", false);
        let html = note_to_html(&n);
        assert!(html.contains("<p>hi</p>"));
        assert!(html.contains("src=\"http://s.example/api/file/getImage?fileId=5581029f6289dc3301000000\""));
    }

    #[test]
    fn backup_note_matches_js_plugin_shape() {
        let n = note("![i](leanote://file/getImage?fileId=5581029f6289dc3301000000)", true);
        let files = vec![("5581029f6289dc3301000000".to_string(), vec![1u8, 2, 3], false)];
        let v = backup_note(&n, &files);
        // Field names the import plugin reads (parseNote/fixContentLink).
        for k in ["title", "content", "tags", "isMarkdown", "createdTime", "updatedTime", "files"] {
            assert!(v.get(k).is_some(), "missing field {k}");
        }
        let f = &v["files"][0];
        for k in ["fileId", "base64", "md5", "type", "isAttach", "createdTime"] {
            assert!(f.get(k).is_some(), "file missing {k}");
        }
        assert_eq!(f["base64"], base64_encode(&[1, 2, 3]));
        assert_eq!(f["md5"], md5_hex(&[1, 2, 3]));
        assert_eq!(v["tags"][0], "rust");
        assert!(v["isMarkdown"].as_bool().unwrap());
    }

    #[test]
    fn backup_rewrites_server_file_urls_to_leanote_scheme() {
        set_backup_server_host("http://s.example");
        let n = note(
            "![i](http://s.example/api/file/getImage?fileId=5581029f6289dc3301000000)\n[a](http://s.example/api/file/getAttach?fileId=aabbccddeeff001122334455)",
            true,
        );
        let files = vec![
            ("5581029f6289dc3301000000".to_string(), vec![0u8], false),
            ("aabbccddeeff001122334455".to_string(), vec![9u8], true),
        ];
        let v = backup_note(&n, &files);
        let c = v["content"].as_str().unwrap();
        assert!(c.contains("leanote://file/getImage?fileId=5581029f6289dc3301000000"), "image url not rewritten: {c}");
        assert!(c.contains("leanote://file/getAttach?fileId=aabbccddeeff001122334455"), "attach url not rewritten");
        assert!(v["files"][1]["isAttach"].as_bool().unwrap());
    }

    #[test]
    fn leanote_time_format() {
        assert_eq!(leanote_time(0), "1970-01-01 00:00:00");
        // 2025-12-09T04:00:00Z
        assert_eq!(leanote_time(1_760_235_200_000), "2025-12-09 04:00:00");
    }

    #[test]
    fn md5_known_vectors() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5_hex(b"The quick brown fox jumps over the lazy dog"),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
        // > one block (64 bytes) boundary
        let long = vec![b'a'; 1000];
        assert_eq!(md5_hex(&long).len(), 32);
    }

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn safe_filename_matches_js_fixfilename() {
        assert_eq!(safe_filename("a/b:c"), "a-b-c");
        assert_eq!(safe_filename("a--b  c"), "a-b-c");
        assert_eq!(safe_filename("trailing-"), "trailing");
        assert_eq!(safe_filename(""), "Untitled");
    }

    #[test]
    fn large_note_generates_without_pathology() {
        // 100KB markdown: one big table + long code + paragraphs.
        let mut src = String::with_capacity(110_000);
        src.push_str("# Big\n\n");
        for i in 0..2_000 {
            src.push_str(&format!("row {i} | cell {i} | more {i}\n"));
        }
        while src.len() < 50_000 {
            src.push_str("para with [link](https://leanote.com) and `code` and **bold**\n\n");
        }
        src.push_str("```rust\n");
        while src.len() < 100_000 {
            src.push_str("fn generated_code_line_more_text_here() {}\n");
        }
        src.push_str("```\n");
        let n = note(&src, true);
        let start = std::time::Instant::now();
        let html = note_to_html(&n);
        assert!(html.len() > 100_000);
        let _md = note_to_markdown_file(&n);
        // Guard against pathological quadratic behavior: 100KB must render
        // in well under a second (typical: single-digit ms).
        assert!(start.elapsed().as_millis() < 1_000, "html gen took {:?}", start.elapsed());
    }

    #[test]
    fn backup_json_round_trips_through_serde() {
        let dir = std::env::temp_dir().join(format!("leanote-export-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(dir.clone(), "user1".into()).unwrap();
        let n = store
            .add_note(leanote_core::NewNote {
                title: "Backup me".into(),
                content: "# body".into(),
                tags: vec!["t1".into()],
                notebook_title: "NB".into(),
                is_markdown: true,
            })
            .unwrap();
        let json_str = backup_to_leanote_json(&store);
        let v: Value = serde_json::from_str(&json_str).expect("backup JSON must parse");
        assert_eq!(v["notes"].as_array().unwrap().len(), 1);
        assert_eq!(v["notes"][0]["title"], "Backup me");
        assert_eq!(v["notes"][0]["files"].as_array().unwrap().len(), 0);
        assert_eq!(v["notebooks"].as_array().unwrap().len(), 1);
        for k in ["exportDate", "app", "appVersion", "apiVersion"] {
            assert!(v.get(k).is_some(), "missing top-level {k}");
        }
        let _ = n;
        let _ = std::fs::remove_dir_all(&dir);
    }
}

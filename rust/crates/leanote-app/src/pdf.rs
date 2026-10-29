//! Local PDF export: note Markdown → Typst markup → `typst::compile` →
//! `typst_pdf::pdf`. Pure Rust, no server (the user's server has no
//! wkhtmltopdf so `note/exportPdf` always answers `{"Ok":false}`).
//!
//! Subset (Ticket 15 successor): headings/paragraph/bold/italic/inline-code/
//! code-block/lists/blockquote/table/hr; images emit nothing (no network,
//! local byte caches are not reachable from a pure serializer); links render
//! text with the URL in parens. CJK works via system-font discovery
//! (`typst-kit` `scan-fonts`) with the crate's embedded fonts as a guaranteed
//! fallback — Typst itself does CJK line breaking and picks any font covering
//! the glyphs.

use markdown::mdast::Node;
use std::path::PathBuf;
use std::sync::LazyLock;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::LibraryExt;
use typst::World;

/// Pre-preamble every export shares: A4, 2cm margins, 11pt body text.
/// `#set text(lang: …)` is omitted: per-glyph font fallback (Typst picks the
/// first font that covers each glyph) is what makes CJK work with Latin
/// layout metrics.
const PREAMBLE: &str = "#set page(paper: \"a4\", margin: 2cm)\n#set text(size: 11pt)\n";

/// Note markdown → Typst markup. `title` becomes an `= ` heading; rich-text
/// notes arrive already converted by `browse::reader_markdown_src`.
pub fn markdown_to_typst(title: &str, md_src: &str) -> String {
    let opts = markdown::ParseOptions::gfm();
    let root = markdown::to_mdast(md_src, &opts).expect("gfm markdown has no syntax errors");
    let mut out = String::with_capacity(md_src.len() + 128);
    out.push_str(PREAMBLE);
    if !title.trim().is_empty() {
        out.push_str("= ");
        out.push_str(&escape_content(title));
        out.push_str("\n\n");
    }
    let Node::Root(r) = &root else { return out };
    blocks(&r.children, &mut out);
    out
}

/// Block-level nodes. Each appends the blank line that ends a Typst block.
/// Emitted markup sits in content mode, where `escape_content` applies.
fn blocks(children: &[Node], out: &mut String) {
    for n in children {
        match n {
            Node::Heading(h) => {
                out.push_str(&"=".repeat(h.depth.clamp(1, 6) as usize));
                out.push(' ');
                inlines(&h.children, out);
                out.push_str("\n\n");
            }
            Node::Paragraph(p) => {
                inlines(&p.children, out);
                out.push_str("\n\n");
            }
            Node::Code(c) => {
                // ```lang fence: raw block with an optional language tag.
                out.push_str("```");
                if let Some(l) = &c.lang {
                    out.push_str(l.split_whitespace().next().unwrap_or_default());
                }
                out.push('\n');
                out.push_str(&c.value);
                if !c.value.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n\n");
            }
            Node::List(l) => list(l, out),
            Node::Blockquote(b) => {
                out.push_str("#quote(block: true)[\n");
                blocks(&b.children, out);
                out.push_str("]\n\n");
            }
            Node::Table(t) => {
                let ncols = match t.children.first() {
                    Some(Node::TableRow(r)) => r.children.len(),
                    _ => 1,
                };
                out.push_str("#table(\n  columns: (");
                for _ in 0..ncols {
                    out.push_str("auto, ");
                }
                out.push_str("),\n  table.header(\n");
                if let Some(Node::TableRow(header)) = t.children.first() {
                    cells(&header.children, out, true);
                }
                out.push_str("  ),\n");
                for row in t.children.iter().skip(1) {
                    if let Node::TableRow(r) = row {
                        cells(&r.children, out, false);
                    }
                }
                out.push_str(")\n\n");
            }
            Node::ThematicBreak(_) => out.push_str("#line(length: 100%)\n\n"),
            // Definition/Html/frontmatter: leanote notes don't produce these.
            _ => {}
        }
    }
}

/// Unordered: `- item`; ordered: `+ item` (Typst numbers `+` lists itself;
/// a markdown `start` beyond 1 is rare enough to accept restarting at 1).
/// Task lists spell the checkbox out; typst `[x]` would open a content block.
fn list(l: &markdown::mdast::List, out: &mut String) {
    let marker = if l.ordered { "+ " } else { "- " };
    let indent = " ".repeat(marker.len());
    for item in &l.children {
        let Node::ListItem(i) = item else { continue };
        out.push_str(marker);
        if let Some(checked) = i.checked {
            out.push_str(if checked { "☒ " } else { "☐ " });
        }
        for (k, c) in i.children.iter().enumerate() {
            let sub = &mut String::new();
            blocks(std::slice::from_ref(c), sub);
            if k == 0 {
                // First child renders starting on the marker line.
                out.push_str(sub.trim_start());
            } else {
                // Further blocks (nested lists, paragraphs) align under text.
                out.push('\n');
                for line in sub.lines() {
                    out.push_str(&indent);
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
    }
    out.push('\n');
}

/// One table row; every cell is a `[...]` content block (code-mode args).
fn cells(children: &[Node], out: &mut String, header: bool) {
    for c in children {
        let Node::TableCell(t) = c else { continue };
        if header {
            out.push_str("    [*");
            inlines(&t.children, out);
            out.push_str("*],\n");
        } else {
            out.push_str("  [");
            inlines(&t.children, out);
            out.push_str("],\n");
        }
    }
}

/// Inline nodes, appended with no separators.
fn inlines(children: &[Node], out: &mut String) {
    for n in children {
        match n {
            Node::Text(t) => out.push_str(&escape_content(&t.value)),
            Node::Strong(s) => {
                out.push('*');
                inlines(&s.children, out);
                out.push('*');
            }
            Node::Emphasis(e) => {
                out.push('_');
                inlines(&e.children, out);
                out.push('_');
            }
            Node::Delete(d) => {
                out.push_str("#strike[");
                inlines(&d.children, out);
                out.push(']');
            }
            Node::InlineCode(c) => {
                out.push('`');
                out.push_str(&c.value.replace('\\', "\\\\").replace('`', "\\`"));
                out.push('`');
            }
            Node::Link(l) => {
                // Text with the URL in parens; no link element, no fetching.
                inlines(&l.children, out);
                if !l.url.is_empty() {
                    out.push_str(" (");
                    out.push_str(&escape_content(&l.url));
                    out.push(')');
                }
            }
            Node::Html(h) => out.push_str(&escape_content(&h.value)),
            Node::Image(_) => {} // skipped by contract
            Node::Break(_) => out.push_str("\\\n"),
            // Autolinks arrive as Link nodes; others have no text payload.
            _ => {}
        }
    }
}

/// Typst markup specials that must not act on user text. We are always
/// inside content mode (top level, `[]` blocks, or `[*…*]`), so `[`, `]`,
/// `#`, `*`, `_`, `` ` ``, `$`, `<`, `@`, `"`, `'`, `~`, and `\` are the
/// ones that carry meaning; `-`/`+`/`/`/`=` only matter at line starts,
/// where list markers and set rules would misparse — escaping them too
/// costs nothing and removes a whole class of position bugs.
fn escape_content(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' | '*' | '_' | '#' | '$' | '<' | '>' | '-' | '+' | '/' | '=' | '"' | '\'' | '~'
            | '`' | '@' | '[' | ']' => {
                o.push('\\');
                o.push(ch);
            }
            _ => o.push(ch),
        }
    }
    o
}

/// Minimal in-memory world: one virtual main source, the shared font store.
struct NoteWorld<'a> {
    library: LazyHash<typst::Library>,
    fonts: &'a typst_kit::fonts::FontStore,
    main_id: FileId,
    source: Source,
}

impl World for NoteWorld<'_> {
    fn library(&self) -> &LazyHash<typst::Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }
    fn main(&self) -> FileId {
        self.main_id
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main_id {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(PathBuf::from(
                id.vpath().get_with_slash(),
            )))
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.source(id)
            .map(|s| Bytes::from_string(s.text().to_string()))
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        // `datetime.today()` is only reachable via code we never emit.
        None
    }
}

/// System fonts + typst-kit's embedded Libertinus/DejaVu fallback. `FontPath`
/// entries load lazily on first use, so the scan cost is paid once per
/// process, not per font.
static FONTS: LazyLock<typst_kit::fonts::FontStore> = LazyLock::new(|| {
    let mut store = typst_kit::fonts::FontStore::new();
    store.extend(typst_kit::fonts::system());
    store.extend(typst_kit::fonts::embedded());
    store
});

/// Render one note (title + markdown body) to PDF bytes (A4, 2cm margins,
/// 11pt). CJK-safe: system fonts supply CJK glyphs and Typst breaks CJK
/// lines by default.
pub fn note_to_pdf_title(title: &str, markdown_src: &str) -> Result<Vec<u8>, String> {
    let src = markdown_to_typst(title, markdown_src);
    let main_id = FileId::new(RootedPath::new(
        VirtualRoot::Project,
        VirtualPath::new("note.typ").expect("plain filename"),
    ));
    let world = NoteWorld {
        library: LazyHash::new(typst::Library::default()),
        fonts: &FONTS,
        main_id,
        source: Source::new(main_id, src),
    };
    let doc = match typst::compile::<typst_layout::PagedDocument>(&world) {
        typst::diag::Warned { output: Ok(d), .. } => d,
        typst::diag::Warned {
            output: Err(errs), ..
        } => {
            return Err(fmt_diags(&errs));
        }
    };
    typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default()).map_err(|e| fmt_diags(&e))
}

/// First few diagnostic messages, semicolon-joined.
fn fmt_diags(errs: &[typst::diag::SourceDiagnostic]) -> String {
    errs.iter()
        .take(3)
        .map(|d| d.message.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializer_covers_headings_emphasis_code_table_and_cjk() {
        let md = "# 标题 One\n\n**bold** and *italic* and `code` 中文字符测试。\n\n\
                  | 姓名 | 值 |\n| --- | --- |\n| 甲 | 1 |\n| 乙 | 2 |\n\n\
                  ```rust\nfn f() {}\n```\n\n- 甲项\n- 乙项\n\n> 引用块\n\n\
                  [链接](https://leanote.com) and ![img](leanote://file/getImage?fileId=x)\n\n---\n";
        let t = markdown_to_typst("我的笔记", md);
        assert!(
            t.contains("#set page(paper: \"a4\", margin: 2cm)"),
            "preamble: {t}"
        );
        assert!(t.contains("= 我的笔记"), "title heading: {t}");
        assert!(t.contains("= 标题 One"), "h1 heading: {t}");
        assert!(t.contains("*bold*"), "bold: {t}");
        assert!(t.contains("_italic_"), "italic: {t}");
        assert!(t.contains("`code`"), "inline code: {t}");
        assert!(t.contains("中文字符测试"), "CJK text passthrough: {t}");
        assert!(t.contains("#table("), "table: {t}");
        assert!(t.contains("table.header("), "table header: {t}");
        assert!(t.contains("[*姓名*]"), "CJK header cell: {t}");
        assert!(t.contains("  [甲],"), "body cell is a content block: {t}");
        assert!(t.contains("columns: (auto, auto, )"), "columns array: {t}");
        assert!(t.contains("```rust\nfn f() {}\n```"), "fenced code: {t}");
        assert!(t.contains("- 甲项"), "list item: {t}");
        assert!(t.contains("#quote(block: true)["), "blockquote: {t}");
        assert!(t.contains("引用块"), "quote CJK: {t}");
        assert!(
            t.contains("(https:\\/\\/leanote.com)"),
            "link url in parens: {t}"
        );
        assert!(!t.contains("fileId"), "image skipped: {t}");
        assert!(t.contains("#line(length: 100%)"), "hr: {t}");
    }

    #[test]
    fn escapes_typst_specials_in_text() {
        let md = "a #1 $dollar <tag> /slash/ =eq \"quote\" ~tile @ref [brk] \\ bt\n\n`code span`\n";
        let t = markdown_to_typst("", md);
        assert!(t.contains("\\#1"), "hash escaped: {t}");
        assert!(t.contains("\\$"), "dollar escaped: {t}");
        assert!(t.contains("\\<tag\\>"), "html escaped to text: {t}");
        assert!(t.contains("\\/slash\\/"), "slash escaped: {t}");
        assert!(t.contains("\\~"), "tilde escaped: {t}");
        assert!(t.contains("\\@"), "at escaped: {t}");
        assert!(t.contains("\\[brk\\]"), "brackets escaped: {t}");
        assert!(t.contains("\\\\"), "backslash escaped: {t}");
        assert!(t.contains("`code span`"), "inline code kept verbatim: {t}");
    }

    #[test]
    fn inline_code_with_internal_backtick_is_escaped() {
        // markdown parses ``a ` b`` as one span containing a backtick.
        let md = "``a ` b``\n";
        let t = markdown_to_typst("", md);
        assert!(t.contains("`a \\` b`"), "internal backtick escaped: {t}");
    }

    #[test]
    fn ordered_list_and_task_list() {
        let md = "1. one\n2. two\n\n- [ ] todo\n- [x] done\n";
        let t = markdown_to_typst("", md);
        assert!(t.contains("+ one"), "ordered: {t}");
        assert!(t.contains("☐ todo"), "task open: {t}");
        assert!(t.contains("☒ done"), "task done: {t}");
    }

    /// Smoke test guarded on the environment having ANY usable font: CI
    /// without system fonts and without the embedded fallback would fail
    /// through no fault of the serializer, so skip rather than error there.
    #[test]
    fn note_to_pdf_smoke_cjk() {
        // ponytail: skip when the font store is empty (stripped CI image);
        // serializer coverage above is the contract check.
        if FONTS.book().families().count() == 0 {
            eprintln!("no fonts available; skipping pdf smoke test");
            return;
        }
        let bytes = note_to_pdf_title(
            "中文标题",
            "正文 **粗体** `code` 表格：\n\n| 甲 | 乙 |\n| --- | --- |\n| 1 | 2 |\n",
        )
        .unwrap();
        assert!(
            bytes.starts_with(b"%PDF"),
            "not a pdf: {:?}",
            &bytes[..8.min(bytes.len())]
        );
    }
}

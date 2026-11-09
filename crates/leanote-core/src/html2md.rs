//! Ticket 12: HTML→Markdown converter for legacy rich-text notes
//! (`IsMarkdown:false`, TinyMCE output).
//!
//! Thin wrapper over `htmd` (html5ever parse + Markdown writer, turndown
//! semantics): headings / nested ul+ol / tables (thead+tbody, rowspan +
//! colspan best-effort as plain GFM cells) / links / images / strong / em /
//! code / pre / blockquote all round-trip. Delta on top of the default
//! converter, matching the ticket's element list:
//! - `<s>`/`<del>`/`<strike>` → `~~…~~` (default drops them);
//! - `<u>` → plain text (Markdown has no underline; ticket accepts the loss).
//! Display path converts at render time and never persists; the explicit
//! 转存为 Markdown action persists via `Store::convert_to_markdown`.

use htmd::element_handler::Handlers;
use htmd::HtmlToMarkdown;

fn converter() -> HtmlToMarkdown {
    HtmlToMarkdown::builder()
        .add_handler(vec!["u"], |h: &dyn Handlers, e: htmd::Element| {
            let c = h.walk_children(e.node).content;
            if c.is_empty() { None } else { Some(c.into()) }
        })
        .add_handler(vec!["s", "del", "strike"], |h: &dyn Handlers, e: htmd::Element| {
            let c = h.walk_children(e.node).content;
            if c.is_empty() { None } else { Some(format!("~~{c}~~").into()) }
        })
        .build()
}

/// Convert a rich-text note's HTML body to Markdown.
/// Malformed HTML is repaired by the html5ever parser, never an Err.
pub fn html_to_markdown(html: &str) -> String {
    converter().convert(html).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings() {
        assert_eq!(html_to_markdown("<h1>a</h1>").trim(), "# a");
        assert_eq!(html_to_markdown("<h3>b</h3>").trim(), "### b");
        assert_eq!(html_to_markdown("<h6>c</h6>").trim(), "###### c");
    }

    #[test]
    fn nested_lists() {
        let md = html_to_markdown(
            "<ul><li>one<ol><li>first</li><li>second</li></ol></li><li>two</li></ul>",
        );
        assert!(md.contains("*   one"), "{md}");
        assert!(md.contains("    1.  first"), "{md}");
        assert!(md.contains("    2.  second"), "{md}");
        assert!(md.contains("*   two"), "{md}");
        // ordered start attribute survives
        assert!(html_to_markdown(r#"<ol start="3"><li>t</li></ol>"#).contains("3.  t"));
    }

    #[test]
    fn tables() {
        let md = html_to_markdown(
            "<table><thead><tr><th>Task</th><th>Owner</th></tr></thead>\
             <tbody><tr><td>write</td><td>alice</td></tr></tbody></table>",
        );
        assert!(md.contains("| Task  | Owner |"), "{md}");
        assert!(md.contains("| ----- | ----- |"), "{md}");
        assert!(md.contains("| write | alice |"), "{md}");
    }

    #[test]
    fn table_rowspan_colspan_best_effort() {
        // rowspan: cell repeats are dropped (GFM has no spans) but no row is lost.
        let md = html_to_markdown(
            "<table><thead><tr><th>H</th><th>C1</th></tr></thead>\
             <tbody><tr><td rowspan=\"2\">big</td><td>1</td></tr><tr><td>2</td></tr></tbody></table>",
        );
        assert_eq!(md.lines().count(), 4, "{md}");
        assert!(md.contains("| big | 1"), "{md}");
        assert!(md.contains("| 2"), "{md}");
        // colspan: cell text survives as one wide cell.
        assert!(html_to_markdown("<table><tr><td colspan=\"2\">wide</td></tr></table>").contains("wide"));
    }

    #[test]
    fn links_and_images() {
        let md = html_to_markdown(
            r#"<p><a href="https://leanote.com">site</a> <img src="https://l.com/i.png" alt="pic"></p>"#,
        );
        assert!(md.contains("[site](https://leanote.com)"), "{md}");
        assert!(md.contains("![pic](https://l.com/i.png)"), "{md}");
        assert!(html_to_markdown(r#"<img src="/i.png">"#).contains("![](/i.png)"));
    }

    #[test]
    fn bold_italic_code() {
        let md = html_to_markdown(
            "<p><strong>b</strong> <em>i</em> <code>c</code> <s>s</s> <u>u</u></p>",
        );
        assert!(md.contains("**b**"), "{md}");
        assert!(md.contains("*i*"), "{md}");
        assert!(md.contains("`c`"), "{md}");
        assert!(md.contains("~~s~~"), "{md}"); // ticket: strikethrough delta
        assert!(md.contains("u"), "{md}"); // underline degrades to text
        // fenced code block keeps its language
        let pre = html_to_markdown(r#"<pre><code class="language-rust">fn x() {}</code></pre>"#);
        assert!(pre.contains("```rust"), "{pre}");
        assert!(pre.contains("fn x() {}"), "{pre}");
    }

    #[test]
    fn blockquote() {
        let md = html_to_markdown("<blockquote><p>quoted</p></blockquote>");
        assert!(md.contains("> quoted"), "{md}");
        // nested
        let deep = html_to_markdown("<blockquote><blockquote><p>d</p></blockquote></blockquote>");
        assert!(deep.contains("> > d"), "{deep}");
    }

    #[test]
    fn malformed_html_and_plain_text_do_not_error() {
        assert_eq!(html_to_markdown(""), "");
        assert!(html_to_markdown("plain text, no tags").contains("plain text"));
        let unclosed = html_to_markdown("<p>unclosed <b>bold");
        assert!(unclosed.contains("**bold**"), "{unclosed}");
        // entities decode
        assert!(html_to_markdown("<p>a &amp; b</p>").contains("a & b"));
    }
}

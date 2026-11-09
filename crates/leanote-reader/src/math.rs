//! LaTeX math rendering (issue 22 deferred half, ADR-0011 approach): `$…$`
//! inline and `$$…$$` block formulas become SVG via mitex (LaTeX→Typst
//! math) + in-process typst compile + typst-svg, displayed through gpui's
//! native SVG image support — same pure-Rust, no-webview path as mermaid.
//! Source editor stays text; rendering is a preview-time concern.

use gpui_base::{InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext};
use gpui_kit::component::text::MarkdownPlugin;
use gpui_kit::component::text::TextView;
use gpui_kit::prelude::*;
use gpui_kit::{div, img, px, rgb, App, Image, ImageFormat, Window};
use markdown::mdast::Node;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::LibraryExt;
use typst::World;

/// Embedded (Libertinus/New Computer Modern Math) + system fonts. Math
/// glyphs are covered offline by the embedded set; system scan adds CJK
/// inside formulas. One scan per process.
static FONTS: LazyLock<typst_kit::fonts::FontStore> = LazyLock::new(|| {
    let mut store = typst_kit::fonts::FontStore::new();
    store.extend(typst_kit::fonts::embedded());
    store.extend(typst_kit::fonts::system());
    store
});

/// (latex, display, dark) → SVG. Compilation is ~ms per formula but not
/// free; notes re-render formulas every preview toggle.
type MathCache = Mutex<HashMap<(String, bool, bool), Result<String, String>>>;
static CACHE: LazyLock<MathCache> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Render one LaTeX formula to an SVG string. `display` picks typst block
/// (`$ … $`, centered/larger) vs inline (`$…$`) math spacing. `Err` carries
/// the user-visible converter/compiler message.
pub fn render_math_svg(latex: &str, display: bool, dark: bool) -> Result<String, String> {
    let key = (latex.to_string(), display, dark);
    if let Some(hit) = CACHE.lock().unwrap().get(&key) {
        return hit.clone();
    }
    let out = render_uncached(latex, display, dark);
    CACHE.lock().unwrap().insert(key, out.clone());
    out
}

fn render_uncached(latex: &str, display: bool, dark: bool) -> Result<String, String> {
    let math = mitex::convert_math(latex, None).map_err(|e| format!("LaTeX: {e}"))?;
    // mitex 0.2.4 emits `diff` for `\partial` (pre-0.13 typst name for ∂);
    // typst 0.15 calls the symbol `partial` (`diff` is now unknown).
    // Boundary-aware swap: `x_diff` (subscript variable) stays `diff`.
    let math = rename_token(&math, "diff", "partial");
    compile_math(&math, display, dark)
}

/// Replace whole identifier tokens only (no alphanumeric/underscore
/// neighbors), otherwise keep the string untouched.
fn rename_token(src: &str, from: &str, to: &str) -> String {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while let Some(pos) = src[i..].find(from) {
        let at = i + pos;
        let before_ok = at == 0 || !is_word(src[..at].chars().next_back().unwrap());
        let end = at + from.len();
        let after_ok = src[end..].chars().next().is_none_or(|c| !is_word(c));
        if before_ok && after_ok {
            out.push_str(&src[i..at]);
            out.push_str(to);
            i = end;
        } else {
            out.push_str(&src[i..at + from.len()]);
            i = at + from.len();
        }
    }
    out.push_str(&src[i..]);
    out
}

fn compile_math(math: &str, display: bool, dark: bool) -> Result<String, String> {
    let fill = if dark { "#e5e7eb" } else { "#111827" };
    // Inline math in typst needs tight `$…$` delimiters; spaced `$ … $` is
    // display style. Page auto-sizes to the formula. `#let textmath` shims
    // the helper mitex emits for `\text{…}` (it lives in mitex's typst
    // package, absent here); identity — the `[…]` argument is already an
    // upright text content block.
    let src = format!(
        "#set page(width: auto, height: auto, margin: 2pt, fill: none)\n#set text(size: 12pt, fill: rgb(\"{fill}\"))\n#let textmath(c) = c\n{}{math}{}",
        if display { "$ " } else { "$" },
        if display { " $" } else { "$" },
    );
    compile_src(&src)
}

fn compile_src(src: &str) -> Result<String, String> {
    let main_id = FileId::new(RootedPath::new(
        VirtualRoot::Project,
        VirtualPath::new("math.typ").expect("plain filename"),
    ));
    let world = MathWorld {
        library: LazyHash::new(typst::Library::default()),
        fonts: &FONTS,
        main_id,
        source: Source::new(main_id, src.to_string()),
    };
    let doc = match typst::compile::<typst_layout::PagedDocument>(&world) {
        typst::diag::Warned { output: Ok(d), .. } => d,
        typst::diag::Warned {
            output: Err(errs), ..
        } => {
            return Err(errs
                .iter()
                .take(3)
                .map(|d| d.message.to_string())
                .collect::<Vec<_>>()
                .join("; "));
        }
    };
    let page = doc
        .pages()
        .first()
        .ok_or_else(|| "empty formula output".to_string())?;
    Ok(typst_svg::svg(page, &typst_svg::SvgOptions::default()))
}

/// Minimal in-memory world: one virtual main source, the shared font store.
/// Same shape as leanote-app's pdf.rs NoteWorld.
struct MathWorld<'a> {
    library: LazyHash<typst::Library>,
    fonts: &'a typst_kit::fonts::FontStore,
    main_id: FileId,
    source: Source,
}

impl World for MathWorld<'_> {
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
            Err(FileError::NotFound(std::path::PathBuf::from(
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
        None
    }
}

/// `viewBox="x y w h"` numbers of the root svg tag (typst-svg always emits
/// them, in pt).
fn viewbox(svg: &str) -> Option<(f32, f32)> {
    let start = svg.find("viewBox=\"")? + "viewBox=\"".len();
    let rest = &svg[start..];
    let end = rest.find('"')?;
    let mut it = rest[..end]
        .split_whitespace()
        .map(|n| n.parse::<f32>().ok());
    let _x = it.next()??;
    let _y = it.next()??;
    Some((it.next()??, it.next()??))
}

fn math_image(latex: &str, display: bool, dark: bool) -> Result<(Arc<Image>, f32, f32), String> {
    let svg = render_math_svg(latex, display, dark)?;
    // ponytail: pt→px 1:1 (96/72 ≈ 1.33 scale skipped); 12pt formulas sit
    // between preview body text and code size on both platforms tested.
    let (w, h) = viewbox(&svg).unwrap_or((60., 20.));
    let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes()));
    Ok((image, w, h))
}

struct InlineMathPlugin {
    /// Claims `$$…$$` written on a single line — mdast models that as an
    /// `InlineMath` node, not a block `Math`, but it is display math.
    display: bool,
    dark: bool,
}

impl MarkdownPlugin for InlineMathPlugin {
    fn name(&self) -> &str {
        if self.display {
            "math-single-line-display"
        } else {
            "math-inline"
        }
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::InlineMath(m) = node else {
            return None;
        };
        let is_display = cx
            .node_source(node)
            .is_some_and(|s| s.trim_start().starts_with("$$"));
        if is_display != self.display {
            return None; // the sibling instance claims it
        }
        // Fallback text is the raw LaTeX source: if conversion fails the
        // reader still sees what the author wrote.
        Some(MarkdownNode::new(self.name(), m.value.clone()).text(m.value.clone()))
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        _context: &InlineRenderContext,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<InlineElement> {
        let latex = node.as_text().to_string();
        let (image, w, h) = math_image(&latex, self.display, self.dark).ok()?;
        // ponytail: baseline at 80% height — approximates the math axis for
        // typical formulas; deep descenders (sum limits) may sit low.
        Some(
            InlineElement::new(img(gpui_kit::ImageSource::Image(image)).w(px(w)).h(px(h)))
                .with_baseline(px(h * 0.8)),
        )
    }
}

struct BlockMathPlugin {
    dark: bool,
}

impl MarkdownPlugin for BlockMathPlugin {
    fn name(&self) -> &str {
        "math-block"
    }

    fn is_block(&self) -> bool {
        true
    }

    fn parse(&self, node: &Node, _cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Math(m) = node else {
            return None;
        };
        Some(MarkdownNode::new("math-block", m.value.clone()).text(m.value.clone()))
    }

    fn render(&self, node: &MarkdownNode, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let latex = node.as_text().to_string();
        match math_image(&latex, true, self.dark) {
            Ok((image, w, h)) => gpui_kit::base::h_flex()
                .w_full()
                .py_2()
                .justify_center()
                .child(img(gpui_kit::ImageSource::Image(image)).w(px(w)).h(px(h)))
                .into_any_element(),
            // Fallback (ADR-0011 contract): source + one error line, never
            // a blank hole.
            Err(e) => div()
                .w_full()
                .py_2()
                .child(div().font_family("monospace").child(latex))
                .child(
                    gpui_kit::base::h_flex()
                        .py_1()
                        .text_size(px(12.))
                        .text_color(rgb(0xb91c1c))
                        .child(format!("Math: {e}")),
                )
                .into_any_element(),
        }
    }
}

/// Attach LaTeX math rendering to a markdown `TextView`. Call on every
/// preview surface alongside `mermaid::install_full`.
pub fn install(view: TextView, dark: bool) -> TextView {
    view.plugin(InlineMathPlugin {
        display: true,
        dark,
    })
    .plugin(InlineMathPlugin {
        display: false,
        dark,
    })
    .plugin(BlockMathPlugin { dark })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_user_example_formula() {
        let svg = render_math_svg(r"\partial\dot A/\partial A > 0", false, false)
            .expect("formula compiles");
        assert!(svg.starts_with("<svg"), "{svg}");
        assert!(viewbox(&svg).is_some());
    }

    #[test]
    fn display_and_dark_are_distinct_outputs() {
        let inline = render_math_svg("x^2", false, false).unwrap();
        let display = render_math_svg("x^2", true, false).unwrap();
        let dark = render_math_svg("x^2", false, true).unwrap();
        assert_eq!(inline, render_math_svg("x^2", false, false).unwrap()); // cached
        assert!(display != inline || display != dark);
    }

    #[test]
    fn bad_latex_reports_error_not_panic() {
        // mitex rejects unknown commands; that surfaces as Err (block math
        // renders source + error line), never a panic.
        let err = render_math_svg("\\unknowncmd{x}", false, false);
        assert!(err.unwrap_err().contains("unknown command"));
    }

    #[test]
    fn common_constructs_compile() {
        for src in [
            r"\frac{\partial u}{\partial t} = \sum_{i=1}^n i",
            r"E = mc^2",
            r"\alpha_1 \cdot \beta^{\gamma}",
        ] {
            render_math_svg(src, true, false).unwrap_or_else(|e| panic!("{src}: {e}"));
        }
    }

    #[test]
    fn text_with_cjk_compiles() {
        // User-reported shape: \text{…} blocks with CJK and parens.
        let src = r"\text{形式验证} > \text{可执行验证(测试)} > \text{LLM-as-judge}";
        let svg = render_math_svg(src, true, false).unwrap_or_else(|e| panic!("{e}"));
        assert!(svg.starts_with("<svg"), "{svg}");
    }
}

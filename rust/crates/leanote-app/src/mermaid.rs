//! Mermaid rendering (ADR-0011): fenced `mermaid` blocks become SVG via the
//! pure-Rust `merman` engine, displayed through gpui's native SVG image
//! support. One `install` helper wires the block parser + renderer into a
//! `TextView`, so every preview surface shares a single code path.
//!
//! Dark mode: merman's `dark` theme when the UI is dark, `default`
//! otherwise — no per-theme-family palettes (deliberate, ADR-0011).
//! `htmlLabels: false` keeps labels as SVG `<text>` instead of
//! `<foreignObject>`, which the usvg rasterizer cannot draw.

use gpui_base::{MarkdownNode, MarkdownParseContext};
use gpui_kit::component::text::TextView;
use gpui_kit::prelude::*;
use gpui_kit::{div, img, px, rgb, App, Image, ImageFormat, Window};
use markdown::mdast::Node;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;

/// Render one mermaid source to an SVG string. `Err` carries the
/// user-visible parse error line.
pub fn render_svg(src: &str, dark: bool) -> Result<String, String> {
    let cfg = if dark {
        serde_json::json!({
            "theme": "dark",
            "htmlLabels": false,
            "flowchart": {"htmlLabels": false},
            "sequence": {"useMaxWidth": false},
        })
    } else {
        serde_json::json!({
            "htmlLabels": false,
            "flowchart": {"htmlLabels": false},
            "sequence": {"useMaxWidth": false},
        })
    };
    let renderer = merman::Renderer::new().with_engine(
        merman::Engine::new().with_site_config(merman::MermaidConfig::from_value(cfg)),
    );
    match renderer.render(merman::RenderRequest::svg(
        src,
        merman::OperationControl::new(),
        merman::SvgRequest::default(),
    )) {
        Ok(merman::RenderOutput::Svg(Some(svg))) => {
            let svg = svg.svg().to_string();
            Ok(fit_svg(svg))
        }
        Ok(_) => Err("no SVG output".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// Fit the SVG to its ink: rewrite the root tag to the trimmed viewBox with
/// intrinsic px width/height.
///
/// merman emits `width="100%"` + `style="max-width: …px"` (the browser
/// responsive contract), and several families (gantt/pie/mindmap) also
/// reserve blank margins inside their own viewBox — a gantt keeps ~50px of
/// empty axis strip below the drawing at any task count. The gpui raster
/// element reserves the full canvas, so the user sees a large blank band
/// under the diagram. Rasterizing once at 1x to find the actual ink bbox
/// and cropping the viewBox to it removes both problems at once; the
/// percent width/max-width style become meaningless and are dropped.
/// Any parse/raster failure keeps the original SVG untouched.
fn fit_svg(svg: String) -> String {
    let Some((x, y, w, h)) = ink_viewbox(&svg, 2.) else {
        return svg;
    };
    intrinsic_root(svg, x, y, w, h)
}

/// Rewrite the root `<svg>` tag: viewBox ← `vb`, width/height ← its size.
///
/// merman emits `width="100%"` + `style="max-width: …px"`; both are dropped
/// (a percent width gives the gpui rasterizer no intrinsic aspect, and the
/// max-width CSS fights the pinned px size). Any other style declarations
/// survive. Attributes are strictly `name="value"` pairs (merman output);
/// any deviation bails and returns the original SVG untouched.
fn intrinsic_root(svg: String, x: f32, y: f32, w: f32, h: f32) -> String {
    let Some(gt) = svg.find('>') else { return svg };
    let head = &svg[..gt];
    let Some(tag_start) = head.find("<svg") else {
        return svg;
    };
    let tag = head[tag_start..].to_string();
    let vb = format!(r#"viewBox="{x} {y} {w} {h}""#);
    let mut new_tag = String::with_capacity(tag.len() + 32);
    let mut wrote_vb = false;
    let mut wrote_size = false;
    let mut rest = tag.as_str();
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].trim_end();
        let attr = name.rsplit(' ').next().unwrap_or(name);
        let Some(value) = rest[eq + 1..].strip_prefix('"') else {
            return svg;
        };
        let Some(end) = value.find('"') else {
            return svg;
        };
        let value = &value[..end];
        match attr {
            "width" | "height" => {
                if !wrote_size {
                    new_tag.push_str(&format!("width=\"{w}\" height=\"{h}\" "));
                    wrote_size = true;
                }
            }
            "viewBox" => {
                new_tag.push_str(&vb);
                new_tag.push(' ');
                wrote_vb = true;
            }
            "style" => {
                let kept = value
                    .split(';')
                    .map(str::trim)
                    .filter(|d| !d.is_empty() && !d.starts_with("max-width"))
                    .collect::<Vec<_>>()
                    .join("; ");
                if !kept.is_empty() {
                    new_tag.push_str(&format!("style=\"{kept}\" "));
                }
            }
            _ => new_tag.push_str(&format!("{attr}=\"{value}\" ")),
        }
        rest = &rest[eq + 2 + value.len() + 1..];
    }
    if !wrote_vb {
        new_tag.push_str(&vb);
        new_tag.push(' ');
    }
    if !wrote_size {
        new_tag.push_str(&format!("width=\"{w}\" height=\"{h}\" "));
    }
    let mut out = String::with_capacity(svg.len() + 32);
    out.push_str(&svg[..tag_start]);
    out.push_str("<svg ");
    out.push_str(new_tag.trim_start_matches("<svg "));
    out.push_str(&svg[tag_start + tag.len()..]);
    out
}

// ponytail: global cache, unbounded by diagram count but bounded by app
// lifetime; switch to a keyed LRU if huge notes ever make this a problem.
static CACHE: LazyLock<Mutex<HashMap<(u64, bool), Result<String, String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn render_cached(src: &str, dark: bool) -> Result<String, String> {
    let key = (fxhash(src.as_bytes()), dark);
    if let Ok(map) = CACHE.lock() {
        if let Some(hit) = map.get(&key) {
            return hit.clone();
        }
    }
    let rendered = render_svg(src, dark);
    if let Ok(mut map) = CACHE.lock() {
        map.insert(key, rendered.clone());
    }
    rendered
}

/// Cheap stable 64-bit hash (FNV-1a): inputs are note-sized sources, this
/// only keys a cache.
fn fxhash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Rasterize `svg` at 1x, find the ink bounding box, return a cropped
/// viewBox `(ox, oy, w, h)` in user space (honoring any viewBox origin)
/// with `margin` slack. `None` when the SVG fails to parse or draws
/// nothing — callers then keep the original canvas.
fn ink_viewbox(svg: &str, margin: f32) -> Option<(f32, f32, f32, f32)> {
    // System fonts are REQUIRED: `Options::default()` ships an empty
    // fontdb, so every <text> is dropped and the bbox misses all labels
    // (gpui's own rasterizer loads system fonts — the two must agree or
    // the crop cuts text that the real render draws).
    let mut opts = usvg::Options::default();
    let mut db = usvg::fontdb::Database::new();
    db.load_system_fonts();
    opts.fontdb = std::sync::Arc::new(db);
    let tree = usvg::Tree::from_str(svg, &opts).ok()?;
    let (w, h) = (tree.size().width(), tree.size().height());
    if w <= 0. || h <= 0. {
        return None;
    }
    let (ox, oy) = vb_nums(svg).map(|n| (n[0], n[1])).unwrap_or((0., 0.));
    let mut pm = resvg::tiny_skia::Pixmap::new(w.ceil() as u32, h.ceil() as u32)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(1., 1.),
        &mut pm.as_mut(),
    );
    let px = pm.pixels();
    let (ww, hh) = (pm.width(), pm.height());
    let mut minx = ww;
    let mut maxx = 0u32;
    let mut miny = hh;
    let mut maxy = 0u32;
    for y in 0..hh {
        for x in 0..ww {
            if px[(y * ww + x) as usize].alpha() > 0 {
                if x < minx {
                    minx = x
                }
                if x > maxx {
                    maxx = x
                }
                if y < miny {
                    miny = y
                }
                if y > maxy {
                    maxy = y
                }
            }
        }
    }
    if minx > maxx || miny > maxy {
        return None;
    }
    let x0 = minx as f32 - margin;
    let y0 = miny as f32 - margin;
    Some((
        x0 + ox,
        y0 + oy,
        (maxx + 1) as f32 + margin - x0,
        (maxy + 1) as f32 + margin - y0,
    ))
}

/// `viewBox="x y w h"` numbers, when present.
#[allow(non_snake_case)]
fn vb_nums(svg: &str) -> Option<[f32; 4]> {
    let start = svg.find("viewBox=\"")? + "viewBox=\"".len();
    let rest = &svg[start..];
    let end = rest.find('"')?;
    let mut it = rest[..end]
        .split_whitespace()
        .map(|n| n.parse::<f32>().ok());
    Some([it.next()??, it.next()??, it.next()??, it.next()??])
}

fn parse_mermaid_block(node: &Node, _cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
    let Node::Code(code) = node else { return None };
    if code.lang.as_deref()? != "mermaid" {
        return None;
    }
    Some(MarkdownNode::new("mermaid", code.value.clone()).text(code.value.clone()))
}

/// Attach mermaid rendering to a markdown `TextView`, plus the image
/// right-click menu. `shell` is the AppShell the preview belongs to; it
/// owns the menu surface and the save flow. `dark` is the current UI mode
/// so diagrams follow system light/dark.
pub fn install_full(
    view: TextView,
    dark: bool,
    shell: Option<gpui_kit::WeakEntity<crate::AppShell>>,
) -> TextView {
    // The diagram `img` is built by our own block renderer, NOT by the
    // markdown image path in node.rs — so the extension's click handler
    // (wired via `configure` below) never reaches it. Keep a copy here and
    // attach it to the rendered image directly.
    let diagram_menu = shell.clone().map(|shell| {
        move |url: &gpui_kit::SharedUri,
              source: &gpui_kit::ImageSource,
              event: &gpui_kit::ClickEvent,
              window: &mut Window,
              cx: &mut App| {
            crate::app_shell::on_image_menu(shell.clone(), url, source, event, window, cx)
        }
    });
    let view = view
        .markdown_block_parser(parse_mermaid_block)
        .markdown_block_renderer(
            "mermaid",
            move |node: &MarkdownNode, _window: &mut Window, _cx: &mut App| {
                let src = node.as_text().to_string();
                let container = div().w_full().min_w_0();
                match render_cached(&src, dark) {
                    Ok(svg) => {
                        let image = Arc::new(Image::from_bytes(
                            ImageFormat::Svg,
                            svg.clone().into_bytes(),
                        ));
                        // Intrinsic size (render_svg already pinned the
                        // root svg tag to the ink-trimmed viewBox); w only —
                        // h follows the aspect and stays right when
                        // max_w_full clamps wide diagrams into the pane.
                        let (w, h) = vb_nums(&svg).map(|n| (n[2], n[3])).unwrap_or((400., 300.));
                        let source = gpui_kit::ImageSource::Image(image);
                        let mut img = img(source.clone()).max_w_full().w(px(w)).h(px(h));
                        if let Some(menu) = diagram_menu.as_ref() {
                            let menu = menu.clone();
                            // No real URI: an in-memory SVG. A synthetic one
                            // keeps the shared handler signature and gives
                            // Save-as a stable file name.
                            let uri = gpui_kit::SharedUri::from("mermaid://diagram.svg");
                            img = img.on_click(move |event, window, cx| {
                                menu(&uri, &source, event, window, cx);
                            });
                        }
                        container
                            .child(gpui_kit::base::v_flex().items_start().py_2().child(img))
                            .into_any_element()
                    }
                    Err(e) => {
                        // Fallback (ADR-0011): the source as a plain code
                        // block + one error line, never a blank hole.
                        container
                            .child(
                                gpui_kit::base::v_flex()
                                    .py_2()
                                    .child(div().font_family("monospace").child(src)),
                            )
                            .child(
                                gpui_kit::base::h_flex()
                                    .py_1()
                                    .text_size(px(12.))
                                    .text_color(rgb(0xb91c1c))
                                    .child(format!("Mermaid: {e}")),
                            )
                            .into_any_element()
                    }
                }
            },
        );
    match shell {
        Some(shell) => install_image_menu(view, move |url, source, event, window, cx| {
            crate::app_shell::on_image_menu(shell.clone(), url, source, event, window, cx)
        }),
        None => view,
    }
}

// ---------------------------------------------------------------------------
// Image context menu (复制图片 / 另存为…)
// ---------------------------------------------------------------------------
//
// Right-click on a preview image opens a popup with Copy / Save-as. The
// plumbing is a MarkdownPlugin whose `configure` sets Base's
// `image_context_menu` handler — the one TextView hook every wrapper layer
// (component compat included) forwards through `.plugin()`.

/// A no-op plugin whose only job is carrying the image menu handler into
/// the Base TextView through the plugin path.
struct ImageMenuPlugin<F>(F);

impl<F> gpui_kit::component::text::MarkdownPlugin for ImageMenuPlugin<F>
where
    F: Fn(
            &gpui_kit::SharedUri,
            &gpui_kit::ImageSource,
            &gpui_kit::ClickEvent,
            &mut Window,
            &mut App,
        ) + Clone
        + Send
        + Sync
        + 'static,
{
    fn name(&self) -> &str {
        "image-menu"
    }

    fn parse(
        &self,
        _node: &Node,
        _cx: &gpui_kit::component::text::MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        None
    }

    fn configure(&self, view: gpui_kit::base::TextView) -> gpui_kit::base::TextView {
        let handler = self.0.clone();
        view.image_context_menu(handler)
    }
}

/// Attach the image right-click handler to a markdown `TextView`.
pub fn install_image_menu<F>(view: TextView, handler: F) -> TextView
where
    F: Fn(
            &gpui_kit::SharedUri,
            &gpui_kit::ImageSource,
            &gpui_kit::ClickEvent,
            &mut Window,
            &mut App,
        ) + Clone
        + Send
        + Sync
        + 'static,
{
    view.plugin(ImageMenuPlugin(handler))
}

#[cfg(test)]
mod intrinsic_tests {
    use super::*;

    #[test]
    fn pins_percent_width_and_strips_max_width() {
        let svg = r##"<svg id="merman" width="100%" xmlns="http://www.w3.org/2000/svg" class="flowchart" style="max-width: 168px;" viewBox="0 0 168 234.2"><g/></svg>"##;
        let out = intrinsic_root(svg.to_string(), 0., 0., 168., 234.2);
        assert!(out.contains(r#"width="168""#), "{out}");
        assert!(out.contains(r#"height="234.2""#), "{out}");
        assert!(!out.contains("max-width"), "{out}");
        assert!(!out.contains(r#"width="100%""#), "{out}");
        assert!(out.contains(r#"viewBox="0 0 168 234.2""#), "{out}");
        assert!(out.ends_with("<g/></svg>"), "{out}");
    }

    #[test]
    fn keeps_other_style_declarations() {
        let svg = r##"<svg width="100%" style="max-width: 400px; background-color: white" viewBox="0 0 400 300"><g/></svg>"##;
        let out = intrinsic_root(svg.to_string(), 0., 0., 400., 300.);
        assert!(out.contains("background-color: white"), "{out}");
        assert!(out.contains(r#"width="400""#), "{out}");
    }

    #[test]
    fn rewrite_replaces_existing_viewbox() {
        let svg = r##"<svg width="100%" viewBox="0 0 400 300"><g/></svg>"##;
        let out = intrinsic_root(svg.to_string(), 10., 20., 100., 50.);
        assert_eq!(out.contains(r#"viewBox="10 20 100 50""#), true, "{out}");
        assert!(!out.contains(r#"viewBox="0 0 400 300""#), "{out}");
    }

    #[test]
    fn ink_viewbox_crops_to_drawing() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 80"><rect x="10" y="10" width="20" height="10" fill="#000"/></svg>"##;
        assert_eq!(ink_viewbox(svg, 2.).unwrap(), (8., 8., 24., 14.));
    }

    #[test]
    fn ink_viewbox_honors_viewbox_origin() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-50 -10 200 100"><rect x="60" y="40" width="20" height="10" fill="#000"/></svg>"##;
        // raster col = 60+50 = 110 → x0 = 108 → user x = 108 - 50 = 58
        assert_eq!(ink_viewbox(svg, 2.).unwrap(), (58., 38., 24., 14.));
    }

    #[test]
    fn ink_viewbox_empty_returns_none() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 80"></svg>"##;
        assert!(ink_viewbox(svg, 2.).is_none());
    }

    #[test]
    fn ink_viewbox_counts_text_ink() {
        // Regression: an empty fontdb drops <text> from the bbox, so the
        // crop cut every label that the real (system-font) rasterizer
        // draws. A label placed below the shape must extend the box.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 80"><rect x="10" y="10" width="20" height="10" fill="#000"/><text x="10" y="70" font-size="12" fill="#000">lbl</text></svg>"##;
        let (_, y, _, h) = ink_viewbox(svg, 2.).unwrap();
        // text baseline y=70 ⇒ ink reaches ~70; box must extend well past
        // the shape-only bottom (22 + margin).
        assert!(y + h > 40., "text ink missing from bbox: y={y} h={h}");
    }

    #[test]
    fn render_svg_crop_keeps_gantt_axis_labels() {
        // The gantt axis dates sit in a text-only strip below the bars —
        // exactly the band an empty-fontdb crop would amputate.
        let src = "gantt\ndateFormat YYYY-MM-DD\nT1 :a1, 2026-01-01, 3d\nT2 :a2, 2026-01-05, 5d";
        let out = render_svg(src, false).unwrap();
        let n = vb_nums(&out).expect("viewBox");
        // raw canvas is 800x148; the crop must still contain the axis
        // labels (sys-font ink bottom ≈ 110 + margin) yet cut the blank
        // tail below them.
        assert!(n[1] + n[3] > 100., "axis label strip cut: {:?}", n);
        assert!(n[3] < 120., "blank tail not trimmed: {:?}", n);
    }

    #[test]
    fn render_svg_trims_gantt_blank() {
        // gantt reserves a constant ~50px empty axis strip below the last
        // task; the trimmed height must be well under the raw 124px canvas.
        let src = "gantt\ndateFormat YYYY-MM-DD\nsection S\nT :a1, 2026-01-01, 3d";
        let out = render_svg(src, false).unwrap();
        let n = vb_nums(&out).expect("viewBox");
        assert!(n[3] < 90., "trimmed h {} not under 90: {out}", n[3]);
        assert!(!out.contains(r#"width="100%""#), "{out}");
    }

    #[test]
    fn render_svg_emits_intrinsic_root() {
        let out = render_svg("graph TD\n  A --> B", false).unwrap();
        assert!(!out.contains(r#"width="100%""#), "{out}");
        let head = &out[..out.find('>').unwrap()];
        assert!(head.contains(r#"height=""#), "{head}");
    }
}

//! View-level math test: the exact TextView chain the preview surfaces
//! build — kit `TextView` + `leanote_reader::math::install` — must CLAIM
//! math nodes. Unclaimed math falls back to the literal `$…$` source
//! (dollars kept) — the user-reported "nothing renders" symptom.

use gpui_kit::component::text::{TextView, TextViewState};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Context, Entity, IntoElement, Render, TestAppContext, Window};

struct PreviewRoot {
    view: TextView,
    state: Entity<TextViewState>,
}

impl Render for PreviewRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(600.)).h(px(400.)).child(self.view.clone())
    }
}

fn rendered_text_of(src: &str, cx: &mut TestAppContext) -> String {
    let (root, cx) = cx.add_window_view(|_, cx| {
        let state = cx.new(|cx| TextViewState::markdown(src, cx));
        // Exact production chain (app_shell::mermaid_view): mermaid
        // install_full, then math::install on top.
        let view = leanote_reader::math::install(
            leanote_reader::mermaid::install_full(TextView::new(&state), false, None),
            false,
        );
        PreviewRoot { view, state }
    });
    cx.run_until_parked();
    let text = root.read_with(cx, |root, cx| {
        root.state.read(cx).rendered_text().as_str().to_string()
    });
    cx.run_until_parked();
    text
}
#[gpui_kit::test]
fn preview_claims_inline_and_display_math(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let text = rendered_text_of(
        "前置 $\\partial\\dot A/\\partial A > 0$ 后置\n\n$$\\text{形式验证} > \\text{内在自评}$$",
        cx,
    );
    // Claimed math contributes the node's fallback text (bare latex, no
    // dollars); the formula itself renders as an inline SVG image.
    assert!(
        !text.contains("$\\partial"),
        "inline math unclaimed, rendered text: {text:?}"
    );
    assert!(
        !text.contains("$$\\text"),
        "single-line display math unclaimed, rendered text: {text:?}"
    );
}

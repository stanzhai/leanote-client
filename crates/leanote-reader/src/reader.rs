//! 笔记内容 → 阅读态 markdown 源（issue 05 自 leanote-app browse.rs 搬入）。

use base64::Engine as _;
use gpui_kit::component::text::{TextView, TextViewStyle};
use gpui_kit::HighlightStyle;
use leanote_core::Store;

/// HTML 富文本笔记转 markdown 阅读（ADR-0001）；markdown 笔记原样。
pub fn reader_markdown_src(title: &str, content: &str, is_markdown: bool) -> String {
    if is_markdown {
        content.to_string()
    } else {
        let body = leanote_core::html2md::html_to_markdown(content);
        if body.trim().is_empty() {
            format!("# {title}\n")
        } else {
            body
        }
    }
}

/// `leanote://file/getImage?fileId=<id>` → data URL（本地 images 表字节）；
/// 远程图片链接补 token（对齐 Electron `src/file.js` 语义）。
pub fn resolve_note_images(content: &str, store: &Store) -> String {
    let mut out = content.to_string();
    for id in Store::image_ids_in_content(content) {
        if let Some(img) = store.get_image(&id).ok().flatten() {
            out = out.replace(
                &format!("leanote://file/getImage?fileId={id}"),
                &format!(
                    "data:{};base64,{}",
                    img.mime,
                    base64::engine::general_purpose::STANDARD.encode(&img.bytes)
                ),
            );
        }
    }
    let account = store.active_account().ok().flatten();
    if let Some(acct) = &account {
        if !acct.token.is_empty() && !acct.host.is_empty() {
            let host = acct.host.trim_end_matches('/');
            let token = acct.token.as_str();
            // Absolute links to our own host: append the token (the
            // server 500s private images for anonymous GETs).
            out = out.replace(
                &format!("{host}/api/file/getImage?fileId="),
                &format!("{host}/api/file/getImage?token={token}&fileId="),
            );
        }
    }
    // Relative links (the server's canonical form, what pulls store):
    // local bytes win (offline render), else absolutize with the token.
    // Skip occurrences inside absolute urls (a foreign host's tail) —
    // replacing those would corrupt the link.
    for id in Store::image_ids_in_content(content) {
        let data_or_remote = match store.get_image(&id).ok().flatten() {
            Some(img) => format!(
                "data:{};base64,{}",
                img.mime,
                base64::engine::general_purpose::STANDARD.encode(&img.bytes)
            ),
            None => match &account {
                Some(acct) if !acct.token.is_empty() && !acct.host.is_empty() => format!(
                    "{}/api/file/getImage?token={}&fileId={id}",
                    acct.host.trim_end_matches('/'),
                    acct.token
                ),
                _ => continue,
            },
        };
        out = replace_relative(
            &out,
            &format!("/api/file/getImage?fileId={id}"),
            &data_or_remote,
        );
    }
    out
}

/// Replace `needle` occurrences NOT immediately preceded by `://<host>`
/// (i.e. relative links only, not the tail of an absolute url).
fn replace_relative(content: &str, needle: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(pos) = rest.find(needle) {
        let head = &rest[..pos];
        let absolute = head.rfind("://").is_some_and(|p| {
            !head[p + 3..].is_empty()
                && !head[p + 3..].contains(['/', '"', '\'', '(', ')'])
                && !head[p + 3..].chars().any(char::is_whitespace)
        });
        if absolute {
            // part of an absolute url — copy through and keep looking
            out.push_str(&rest[..pos + needle.len()]);
        } else {
            out.push_str(head);
            out.push_str(replacement);
        }
        rest = &rest[pos + needle.len()..];
    }
    out.push_str(rest);
    out
}

/// 行内 code 高亮底色取主题 accent（桌面原版读自家 theme::accent，共享后
/// 由调用方传入当前 accent）。
pub fn markdown_view_style(accent: gpui_kit::Hsla) -> TextViewStyle {
    TextViewStyle::default().inline_code(HighlightStyle {
        background_color: Some(accent.opacity(0.1)),
        ..Default::default()
    })
}

/// 阅读/预览面通用构造：markdown 源 → 可滚动 TextView（GFM 表格、链接外开）。
pub fn markdown_view(
    id: gpui_kit::SharedString,
    src: String,
    dark: bool,
    accent: gpui_kit::Hsla,
) -> TextView {
    crate::math::install(
        crate::mermaid::install_full(TextView::markdown(id, src), dark, None),
        dark,
    )
    .style(markdown_view_style(accent))
    .scrollable(true)
    .on_link_click(|url, _ev, _window, cx| cx.open_url(url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_note_converts_and_plain_falls_back_to_title() {
        let md = reader_markdown_src("T", "# Hi\n\nbody", true);
        assert_eq!(md, "# Hi\n\nbody");
        let converted = reader_markdown_src("标题", "<h2>归档</h2><p>文本</p>", false);
        assert!(converted.contains("归档"), "{converted}");
        let plain = reader_markdown_src("空标题", "<p></p>", false);
        assert!(plain.starts_with("# 空标题"), "{plain}");
    }

    #[test]
    fn resolve_swaps_known_ids_only() {
        let dir = std::env::temp_dir().join(format!("lt-reader-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(dir.clone(), "u1".into()).unwrap();
        store
            .insert_image(&leanote_core::ImageAttachment {
                local_id: leanote_core::Id("imgaaaaaaaaaaaaaaaaaaaa1".into()),
                server_id: String::new(),
                file_name: "a.png".into(),
                mime: "image/png".into(),
                bytes: vec![1, 2, 3],
                size: 3,
                created_time: 0,
            })
            .unwrap();
        let out = resolve_note_images(
            "![a](leanote://file/getImage?fileId=imgaaaaaaaaaaaaaaaaaaaa1) ![b](leanote://file/getImage?fileId=zzz)",
            &store,
        );
        assert!(out.contains("data:image/png;base64,"), "{out}");
        assert!(out.contains("fileId=zzz"), "{out}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn resolve_relative_links_local_bytes_then_token() {
        let dir = std::env::temp_dir().join(format!("lt-reader-rel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(dir.clone(), "u1".into()).unwrap();
        // bytes cached locally (id matches by LocalId or ServerId)
        store
            .insert_image(&leanote_core::ImageAttachment {
                local_id: leanote_core::Id("srvaaaaaaaaaaaaaaaaaaaa1".into()),
                server_id: "srvaaaaaaaaaaaaaaaaaaaa1".into(),
                file_name: "a.png".into(),
                mime: "image/png".into(),
                bytes: vec![1, 2, 3],
                size: 3,
                created_time: 0,
            })
            .unwrap();
        // no account yet → no token absolutize for the missing one
        let out = resolve_note_images(
            "![a](/api/file/getImage?fileId=srvaaaaaaaaaaaaaaaaaaaa1) \
             ![b](/api/file/getImage?fileId=srvbbbbbbbbbbbbbbbbbbbb1)",
            &store,
        );
        assert!(out.contains("data:image/png;base64,"), "{out}");
        assert!(
            out.contains("/api/file/getImage?fileId=srvbbbbbbbbbbbbbbbbbbbb1"),
            "{out}"
        );
        // foreign absolute urls keep their tail intact (no corruption)
        let out = resolve_note_images(
            "![f](https://other.example/api/file/getImage?fileId=srvaaaaaaaaaaaaaaaaaaaa1)",
            &store,
        );
        assert!(
            out.contains("https://other.example/api/file/getImage?fileId="),
            "{out}"
        );
        assert!(!out.contains("https://other.example/data:"), "{out}");
        std::fs::remove_dir_all(dir).ok();
    }
}

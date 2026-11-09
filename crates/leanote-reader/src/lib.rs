//! 共享阅读渲染管线：笔记内容 → markdown 源（HTML 笔记转换、
//! `leanote://` 图片解析）→ kit `TextView` + Mermaid 内联 SVG。
//! 由桌面 leanote-app 与移动端 leanote-mobile 共用（ADR-0011、issue 05）。

pub mod mermaid;
mod reader;

pub use reader::{markdown_view, markdown_view_style, reader_markdown_src, resolve_note_images};

//! UI round 10 #6: LXGW WenKai Mono (霞鹜文楷等宽) loader.
//!
//! Downloads the official ttf ONCE into the data dir, then registers it
//! in-process via `TextSystem::add_fonts` (DirectWrite in-memory font
//! reference / CoreText CGDataProvider / cosmic-text). The previous
//! "~/Library/Fonts" install was macOS-only in effect: Windows scans no
//! HOME-relative font dir, and per-user fonts there additionally need an
//! HKCU `...\CurrentVersion\Fonts` registry entry nobody wrote — so the
//! family never resolved. In-process registration is identical on every
//! platform and takes effect in the SAME process: no restart, no OS dir,
//! no registry.
//!
//! Download runs on a background thread at boot: non-fatal by design. On
//! failure the UI keeps the system font.

use std::io::Read;
use std::path::PathBuf;

/// Regular weight, v1.530. Direct asset URLs, tried in order: GitHub first,
/// then mainland-reachable acceleration proxies (GitHub/objects.githubusercontent
/// are commonly unreachable from mainland China). Same asset, magic-checked
/// below — a proxy answering an HTML error page cannot sneak through.
const WENKAI_URLS: [&str; 3] = [
    "https://github.com/lxgw/LxgwWenKai/releases/download/v1.530/LXGWWenKaiMono-Regular.ttf",
    "https://ghfast.top/https://github.com/lxgw/LxgwWenKai/releases/download/v1.530/LXGWWenKaiMono-Regular.ttf",
    "https://gh-proxy.com/https://github.com/lxgw/LxgwWenKai/releases/download/v1.530/LXGWWenKaiMono-Regular.ttf",
];

const FONT_FILE: &str = "LXGWWenKaiMono-Regular.ttf";

/// Cached ttf in the app data dir (download-once, shared with pdf.rs).
pub fn font_path(dir: &std::path::Path) -> PathBuf {
    dir.join(FONT_FILE)
}

/// Boot-install state for the settings row: 0 = download pending,
/// 1 = registered (live — nothing left to do), 2 = failed.
pub static INSTALL_STATE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Register the cached ttf with the text system. True when the family is
/// usable now. `add_fonts` invalidates cached font resolution and line
/// layouts; repaint afterwards with `App::refresh_windows` when windows
/// are already on screen.
pub fn register(cx: &gpui_kit::App, dir: &std::path::Path) -> bool {
    let Ok(bytes) = std::fs::read(font_path(dir)) else {
        return false;
    };
    let ok = cx.text_system().add_fonts(vec![bytes.into()]).is_ok();
    if ok {
        INSTALL_STATE.store(1, std::sync::atomic::Ordering::Relaxed);
    }
    ok
}

/// Blocking download (GitHub first, then CN mirrors). Call off the UI thread.
pub fn install_wenkai(dir: &std::path::Path) -> std::io::Result<()> {
    let dest = font_path(dir);
    if dest.is_file() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(15)))
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut last_err: Option<std::io::Error> = None;
    for url in WENKAI_URLS {
        match fetch(&agent, url, &dest) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = Some(e),
        }
    }
    INSTALL_STATE.store(2, std::sync::atomic::Ordering::Relaxed);
    Err(last_err.unwrap_or_else(|| std::io::Error::other("wenkai download failed")))
}

fn fetch(agent: &ureq::Agent, url: &str, dest: &std::path::Path) -> std::io::Result<()> {
    let resp = agent.get(url).call().map_err(std::io::Error::other)?;
    // GitHub release assets redirect (302); the agent follows by default.
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(std::io::Error::other(format!("http {status}")));
    }
    // Stream to a sibling temp file, then rename — a half-written ttf in
    // the cache dir would "register" garbage on the next boot.
    let tmp = dest.with_extension("ttf.part");
    let mut reader = resp.into_body().into_reader();
    let mut file = std::fs::File::create(&tmp)?;
    std::io::copy(&mut reader, &mut file)?;
    file.sync_all().ok(); // best-effort flush before the rename
                          // Sanity: a ttf starts with 0x00 0x01 0x00 0x00 (or 'OTTO'/'true').
    let mut magic = [0u8; 4];
    let mut probe = std::fs::File::open(&tmp)?;
    probe.read_exact(&mut magic)?;
    let ok = &magic == b"\x00\x01\x00\x00" || &magic == b"OTTO" || &magic == b"true";
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        return Err(std::io::Error::other("not a ttf/otf"));
    }
    std::fs::rename(&tmp, dest)
}

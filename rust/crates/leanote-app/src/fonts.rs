//! UI round 10 #6: LXGW WenKai Mono (霞鹜文楷等宽) installer.
//!
//! Downloads the official ttf from the LXGW GitHub release into
//! ~/Library/Fonts so CoreText publishes the family on the next launch
//! (CoreText scans font dirs at process start, not live — a restart is
//! required, hence the settings hint text).
//!
//! Runs on a plain background thread at boot: non-fatal by design. On
//! failure the UI keeps the system font; the settings row can retry.

use std::io::Read;
use std::path::PathBuf;

/// Regular weight, v1.530. Direct asset URL — no zip involved.
const WENKAI_URL: &str =
    "https://github.com/lxgw/LxgwWenKai/releases/download/v1.530/LXGWWenKaiMono-Regular.ttf";

const FONT_FILE: &str = "LXGWWenKaiMono-Regular.ttf";

/// Target path: the per-user font dir (created on demand).
pub fn install_path() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(&home)
        .join("Library")
        .join("Fonts")
        .join(FONT_FILE)
}

/// Boot-install state for the settings row: 0 = download pending,
/// 1 = installed (restart needed), 2 = failed. The row shows a hint only
/// while a transition is still relevant — silent once settled.
pub static INSTALL_STATE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Outcome surfaced to the settings row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InstallOutcome {
    /// File already present (previous run or manual install).
    AlreadyInstalled,
    /// Fetched and written this call.
    Installed,
}

/// Blocking install. One retry on failure (flaky hotel Wi-Fi), then Err.
/// Call off the UI thread.
pub fn install_wenkai() -> std::io::Result<InstallOutcome> {
    let dest = install_path();
    if dest.is_file() {
        INSTALL_STATE.store(1, std::sync::atomic::Ordering::Relaxed);
        return Ok(InstallOutcome::AlreadyInstalled);
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
    for _ in 0..2 {
        match fetch(&agent, &dest) {
            Ok(()) => {
                INSTALL_STATE.store(1, std::sync::atomic::Ordering::Relaxed);
                return Ok(InstallOutcome::Installed);
            }
            Err(e) => last_err = Some(e),
        }
    }
    INSTALL_STATE.store(2, std::sync::atomic::Ordering::Relaxed);
    Err(last_err.unwrap_or_else(|| std::io::Error::other("wenkai download failed")))
}

fn fetch(agent: &ureq::Agent, dest: &std::path::Path) -> std::io::Result<()> {
    let resp = agent
        .get(WENKAI_URL)
        .call()
        .map_err(std::io::Error::other)?;
    // GitHub release assets redirect (302); the agent follows by default.
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(std::io::Error::other(format!("http {status}")));
    }
    // Stream to a sibling temp file, then rename — a half-written ttf in
    // the font dir would "install" garbage that CoreText then serves.
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

//! Local app log (ticket 16): crashes, sync errors, HTTP server errors →
//! `<app_data_dir>/logs/app.log`, size-capped rotation (app.log → app.log.1
//! → app.log.2, drop app.log.2). No remote reporting.
//!
//! Separate from `LEANOTE_MCP_DEBUG=1` (`~/leanote_mcp.log`, ticket 05/06) —
//! that path stays byte-identical and is not touched here.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::Mutex;

/// Rotate when the live file exceeds this. 2MB keeps ~3 files under 8MB.
const MAX_BYTES: u64 = 2 * 1024 * 1024;
/// app.log → app.log.1 → app.log.2 → dropped.
const ROTATIONS: u32 = 2;

static LOG_DIR: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    let dir = data_dir_override()
        .unwrap_or_else(crate::app_data_dir)
        .join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
});
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// `LEANOTE_DATA` wins (same convention as the store), else the
/// platform app data dir.
fn data_dir_override() -> Option<PathBuf> {
    std::env::var_os("LEANOTE_DATA").map(PathBuf::from)
}

/// Memoized `<dir>/logs`. `None` → logging disabled (dir un-probeable).
fn log_dir() -> Option<&'static PathBuf> {
    LOG_DIR.as_ref()
}

/// `logs/app.log` under the app data dir (or `LEANOTE_DATA`).
pub fn log_path() -> Option<PathBuf> {
    log_dir().map(|d| d.join("app.log"))
}

/// Local-time timestamp prefix. Same libc-localtime trick as
/// leanote-cli's format_time (no chrono dep).
fn stamp() -> String {
    unsafe {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        extern "C" {
            fn localtime(time: *const i64) -> *const Tm;
        }
        #[repr(C)]
        struct Tm {
            tm_sec: i32,
            tm_min: i32,
            tm_hour: i32,
            tm_mday: i32,
            tm_mon: i32,
            tm_year: i32,
            tm_wday: i32,
            tm_yday: i32,
            tm_isdst: i32,
            tm_gmtoff: i64,
            tm_zone: *const i8,
        }
        let p = localtime(&t);
        if p.is_null() {
            return format!("unix+{t}");
        }
        let tm = &*p;
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

/// Rotate if over cap, then append one `[stamp] [tag] msg` line.
fn append(tag: &str, msg: &str) {
    let Some(dir) = log_dir() else { return };
    let path = dir.join("app.log");
    let _guard = WRITE_LOCK.lock();
    if std::fs::metadata(&path)
        .map(|m| m.len() > MAX_BYTES)
        .unwrap_or(false)
    {
        // app.log.(N-1) → app.log.N, newest last; drop the oldest first.
        for n in (1..ROTATIONS).rev() {
            let _ = std::fs::rename(
                dir.join(format!("app.log.{n}")),
                dir.join(format!("app.log.{}", n + 1)),
            );
        }
        let _ = std::fs::rename(&path, dir.join("app.log.1"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{}] [{}] {}", stamp(), tag, msg);
    }
}

/// One-line entry: `log("sync", "incr failed: …")`.
pub fn log(tag: &str, msg: impl AsRef<str>) {
    append(tag, msg.as_ref());
}

/// Crash capture: forward panics to the log, then run the previous hook
/// (default abort-on-second-panic stays intact). Call once at process start.
pub fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        append("panic", &info.to_string());
        prev(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_caps_files() {
        // LOG_DIR binds to LEANOTE_DATA at first use and test order
        // within a binary is undefined → run the real check in a child
        // process so this binary's global state stays untouched.
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("logging::tests::rotation_child")
            .env("LEANOTE_ROTATION_CHILD", "1")
            .env(
                "LEANOTE_DATA",
                std::env::temp_dir()
                    .join("nonexistent-leanote-log")
                    .join("leanote.db")
                    .parent()
                    .unwrap(),
            )
            .output()
            .expect("spawn child");
        assert!(
            out.status.success(),
            "rotation child failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Runs only when spawned by rotation_caps_files above.
    #[test]
    fn rotation_child() {
        if std::env::var_os("LEANOTE_ROTATION_CHILD").is_none() {
            return;
        }
        let dir = data_dir_override().expect("LEANOTE_DATA set by parent");
        let path = dir.join("logs").join("app.log");
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        // Seed a live file over cap → the next write rotates.
        std::fs::write(&path, vec![b'x'; (MAX_BYTES + 1) as usize]).unwrap();
        log("test", "rotate me");
        assert!(path.metadata().unwrap().len() < MAX_BYTES);
        assert!(dir.join("logs").join("app.log.1").exists());
        let first = std::fs::read_to_string(&path).unwrap();
        assert!(first.contains("[test] rotate me"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn log_path_is_under_data_dir() {
        // Same test binary as rotation_caps_files — order not guaranteed,
        // but both only need *a* stable logs/app.log path.
        let p = log_path().expect("log path");
        assert!(p.ends_with("logs/app.log"));
    }
}

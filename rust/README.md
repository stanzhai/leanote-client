# Leanote (Rust)

Rust rewrite of the leanote desktop app. Workspace layout:

- `crates/leanote-core` — SQLite store, sync engine, conflicts, html2md
- `crates/leanote-cli` — `leanote` binary (add/list/show/search/edit/sync/serve)
- `crates/leanote-http` — local `/api/*` + MCP server on 127.0.0.1:8912
- `crates/leanote-app` — GPUI desktop shell (login, three-pane, editor, tray)
- `fake-server` — programmable leanote server for dev/tests

## Build

```sh
cargo build --release          # all binaries → target/release/{leanote,Leanote}
cargo test                     # workspace tests
```

Data dir: `~/Library/Application Support/leanote/` (macOS), `%APPDATA%\leanote`
(Windows), `~/.config/leanote` (Linux, honors `XDG_CONFIG_HOME`). Override with
`LEANOTE_DATA`. The legacy Electron NeDB files (`nedb55/`) may live in the
same macOS dir; they are never read or written (ADR-0008).
Logs: `<data dir>/logs/app.log` (crashes, sync errors, HTTP server errors;
size-capped rotation). `LEANOTE_MCP_DEBUG=1` additionally appends to
`~/leanote_mcp.log`, unchanged from the JS app.

## Packaging

Per-platform scripts (CI runs the same ones; see `.github/workflows/release.yml`):

```sh
scripts/package-macos.sh <aarch64|x86_64> [output-dir] [version]   # .app + .dmg
scripts/package-linux.sh <x86_64|aarch64> [output-dir] [version]   # AppImage + .deb
scripts/package-windows.ps1 -Arch <x64|arm64> [-OutDir …] [-Version …]  # NSIS + zip
```

Artifacts are named `Leanote_<version>_<os>_<arch>.<ext>`. The GUI binary is
`Leanote` everywhere; the CLI ships alongside as `leanote-cli` (inside the
macOS .app — APFS/NTFS are case-insensitive so it can't be `leanote`),
`bin\leanote.exe` (Windows, optional PATH section in NSIS), and `/usr/bin/leanote`
(Linux). Icons are generated from `build/icon.png` by `scripts/gen-icons.sh`
and committed under `packaging/` so CI needs no graphics toolchain.

## Tray

macOS-first, via `tray-icon` (gpui-kit ships no tray API): icon + menu
显示/退出, left-click shows the main window, 退出 is a true quit. Closing
the last window quits on Win/Linux (gpui `QuitMode::Default`); on macOS the
app stays resident with its dock icon (platform default) — use the tray
退出 to quit.

## Keyboard & UI (issue 17)

- **⌘K / ⌘⇧P (Ctrl+Shift+P)** — three-section quick panel: quick actions
  (new note / sync now / export backup / toggle Vim), recents (top 6, empty
  query), and debounced search. Bare keys always go to the focused element;
  only modifier combos open the panel. IME composition never triggers it.
- **Default editing state** — selecting a Markdown note opens it in source
  edit mode (preview is a toggle, not the default). The title row is pinned
  at the top of the pane (editable in source, read-only in preview). Edits
  mark dirty after a debounce; unchanged notes never write or sync. ⌘S saves
  immediately.
- **Vim mode** — normal/insert/visual layers (motions `h j k l w b 0 $ gg G`,
  operators `d c y`, `x dd yy p u`, `i a o O`, `v V`, `:w :q`, esc), a
  toolbar toggle, and a `-- NORMAL --` status line. The preference persists
  locally. ⌘K also exposes a quick toggle.
- **Bottom-left cluster** — sync (cloud icon with state), settings (gear →
  centered dialog: appearance follow/light/dark, editor palette, language,
  Vim default, account info + logout), username (menu with logout →
  confirmation dialog). Settings persist as local JSON.

## Export (issue 18/21)

Note context menu / title-bar ⋯ menu → 导出为… submenu: Markdown, HTML,
PDF, Word. All run locally (no server round-trip): PDF via embedded typst,
`.docx` via knit-md-docx (pulldown-cmark + docx-rs — headings, emphasis,
links, images, lists, task lists, code blocks, quotes, GFM tables,
footnotes; rich-text notes go through the same HTML→Markdown bridge as
PDF). The save panel starts in the remembered export directory
(`~/Downloads` first time) and remembers the chosen dir in `prefs.json`.
Library-wide backup: `.leanote` JSON, same shape as the JS export plugin.

## Window drag & caption buttons

Drag regions: the nav header's empty strip, reader strip (28px), and editor
strip (10px). Windows maps them to OS-native HTCAPTION
(`window_control_area(Drag)`); Linux and macOS move via `start_window_move`
listeners instead. A Drag region must never contain interactive children:
on Windows every pixel of it answers NCHITTEST with HTCAPTION, which is how
the editor title-row toolbar buttons died before this rule.

The editor title row (including the note-title input) drags the window via
press-and-move listeners with a 4px threshold: a click still places the
caret and works the buttons, a drag moves the window. Windows has no
`start_window_move`, so the handoff is `ReleaseCapture` +
`WM_NCLBUTTONDOWN(HTCAPTION)` posted asynchronously (`PostMessageW` — a
synchronous `SendMessageW` nests DefWindowProc's modal move loop inside
gpui's in-flight mouse dispatch and blows the main thread's stack, exit
code 0xC0000409; `main.rs win_dpi_resync::start_window_drag`);
macOS/Linux keep `start_window_move`. Drag-selection inside the title input
yields to the window drag (explicit product ruling).

Min/max/close (Windows/Linux) are self-drawn in a root-level overlay flush
to the window's top-right corner — present in every phase, including
login/syncing where the nav is hidden — and each button `occlude()`s itself
so the drag strips beneath cannot shadow it in the NCHITTEST stack
(ADR-0011, amended 2026-10-31). macOS keeps native traffic lights.

Windows/Linux nav header: the app icon + the "Leanote" name sit at the drag
region's left end; macOS keeps the traffic-light inset with no name (issue
20 Q7). The icon is a full-bleed LANCZOS downscale of the 1690px logo
master — opaque black square, white ant glyph, grayscale anti-aliased edges
(no thresholding; the binary-alpha rounds produced visible jaggies at
taskbar sizes). `gen-icons.sh` emits both the exe's `packaging/icon.ico`
and `assets/app-icon.png` from the same frames, byte-identical at 128px,
so the taskbar icon and the in-app header icon always match.

## Windows main-thread stack

`main()` runs the whole app on a worker thread with a 64 MB stack on Windows
(`main.rs`). The MSVC default 1 MB main stack sits within ~10% of the debug
build's editor render depth (title row + tag chips + slash panel are gpui
element subtrees built recursively); restoring a note at boot overflowed it
(exit 0xC0000409). Stack pages commit on demand, so the reserve costs nothing
until used.

## Windows high-DPI viewport resync

On some high-DPI Windows desktops (observed: 175%, DPI context flips
96 ↔ monitor DPI during window creation), `gpui-pre-windows` records the
client device size as the LOGICAL size — the layout viewport ends up
`scale`-times larger than the visible window: the right portion of the UI
painted outside the swapchain and was unreachable by the mouse (the
original "toolbar buttons don't respond" report). `main.rs`
`win_dpi_resync` measures the live client with Win32 after startup and,
on mismatch, forces a WM_SIZE (±2px SetWindowPos, physical size
unchanged) so the backend recomputes logical = device / scale; the
close-time geometry save then switches to device pixels to keep the
window at the same on-screen size across relaunches (ADR-0011).

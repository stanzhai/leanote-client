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

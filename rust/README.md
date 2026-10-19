# leanote-rust

Rust rewrite of the leanote desktop app. Workspace layout:

- `crates/leanote-core` — SQLite store, sync engine, conflicts, html2md
- `crates/leanote-cli` — `leanote` binary (add/list/show/search/edit/sync/serve)
- `crates/leanote-http` — local `/api/*` + MCP server on 127.0.0.1:8912
- `crates/leanote-app` — GPUI desktop shell (login, three-pane, editor, tray)
- `fake-server` — programmable leanote server for dev/tests

## Build

```sh
cargo build --release          # all binaries → target/release/{leanote,leanote-app}
cargo test                     # workspace tests
```

Data dir: `~/Library/Application Support/leanote-rust/` (macOS) or
`~/.config/leanote-rust/` (Linux). Override with `LEANOTE_RUST_DATA`.
Logs: `<data dir>/logs/app.log` (crashes, sync errors, HTTP server errors;
size-capped rotation). `LEANOTE_MCP_DEBUG=1` additionally appends to
`~/leanote_mcp.log`, unchanged from the JS app.

## Packaging

### macOS

```sh
scripts/package-macos.sh [output-dir]   # default: ../dist
```

Produces `Leanote.app` (GUI + CLI binaries, `icon.icns` reused from the repo
root, ad-hoc codesigned) and `Leanote.dmg` via `hdiutil`, then repoints
`~/bin/leanote` at the packaged CLI (`…/Leanote.app/Contents/MacOS/leanote`).
The CLI also rewrites the shim itself on first run (`shim-switch`, ticket 07).

Manual .dmg alternative:

```sh
hdiutil create -volname Leanote -srcfolder dist/Leanote.app -ov -format UDZO dist/Leanote.dmg
```

### Linux / Windows

Not packaged here — the acceptance bar is `cargo check`/`cargo build` per
platform plus runtime checks on real hardware (see ticket 16 notes):
`cargo check --target x86_64-unknown-linux-gnu` /
`cargo check --target x86_64-pc-windows-msvc` with the matching target
installed via `rustup target add …`; without a full cross toolchain (GTK,
X11, MSVC SDK) a *build* of the GUI cannot be produced on a Mac host.

## Tray

macOS-first, via `tray-icon` (gpui-kit ships no tray API): icon + menu
显示/退出, left-click shows the main window, 退出 is a true quit. Closing
the last window quits on Win/Linux (gpui `QuitMode::Default`); on macOS the
app stays resident with its dock icon (platform default) — use the tray
退出 to quit.

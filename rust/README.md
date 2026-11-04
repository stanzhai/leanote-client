# Leanote (Rust)

Rust rewrite of the leanote desktop app. Workspace layout:

- `crates/leanote-core` — SQLite store, sync engine, conflicts, html2md
- `crates/leanote-cli` — `leanote` binary (add/list/show/search/edit/sync/serve)
- `crates/leanote-http` — local `/api/*` + MCP server on 127.0.0.1:8912
- `crates/leanote-app` — GPUI desktop shell (login, three-pane, editor, tray)
- `crates/leanote-reader` — shared reading pipeline (markdown/HTML→md, image
  resolve, Mermaid inline SVG) used by desktop and mobile (ADR-0011)
- `crates/leanote-mobile` — iOS/Android read-mostly shell on vendored
  `gpui-pre-mobile` (ADR-0013; see `.scratch/mobile/`)
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

## Mobile (iOS / Android)

Read-mostly companion app on the same gpui stack (`gpui-pre-mobile` vendored
under `vendor/`, retargeted to our vendored gpui-pre 0.3.9 — ADR-0013).
Host projects live in `mobile/{ios,android}`; the shared UI/data crates are
`leanote-mobile` + `leanote-core` + `leanote-reader`.

### iOS (simulator)

Prereqs: Xcode (agree the license: `sudo xcodebuild -license`), XcodeGen
(`brew install xcodegen`). If `xcode-select` still points at
CommandLineTools (no iOS SDK), the Xcode project's prebuild script feeds
SDKROOT/CC explicitly — otherwise `sudo xcode-select -s /Applications/Xcode.app`
is the clean fix.

```sh
cd mobile/ios
xcodegen generate                                   # once, and after project.yml edits
xcodebuild -project Leanote.xcodeproj -scheme Leanote \
  -sdk iphonesimulator -destination 'platform=iOS Simulator,name=iPhone 17 Pro' build
# or just open Leanote.xcodeproj in Xcode and hit Run
xcrun simctl boot "iPhone 17 Pro"                    # if not booted
APP=$(ls -d ~/Library/Developer/Xcode/DerivedData/Leanote-*/Build/Products/Debug-iphonesimulator/Leanote.app)
xcrun simctl install "iPhone 17 Pro" "$APP"
xcrun simctl launch "iPhone 17 Pro" com.leanote.mobile
xcrun simctl io "iPhone 17 Pro" screenshot /tmp/leanote.png   # smoke evidence
```

Device/TestFlight: the project pre-wires `DEVELOPMENT_TEAM` — sign in to
Xcode → Accounts, archive for `-sdk iphoneos`, upload via Organizer
(steps in `.scratch/mobile/issues/09-ios-v1-acceptance.md`).

### Android

Prereqs: Android SDK + NDK (`ANDROID_NDK_HOME`), `cargo install cargo-ndk`.
The API level matters: **`-P 26`** (= minSdk) — the cargo-ndk default 21
fails linking `libnativewindow`.

```sh
mobile/android/build.sh                # Rust .so + debug APK, one shot
# manual:
cargo ndk -t arm64-v8a -P 26 -o mobile/android/gradle/app/src/main/jniLibs \
  build --lib -p leanote-mobile
cd mobile/android/gradle && ./gradlew assembleDebug
adb install app/build/outputs/apk/debug/app-debug.apk  # sideload (self-use v1)
```

### Cross-checks (what CI runs — `.github/workflows/mobile-check.yml`)

```sh
# iOS target (needs Xcode; if xcode-select is CLT, set the env as below)
SDK=/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk
TC=/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin
SDKROOT=$SDK CC_aarch64_apple_ios=$TC/clang PATH="$TC:$PATH" \
  cargo check -p leanote-mobile --target aarch64-apple-ios

# Android target (needs NDK)
N=$HOME/Library/Android/sdk/ndk/28.2.13676358/toolchains/llvm/prebuilt/darwin-x86_64/bin
CC_aarch64_linux_android=$N/aarch64-linux-android24-clang \
CXX_aarch64_linux_android=$N/aarch64-linux-android24-clang++ \
AR_aarch64_linux_android=$N/llvm-ar \
  cargo check -p leanote-mobile --target aarch64-linux-android
```

### Tests & dev hooks

```sh
cargo test                                        # whole workspace (desktop + reader + core)
cargo test -p leanote-reader -p leanote-mobile    # shared pipeline / mobile units
```

Simulator smoke without typing (simctl has no tap/keyboard): launch with
env prefixed `SIMCTL_CHILD_` —

```sh
SIMCTL_CHILD_LEANOTE_FAKE_SERVER=1 xcrun simctl launch …  # in-process fake server + auto login (full sync)
SIMCTL_CHILD_LEANOTE_INITIAL_TAB=search|settings …        # open on a given tab
SIMCTL_CHILD_LEANOTE_INITIAL_SCREEN=notebooks|tags|reader:<noteId> …  # notes-tab deep link
SIMCTL_CHILD_LEANOTE_SEARCH=关键词 …                       # prefill + run a search
xcrun simctl ui "iPhone 17 Pro" appearance dark           # dark-mode pass
```

Already-logged-in + `LEANOTE_FAKE_SERVER=1` boots only the in-process fake
server (startup incremental sync) instead of re-login. Device data dir on
iOS is `<container>/Library/Application Support/leanote/` (find the
container with `xcrun simctl get_app_container <sim> com.leanote.mobile data`).

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

#!/bin/sh
# Manual acceptance checklist runner (ticket 16) — headless-exercisable
# parts only: CLI, sync, conflicts, local HTTP API, MCP, logging, packaging.
# GUI-only items (tray visible, editor focus, window restore) need human
# eyes and are recorded separately in the ticket comment.
#
# Usage: rust/scripts/acceptance-macos.sh          (uses target/release)
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
BIN="$ROOT/rust/target/release/leanote"
APPBIN="$ROOT/rust/target/release/leanote-app"
WORK=$(mktemp -d /tmp/leanote-accept.XXXXXX)
PASS=0; FAIL=0

check() { # check <name> <cmd...>
  name=$1; shift
  if "$@" >/dev/null 2>&1; then
    echo "PASS  $name"; PASS=$((PASS+1))
  else
    echo "FAIL  $name"; FAIL=$((FAIL+1))
  fi
}

export LEANOTE_RUST_DATA="$WORK/data"
export LEANOTE_NO_SHIM_SWITCH=1
export LEANOTE_PORT=8933
mkdir -p "$WORK/data"

echo "== CLI surface (file I/O mode) =="
echo "acceptance note body" | "$BIN" add -t "验收笔记" >/dev/null
check "cli add"      sh -c "\"$BIN\" list | grep -q '验收笔记'"
NID=$("$BIN" list | grep '验收笔记' | awk '{print $1}')
check "cli show"     sh -c "\"$BIN\" show \"$NID\" >/dev/null"
check "cli search"   sh -c "\"$BIN\" search 验收笔记 | grep -q '验收笔记'"
echo "edited body" | "$BIN" edit "$NID" >/dev/null
check "cli edit"     sh -c "\"$BIN\" show \"$NID\" | grep -q 'edited body'"

echo "== local HTTP API + MCP =="
"$BIN" serve -p 8933 >/dev/null 2>&1 &
SERVE_PID=$!
sleep 1
check "http ping"    curl -sf "http://127.0.0.1:8933/api/ping"
check "http add"     curl -sf -X POST "http://127.0.0.1:8933/api/note/addNote" \
                     -H 'Content-Type: application/json' \
                     -d '{"title":"HTTP 验收","content":"api body","Tags":["验收"]}'
check "mcp initialize" curl -sf "http://127.0.0.1:8933/mcp" \
                     -H 'Content-Type: application/json' \
                     -H 'Accept: application/json, text/event-stream' \
                     -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"acceptance","version":"0"}}}'
kill $SERVE_PID 2>/dev/null || true

echo "== local log file =="
"$BIN" sync --host http://127.0.0.1:1 --email a@b.c --password x >/dev/null 2>&1 || true
check "log path exists" test -f "$WORK/data/logs/app.log"
check "log records error" grep -q '\[sync\]' "$WORK/data/logs/app.log"

echo "== packaging artifacts =="
check "Leanote.app exists"  test -d "$ROOT/dist/Leanote.app"
check "CLI in .app"         test -x "$ROOT/dist/Leanote.app/Contents/MacOS/leanote"
check "GUI in .app"         test -x "$ROOT/dist/Leanote.app/Contents/MacOS/Leanote"
check "icon.icns in .app"   test -f "$ROOT/dist/Leanote.app/Contents/Resources/app.icns"
check "dmg exists"          test -f "$ROOT/dist/Leanote.dmg"
check "shim points at .app" sh -c "grep -q 'Leanote.app/Contents/MacOS/leanote' '$HOME/bin/leanote'"

echo
echo "passed: $PASS  failed: $FAIL  (workdir $WORK)"
test "$FAIL" -eq 0

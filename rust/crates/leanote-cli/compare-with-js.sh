#!/usr/bin/env bash
# Byte-compare the Rust CLI against the Electron JS CLI (bin/leanote.js).
#
# The JS CLI runs offline (no Electron needed — it reads NeDB files directly).
# Its HTTP probe to 127.0.0.1:8912 is neutralized with a node --require shim so
# both CLIs exercise their offline path on identical fixture data.
#
# Usage: ./compare-with-js.sh [path-to-repo-root] [rust-bin]
# Exits 0 only if every case matches stdout, stderr and exit code.

set -u
REPO="${1:-$(cd "$(dirname "$0")/../../.." && pwd)}"
BIN="${2:-$(dirname "$0")/../target/debug/leanote}"
JS="$REPO/bin/leanote.js"
SHIM="$(mktemp -t leanote_offline).js"
SB="$(mktemp -d -t leanote_cmp)"

[ -x "$BIN" ] || { echo "rust binary not found: $BIN (cargo build first)"; exit 2; }
[ -f "$JS" ] || { echo "JS CLI not found: $JS"; exit 2; }

cat > "$SHIM" <<'EOF'
// Force the JS CLI offline: any http call to 127.0.0.1:8912 fails immediately.
const http = require('http');
const origGet = http.get, origReq = http.request;
function mkFail() {
  const req = new http.ClientRequest({ host: '127.0.0.1', port: 1, path: '/' });
  req.on('socket', () => req.destroy(new Error('offline')));
  return req;
}
http.get = function (a) {
  const o = typeof a === 'object' ? a : null;
  if (o && String(o.port) === '8912') return mkFail();
  return origGet.apply(http, arguments);
};
http.request = function (a) {
  const o = typeof a === 'object' ? a : null;
  if (o && String(o.port) === '8912') return mkFail();
  return origReq.apply(http, arguments);
};
EOF

# --- identical fixtures on both sides ---
seed() { # $1 = side dir
  local NB="$1/Library/Application Support/leanote/nedb55/user1"
  mkdir -p "$NB" "$1/Library/Application Support/leanote-rust"
  printf '{"Ok":true,"UserId":"user1","_id":"u1","IsActive":true}\n' > "$1/Library/Application Support/leanote/nedb55/users.db"
  printf '{"_id":"n1","NotebookId":"notebookaaaa000000000001","Title":"AI札","Seq":-1,"UserId":"user1","ParentNotebookId":"","LocalIsNew":true,"IsDirty":true}\n' > "$NB/notebooks.db"
  cat > "$NB/notes.db" <<'NEOF'
{"_id":"x1","NoteId":"aaaa11111111111111111111","UserId":"user1","NotebookId":"notebookaaaa000000000001","Title":"First note","Content":"# Hello\n\nWorld","Desc":"Hello World","ImgSrc":"","Tags":["ai","work"],"Abstract":"# Hello","IsMarkdown":true,"IsTrash":false,"IsDirty":false,"LocalIsNew":false,"LocalIsDelete":false,"IsBlog":false,"CreatedTime":{"$$date":1774000000000},"UpdatedTime":{"$$date":1774000000000}}
{"_id":"x2","NoteId":"aaaa22222222222222222222","UserId":"user1","NotebookId":"notebookaaaa000000000002","Title":"Second","Content":"Body two","Desc":"Body two","ImgSrc":"","Tags":[],"Abstract":"Body two","IsMarkdown":true,"IsTrash":false,"IsDirty":false,"LocalIsNew":false,"LocalIsDelete":false,"IsBlog":false,"CreatedTime":{"$$date":1773999900000},"UpdatedTime":{"$$date":1773999900000}}
{"_id":"x3","NoteId":"cccc11111111111111111111","UserId":"user1","NotebookId":"notebookaaaa000000000001","Title":"Amb 11","Content":"C11","Desc":"C11","ImgSrc":"","Tags":[],"Abstract":"C11","IsMarkdown":true,"IsTrash":false,"IsDirty":false,"LocalIsNew":false,"LocalIsDelete":false,"IsBlog":false,"CreatedTime":{"$$date":1774000000000},"UpdatedTime":{"$$date":1774000000000}}
{"_id":"x4","NoteId":"cccc22111111111111111111","UserId":"user1","NotebookId":"notebookaaaa000000000001","Title":"Amb 22","Content":"C22","Desc":"C22","ImgSrc":"","Tags":[],"Abstract":"C22","IsMarkdown":true,"IsTrash":false,"IsDirty":false,"LocalIsNew":false,"LocalIsDelete":false,"IsBlog":false,"CreatedTime":{"$$date":1774000000000},"UpdatedTime":{"$$date":1774000000000}}
{"_id":"x5","NoteId":"bbbb33333333333333333333","UserId":"user1","NotebookId":"notebookaaaa000000000001","Title":"Trashed","Content":"gone","Desc":"gone","ImgSrc":"","Tags":[],"Abstract":"gone","IsMarkdown":true,"IsTrash":true,"IsDirty":false,"LocalIsNew":false,"LocalIsDelete":false,"IsBlog":false,"CreatedTime":{"$$date":1773999800000},"UpdatedTime":{"$$date":1773999800000}}
NEOF
  printf '{"_id":"n2","NotebookId":"notebookaaaa000000000002","Title":"Work","Seq":1,"UserId":"user1","ParentNotebookId":"","LocalIsNew":false,"IsDirty":false}\n' >> "$NB/notebooks.db"
  : > "$NB/tags.db"
}
JH="$SB/js"; RH="$SB/rs"
seed "$JH"; seed "$RH"
sqlite3 "$RH/Library/Application Support/leanote-rust/leanote.db" "
CREATE TABLE IF NOT EXISTS notebooks (NotebookId TEXT PRIMARY KEY, UserId TEXT NOT NULL, Title TEXT NOT NULL, ParentNotebookId TEXT NOT NULL DEFAULT '', Seq INTEGER NOT NULL DEFAULT 0, Usn INTEGER NOT NULL DEFAULT 0, LocalIsNew INTEGER NOT NULL DEFAULT 0, IsDirty INTEGER NOT NULL DEFAULT 0, LocalIsDelete INTEGER NOT NULL DEFAULT 0, ServerNotebookId TEXT DEFAULT NULL);
CREATE TABLE IF NOT EXISTS notes (NoteId TEXT PRIMARY KEY, UserId TEXT NOT NULL, NotebookId TEXT NOT NULL, Title TEXT NOT NULL DEFAULT '', Content TEXT NOT NULL DEFAULT '', \"Desc\" TEXT NOT NULL DEFAULT '', ImgSrc TEXT NOT NULL DEFAULT '', Tags TEXT NOT NULL DEFAULT '', Abstract TEXT NOT NULL DEFAULT '', IsMarkdown INTEGER NOT NULL DEFAULT 1, IsTrash INTEGER NOT NULL DEFAULT 0, IsDeleted INTEGER NOT NULL DEFAULT 0, IsDirty INTEGER NOT NULL DEFAULT 0, LocalIsNew INTEGER NOT NULL DEFAULT 0, LocalIsDelete INTEGER NOT NULL DEFAULT 0, IsBlog INTEGER NOT NULL DEFAULT 0, Star INTEGER NOT NULL DEFAULT 0, Usn INTEGER NOT NULL DEFAULT 0, CreatedTime INTEGER NOT NULL DEFAULT 0, UpdatedTime INTEGER NOT NULL DEFAULT 0, ServerNoteId TEXT DEFAULT NULL);
CREATE TABLE IF NOT EXISTS tags (TagId TEXT PRIMARY KEY, UserId TEXT NOT NULL, Tag TEXT NOT NULL, IsDirty INTEGER NOT NULL DEFAULT 0, Count INTEGER NOT NULL DEFAULT 0, LocalIsDelete INTEGER NOT NULL DEFAULT 0, CreatedTime INTEGER NOT NULL DEFAULT 0, UpdatedTime INTEGER NOT NULL DEFAULT 0, UNIQUE(UserId, Tag));
CREATE TABLE IF NOT EXISTS accounts (UserId TEXT PRIMARY KEY, Email TEXT NOT NULL DEFAULT '', Username TEXT NOT NULL DEFAULT '', Host TEXT NOT NULL DEFAULT '', Token TEXT NOT NULL DEFAULT '', IsActive INTEGER NOT NULL DEFAULT 0);
INSERT INTO notebooks VALUES ('notebookaaaa000000000001','user1','AI札','',-1,0,1,1,0,NULL);
INSERT INTO notebooks VALUES ('notebookaaaa000000000002','user1','Work','',1,0,0,0,0,NULL);
INSERT INTO notes VALUES ('aaaa11111111111111111111','user1','notebookaaaa000000000001','First note','# Hello

World','Hello World','','ai,work','# Hello',1,0,0,0,0,0,0,0,0,1774000000000,1774000000000,NULL);
INSERT INTO notes VALUES ('aaaa22222222222222222222','user1','notebookaaaa000000000002','Second','Body two','Body two','','','Body two',1,0,0,0,0,0,0,0,0,1773999900000,1773999900000,NULL);
INSERT INTO notes VALUES ('cccc11111111111111111111','user1','notebookaaaa000000000001','Amb 11','C11','C11','','','C11',1,0,0,0,0,0,0,0,0,1774000000000,1774000000000,NULL);
INSERT INTO notes VALUES ('cccc22111111111111111111','user1','notebookaaaa000000000001','Amb 22','C22','C22','','','C22',1,0,0,0,0,0,0,0,0,1774000000000,1774000000000,NULL);
INSERT INTO notes VALUES ('bbbb33333333333333333333','user1','notebookaaaa000000000001','Trashed','gone','gone','','','gone',1,1,0,0,0,0,0,0,0,1773999800000,1773999800000,NULL);" || exit 2

FAIL=0
cmp_case() { # args: the CLI argv; stdin (if any) via $STDIN_DATA
  local stdin_data="${STDIN_DATA:-}"
  local jo je ro re
  if [ -n "$stdin_data" ]; then
    HOME="$JH" node --require "$SHIM" "$JS" "$@" <<<"$stdin_data" >"$SB/j.out" 2>"$SB/j.err"; je=$?
    HOME="$RH" "$BIN" "$@" <<<"$stdin_data" >"$SB/r.out" 2>"$SB/r.err"; re=$?
  else
    HOME="$JH" node --require "$SHIM" "$JS" "$@" </dev/null >"$SB/j.out" 2>"$SB/j.err"; je=$?
    HOME="$RH" "$BIN" "$@" </dev/null >"$SB/r.out" 2>"$SB/r.err"; re=$?
  fi
  if cmp -s "$SB/j.out" "$SB/r.out" && cmp -s "$SB/j.err" "$SB/r.err" && [ "$je" -eq "$re" ]; then
    echo "MATCH: $*${stdin_data:+ (stdin)}"
  else
    echo "DIFF:  $*${stdin_data:+ (stdin)} (exit js=$je rs=$re)"
    diff "$SB/j.out" "$SB/r.out" | head -8
    diff "$SB/j.err" "$SB/r.err" | head -4
    FAIL=1
  fi
}


norm() { sed -E -e 's/[0-9a-f]{24}/NORMID/g' -e 's/[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}/NTIME/g'; }
cmp_norm_case() { # cases whose notes were created by add (random ids/times)
  HOME="$JH" node --require "$SHIM" "$JS" "$@" </dev/null | norm > "$SB/j.out" 2>"$SB/j.err"; local je=$?
  HOME="$RH" "$BIN" "$@" </dev/null | norm > "$SB/r.out" 2>"$SB/r.err"; local re=$?
  if cmp -s "$SB/j.out" "$SB/r.out" && cmp -s "$SB/j.err" "$SB/r.err" && [ "$je" -eq "$re" ]; then
    echo "MATCH: $* (ids normalized)"
  else
    echo "DIFF:  $* (ids normalized) (exit js=$je rs=$re)"; diff "$SB/j.out" "$SB/r.out" | head -8; FAIL=1
  fi
}
cmp_case list
cmp_case list -v
cmp_case list -n AI札
cmp_case list -n Work
cmp_case list -n Nope
cmp_case list -g ai
cmp_case list -g nomatch
NEDB_SUM_BEFORE=$(find "$RH/Library/Application Support/leanote" -type f -exec shasum {} \; | sort | shasum)
sqlite3 "$RH/Library/Application Support/leanote-rust/leanote.db" "DROP TABLE IF EXISTS notes; DROP TABLE IF EXISTS notebooks;" >/dev/null 2>&1
sqlite3 "$RH/Library/Application Support/leanote-rust/leanote.db" "
CREATE TABLE IF NOT EXISTS notebooks (NotebookId TEXT PRIMARY KEY, UserId TEXT NOT NULL, Title TEXT NOT NULL, ParentNotebookId TEXT NOT NULL DEFAULT '', Seq INTEGER NOT NULL DEFAULT 0, Usn INTEGER NOT NULL DEFAULT 0, LocalIsNew INTEGER NOT NULL DEFAULT 0, IsDirty INTEGER NOT NULL DEFAULT 0, LocalIsDelete INTEGER NOT NULL DEFAULT 0, ServerNotebookId TEXT DEFAULT NULL);
CREATE TABLE IF NOT EXISTS notes (NoteId TEXT PRIMARY KEY, UserId TEXT NOT NULL, NotebookId TEXT NOT NULL, Title TEXT NOT NULL DEFAULT '', Content TEXT NOT NULL DEFAULT '', \"Desc\" TEXT NOT NULL DEFAULT '', ImgSrc TEXT NOT NULL DEFAULT '', Tags TEXT NOT NULL DEFAULT '', Abstract TEXT NOT NULL DEFAULT '', IsMarkdown INTEGER NOT NULL DEFAULT 1, IsTrash INTEGER NOT NULL DEFAULT 0, IsDeleted INTEGER NOT NULL DEFAULT 0, IsDirty INTEGER NOT NULL DEFAULT 0, LocalIsNew INTEGER NOT NULL DEFAULT 0, LocalIsDelete INTEGER NOT NULL DEFAULT 0, IsBlog INTEGER NOT NULL DEFAULT 0, Star INTEGER NOT NULL DEFAULT 0, Usn INTEGER NOT NULL DEFAULT 0, CreatedTime INTEGER NOT NULL DEFAULT 0, UpdatedTime INTEGER NOT NULL DEFAULT 0, ServerNoteId TEXT DEFAULT NULL);
CREATE TABLE IF NOT EXISTS tags (TagId TEXT PRIMARY KEY, UserId TEXT NOT NULL, Tag TEXT NOT NULL, IsDirty INTEGER NOT NULL DEFAULT 0, Count INTEGER NOT NULL DEFAULT 0, LocalIsDelete INTEGER NOT NULL DEFAULT 0, CreatedTime INTEGER NOT NULL DEFAULT 0, UpdatedTime INTEGER NOT NULL DEFAULT 0, UNIQUE(UserId, Tag));
CREATE TABLE IF NOT EXISTS accounts (UserId TEXT PRIMARY KEY, Email TEXT NOT NULL DEFAULT '', Username TEXT NOT NULL DEFAULT '', Host TEXT NOT NULL DEFAULT '', Token TEXT NOT NULL DEFAULT '', IsActive INTEGER NOT NULL DEFAULT 0);
INSERT INTO notebooks VALUES ('notebookaaaa000000000001','user1','AI札','',-1,0,1,1,0,NULL);
INSERT INTO notebooks VALUES ('notebookaaaa000000000002','user1','Work','',1,0,0,0,0,NULL);
INSERT INTO notes VALUES ('aaaa11111111111111111111','user1','notebookaaaa000000000001','First note','# Hello

World','Hello World','','ai,work','# Hello',1,0,0,0,0,0,0,0,0,1774000000000,1774000000000,NULL);
INSERT INTO notes VALUES ('aaaa22222222222222222222','user1','notebookaaaa000000000002','Second','Body two','Body two','','','Body two',1,0,0,0,0,0,0,0,0,1773999900000,1773999900000,NULL);
INSERT INTO notes VALUES ('cccc11111111111111111111','user1','notebookaaaa000000000001','Amb 11','C11','C11','','','C11',1,0,0,0,0,0,0,0,0,1774000000000,1774000000000,NULL);
INSERT INTO notes VALUES ('cccc22111111111111111111','user1','notebookaaaa000000000001','Amb 22','C22','C22','','','C22',1,0,0,0,0,0,0,0,0,1774000000000,1774000000000,NULL);
INSERT INTO notes VALUES ('bbbb33333333333333333333','user1','notebookaaaa000000000001','Trashed','gone','gone','','','gone',1,1,0,0,0,0,0,0,0,1773999800000,1773999800000,NULL);" || exit 2
cmp_case show bbbb3
cmp_case show zzzz
cmp_case show
cmp_case show ""
cmp_case --help
cmp_case help
cmp_case help add
cmp_case add --help
cmp_case list --help
cmp_case show --help
cmp_case add -t onlytitle
cmp_case add -c onlycontent
cmp_case add
STDIN_DATA="# Piped body
line two" cmp_case add -t "Pipe test" -g "t1, t2"
STDIN_DATA="plain stdin" cmp_case -t "Default cmd"
STDIN_DATA="c" cmp_case add -t "NewNB" -n "FreshNB"
cmp_norm_case list -n FreshNB -v
cmp_norm_case list -N 100

# add produces random ids + wall-clock times; compare with ids/times normalized
JID=$(HOME="$JH" node --require "$SHIM" "$JS" list -n FreshNB | head -1 | cut -d' ' -f1)
RID=$(HOME="$RH" "$BIN" list -n FreshNB | head -1 | cut -d' ' -f1)
norm() { sed -E -e 's/[0-9a-f]{24}/NORMID/g' -e 's/[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}/NTIME/g'; }
HOME="$JH" node --require "$SHIM" "$JS" list -N 100 | norm > "$SB/j.out"
HOME="$RH" "$BIN" list -N 100 | norm > "$SB/r.out"
if cmp -s "$SB/j.out" "$SB/r.out"; then echo "MATCH: list -N 100 (random ids/times normalized)"; else echo "DIFF: list -N 100 (normalized)"; diff "$SB/j.out" "$SB/r.out"; FAIL=1; fi
HOME="$JH" node --require "$SHIM" "$JS" show "$JID" | norm > "$SB/j.out" 2>&1
HOME="$RH" "$BIN" show "$RID" | norm > "$SB/r.out" 2>&1
if cmp -s "$SB/j.out" "$SB/r.out"; then echo "MATCH: show added-note full output (normalized)"; else echo "DIFF: show added-note"; diff "$SB/j.out" "$SB/r.out"; FAIL=1; fi

# the Rust CLI must leave the sandboxed NeDB tree byte-identical
NEDB_SUM_AFTER=$(find "$RH/Library/Application Support/leanote" -type f -exec shasum {} \; | sort | shasum)
if [ "$NEDB_SUM_BEFORE" = "$NEDB_SUM_AFTER" ]; then
  echo "MATCH: Rust store never modified Electron NeDB files"
else
  echo "FAIL: Rust CLI modified the Electron data dir"; FAIL=1
fi

rm -rf "$SB" "$SHIM"
if [ "$FAIL" -eq 0 ]; then echo "ALL BYTE-COMPARISONS PASSED"; else echo "BYTE-COMPARISONS FAILED"; exit 1; fi

# Issue 20 — 8 defect fixes: nav layout, tag pipeline, tray removal

Status: resolved

## Context

Rust (GPUI) desktop client follow-up to issue 19. User-reported defect list
(2026-10-22, Chinese), each adjudicated via the grilling ask round (Q1–Q9).
Zero code changes so far in this issue.

## Items

1. **Notebook context menu items no-op.** Menu opens; 重命名/删除/移动 do
   nothing visible. Root cause (code read): `invalidate_lists()` clears
   tag/name/note caches but NOT `cached_notebook_nodes` — every notebook
   mutation re-renders from the stale cache, so the tree never changes.
   Fix: include `cached_notebook_nodes` in the invalidation.
2. **Selection highlight margins asymmetric.** `nav_row` uses `pl(indent)`
   with depth-based indent + `pr_3()`. User ruling Q2: symmetric **8/8** —
   `pl(px(8. + 12. * depth)).pr_2()` (8px both sides, 12px per nesting level).
3. **Notebook list must fill all leftover left-column height.** Currently
   only the open group's wrapper is `flex_1()` and tags cap at `max_h(300)`;
   a bottom spacer + footer `flex_1` wrapper steal height.
4. **Notebooks/Tags accordions mutually exclusive.** Opening one closes the
   other (group_header writes both prefs fields). First-run conflict
   (both default true): notebooks win, tags_open forced false + persisted.
5. **Tag section empty.** Sync writes server JSON strings (`["rust"]`)
   verbatim into notes.Tags (sync.rs `Some(Value::String(s)) => s.clone()`);
   CSV matching then never hits. Fix: parse JSON array→CSV at the sync
   boundary + one-shot migration (`PRAGMA user_version` 0→1) converting
   legacy JSON-string rows.
6. **Tag list source of truth.** `tag_nav_nodes` reads the tags table
   (only 1 row live DB; Count stays stale). Change to pure notes.Tags
   aggregation (dedupe + live counts), like `tag_note_counts` but without
   the tags-table LEFT JOIN.
7. **⌘, / Ctrl+, opens settings.** Only this shortcut; nothing else moves.
   New `OpenSettings` action + bind_keys + root `on_action` → `open_settings`.
8. **Tray removed entirely.** tray.rs, mod decl, init call, `tray-icon` +
   `image` Cargo deps, i18n TrayShow/TrayQuit keys + ALL_KEYS entries.
   ⌘Q path unaffected (no app-level quit binding existed; macOS AppKit
   handles ⌘Q via `terminate:`).

## Adjudications (2026-10-22)

- Q1 menu pops but items no-op (not a popup failure)
- Q2 8/8 symmetric (overrides recommended 8/12)
- Q3 layout: open group's vlist takes `flex_1().min_h_0()`, trash row +
  footer at natural height (spacer + footer flex wrapper deleted)
- Q4 notebooks expanded / tags collapsed on first-run conflict
- Q5 normalize JSON→CSV at sync boundary + migrate存量
- Q6 tag list aggregates from notes.Tags (dedupe + counts)
- Q7 top bar keeps drag region + search button + 44px height, text gone
- Q8 ⌘, only
- Q9 tray fully deleted

## Plan

Order: 5 (sync + migration + aggregation) → 3+4 (layout + accordion) →
2 (margins) → 6 (header label) → 1 (invalidation fix) → 7 (shortcut) →
8 (tray removal) → smoke.

## Comments

- 2026-10-22: Created. Grilling complete, implementation starting.

- 2026-10-22: All 8 items implemented and verified. Build clean (only
  pre-existing third-party warning), 98 tests pass. Live DB migration ran:
  11 JSON-string Tags rows → 0, user_version=1; tag aggregation returns 87
  distinct tags. One bug caught during verification: the aggregation SQL
  originally built `[rust","gpui]` (missing edge quotes) — json_each silently
  parsed `rust"` as one element; fixed to `'["' || ... || '"]'`. Visual
  smoke: no Leanote header label, tag list renders `# name` rows filling
  the column, ⌘, opens settings dialog, no tray process/symbols.

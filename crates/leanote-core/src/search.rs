//! Hybrid search core (ticket: FTS5 + sqlite-vec + RRF fusion).
//!
//! FTS5: standalone `notes_fts(note_id UNINDEXED, title, body, tags)` table
//! holding bigram-preprocessed text, rows managed from the Rust write seams
//! ([`update_fts`]). Chinese-oriented preprocessing: CJK runs become char
//! bigrams (plus unigrams so 1-char queries hit), latin/digit runs become
//! lowercase words. Query side emits the same bigrams wrapped in one FTS5
//! phrase — phrase adjacency over bigrams == substring semantics for CJK.
//!
//! Vector path: `vec_notes` (sqlite-vec vec0, created by [`Store::embed_stale`])
//! is probed per query; any failure (no table, extension missing, embed API
//! down) silently degrades to FTS-only. Fusion is RRF k=60.

use crate::embed::{self, EmbeddingConfig};
use crate::{Id, SearchHit, Store};
use rusqlite::{params, Connection, OptionalExtension};

/// KNN candidate depth for the vector path (design: top-100).
const KNN_K: i64 = 100;
/// FTS candidate depth feeding RRF; ≥ any UI limit, capped for sanity.
fn fts_fetch(limit: usize) -> i64 {
    limit.clamp(100, 1000) as i64
}
/// RRF constant k.
const RRF_K: f32 = 60.0;

/// Index-side tokenization: every maximal alphanumeric run (CJK or
/// latin/digit mixed) emits unigrams then bigrams — substring semantics
/// for both scripts (query-side phrases over bigrams), unigrams keep
/// 1-char queries hit-able. Unigrams-before-bigrams keeps bigram phrases
/// contiguous, so cross-script substrings inside a run also match.
pub fn bigram_tokenize(text: &str) -> Vec<String> {
    fn flush(run: &mut Vec<char>, out: &mut Vec<String>) {
        if run.is_empty() {
            return;
        }
        if run.len() == 1 {
            out.push(run[0].to_string());
        } else {
            for c in run.iter() {
                out.push(c.to_string());
            }
            for w in run.windows(2) {
                out.push(w.iter().collect());
            }
        }
        run.clear();
    }
    let mut out = Vec::new();
    let mut run: Vec<char> = Vec::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            run.extend(c.to_lowercase());
        } else {
            flush(&mut run, &mut out);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// Query-side tokenization: per run → bigrams only (1-char run → unigram).
/// A phrase over these tokens matches any longer run containing the query
/// run as a substring.
fn query_tokenize(text: &str) -> Vec<String> {
    fn flush(run: &mut Vec<char>, out: &mut Vec<String>) {
        if run.is_empty() {
            return;
        }
        if run.len() == 1 {
            out.push(run[0].to_string());
        } else {
            for w in run.windows(2) {
                out.push(w.iter().collect());
            }
        }
        run.clear();
    }
    let mut out = Vec::new();
    let mut run: Vec<char> = Vec::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            run.extend(c.to_lowercase());
        } else {
            flush(&mut run, &mut out);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// FTS5 MATCH expression for a raw user query: one quoted phrase over the
/// query tokens (tokens never contain `"` — it's a separator — so no
/// escaping is needed). None when the query has no indexable token.
pub(crate) fn fts_expr(query: &str) -> Option<String> {
    let toks = query_tokenize(query);
    if toks.is_empty() {
        return None;
    }
    Some(format!("\"{}\"", toks.join(" ")))
}

/// Text that gets indexed/embedded: markdown bodies keep their text
/// (syntax markers are tolerable noise), HTML bodies (IsMarkdown=0) are
/// stripped of tags first.
pub(crate) fn plain_text(content: &str, is_markdown: bool) -> String {
    if is_markdown {
        content.to_string()
    } else {
        strip_html(content)
    }
}

/// Minimal HTML tag stripper for indexing: drops `<...>` spans, tag ends
/// become spaces (word separation), five common entities decoded.
/// ponytail: script/style bodies survive (their text is still "content" to
/// a tag stripper without a DOM); harmless for search, swap in an HTML
/// parser only if it ever measurably hurts.
pub(crate) fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn tokens_text(tokens: &[String]) -> String {
    tokens.join(" ")
}

/// The single FTS write seam: delete + re-insert one note's row. Bigram
/// text is built here; callers pass raw title/content/tags-csv.
pub(crate) fn update_fts(
    conn: &Connection,
    note_id: &str,
    title: &str,
    content: &str,
    tags_csv: &str,
    is_markdown: bool,
) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM notes_fts WHERE note_id=?1", params![note_id])?;
    conn.execute(
        "INSERT INTO notes_fts (note_id, title, body, tags) VALUES (?1,?2,?3,?4)",
        params![
            note_id,
            tokens_text(&bigram_tokenize(title)),
            tokens_text(&bigram_tokenize(&plain_text(content, is_markdown))),
            tokens_text(&bigram_tokenize(tags_csv)),
        ],
    )?;
    Ok(())
}

/// Body-only FTS refresh for the sync content phase: the row was already
/// created by `apply_sync_notes` (empty body), this fills it once
/// `getNoteContent` lands. IsMarkdown is read from the note row (HTML
/// bodies get tag-stripped).
pub(crate) fn update_fts_body(
    conn: &Connection,
    note_id: &str,
    content: &str,
) -> rusqlite::Result<()> {
    let is_md: bool = conn
        .query_row(
            "SELECT IsMarkdown<>0 FROM notes WHERE NoteId=?1",
            params![note_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(true);
    conn.execute(
        "UPDATE notes_fts SET body=?2 WHERE note_id=?1",
        params![
            note_id,
            tokens_text(&bigram_tokenize(&plain_text(content, is_md)))
        ],
    )?;
    Ok(())
}

/// FTS candidates (note ids) ranked by BM25, live notes only.
fn fts_candidates(conn: &Connection, query: &str, fetch: i64) -> Vec<String> {
    let Some(expr) = fts_expr(query) else {
        return Vec::new();
    };
    let sql = "SELECT f.note_id FROM notes_fts f
        JOIN notes n ON n.NoteId = f.note_id
        WHERE notes_fts MATCH ?1
          AND n.IsTrash=0 AND n.LocalIsDelete=0 AND n.IsDeleted=0
        ORDER BY bm25(notes_fts), f.rowid LIMIT ?2";
    let Ok(mut stmt) = conn.prepare(sql) else {
        return Vec::new();
    };
    stmt.query_map(params![expr, fetch], |r| r.get::<_, String>(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// Reciprocal-rank fusion, k=60: `score = Σ 1/(60 + rank)`, 0-based ranks.
/// Output ordered score-desc; ties keep FTS-first (stable sort over the
/// FTS-then-VEC insertion order). `matched_via` marks which list(s) hit.
pub fn rrf_fuse(fts: &[String], vec: &[String]) -> Vec<(String, f32, Vec<&'static str>)> {
    let mut order: Vec<String> = Vec::new();
    let mut idx: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut scores: Vec<(f32, bool, bool)> = Vec::new(); // (rrf, in_fts, in_vec)
    for (rank, id) in fts.iter().enumerate() {
        let i = *idx.entry(id.clone()).or_insert_with(|| {
            order.push(id.clone());
            scores.push((0.0, false, false));
            order.len() - 1
        });
        scores[i].0 += 1.0 / (RRF_K + rank as f32);
        scores[i].1 = true;
    }
    for (rank, id) in vec.iter().enumerate() {
        let i = *idx.entry(id.clone()).or_insert_with(|| {
            order.push(id.clone());
            scores.push((0.0, false, false));
            order.len() - 1
        });
        scores[i].0 += 1.0 / (RRF_K + rank as f32);
        scores[i].2 = true;
    }
    let mut fused: Vec<(String, f32, Vec<&'static str>)> = order
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let (s, f, v) = scores[i];
            let mut via = Vec::new();
            if f {
                via.push("fts");
            }
            if v {
                via.push("vec");
            }
            (id.clone(), s, via)
        })
        .collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused
}

/// Hybrid search: (1) FTS/BM25 ranking, (2) optional vector KNN when an
/// embedding config is present and the vec path is healthy, (3) RRF fusion.
/// Single-path = that path's order. Never fails: embedding/vector errors
/// degrade to FTS-only. Sync/blocking — callers run it off the UI thread.
/// Repair FTS drift against external DB writers (JS CLI writes straight to
/// SQLite; its fixtures do the same). The Rust write seams keep notes_fts
/// current, but they are not the only writer. Diff note ids both ways:
/// orphans removed, missing rows backfilled from notes.
// ponytail: O(all notes) per search; the old substring search paid the same
// cost on every query. Move to a trigger/sidecar check when that bites.
fn reconcile_fts(conn: &Connection) {
    let fts_ids: std::collections::HashSet<String> = conn
        .prepare("SELECT note_id FROM notes_fts")
        .and_then(|mut s| {
            s.query_map([], |r| r.get::<_, String>(0))
                .map(|rows| rows.flatten().collect())
        })
        .map_or(Vec::new(), |v| v)
        .into_iter()
        .collect();
    let notes: Vec<(String, String, String, String, bool)> = conn
        .prepare("SELECT NoteId, Title, Content, Tags, IsMarkdown<>0 FROM notes")
        .and_then(|mut s| {
            s.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get::<_, i64>(4)? != 0,
                ))
            })
            .map(|rows| rows.flatten().collect())
        })
        .map_or(Vec::new(), |v| v);
    let note_ids: std::collections::HashSet<&str> = notes.iter().map(|n| n.0.as_str()).collect();
    for id in fts_ids.iter() {
        if !note_ids.contains(id.as_str()) {
            let _ = conn.execute("DELETE FROM notes_fts WHERE note_id=?1", params![id]);
        }
    }
    for (id, title, content, tags, is_md) in notes {
        if !fts_ids.contains(&id) {
            let _ = update_fts(conn, &id, &title, &content, &tags, is_md);
        }
    }
}

pub fn search_hybrid(
    store: &Store,
    embedding_config: Option<&EmbeddingConfig>,
    query: &str,
    limit: usize,
) -> Vec<SearchHit> {
    if query.trim().is_empty() || limit == 0 {
        return Vec::new();
    }
    reconcile_fts(&store.conn);
    let fts = fts_candidates(&store.conn, query, fts_fetch(limit));
    let vec: Vec<String> = match embedding_config {
        Some(cfg) if store.vec_loaded => {
            // one embed call for the query; any failure → degrade to FTS
            embed::embed_texts(cfg, &[query])
                .ok()
                .and_then(|v| v.into_iter().next())
                .and_then(|q| embed::knn(&store.conn, &q, KNN_K).ok())
                .unwrap_or_default()
        }
        _ => Vec::new(),
    };
    let kw_lower = query.to_lowercase();
    let mut out = Vec::new();
    for (note_id, _score, via) in rrf_fuse(&fts, &vec).into_iter().take(limit) {
        // get_note = live filter: vec KNN may surface trashed/deleted rows
        let Ok(Some(note)) = store.get_note(&Id(note_id)) else {
            continue;
        };
        out.push(SearchHit {
            score: crate::hit_score(&note, &kw_lower),
            excerpt: crate::excerpt(&note, query),
            matched_via: via,
            note,
        });
    }
    out
}

impl Store {
    /// [`Store::search`] with an explicit embedding config (None = FTS-only).
    pub fn search_with(
        &self,
        embedding_config: Option<&EmbeddingConfig>,
        keyword: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<SearchHit>> {
        Ok(search_hybrid(self, embedding_config, keyword, limit))
    }

    /// Full FTS rebuild from the notes table (drift repair; the write seams
    /// keep it current in normal operation).
    pub fn reindex_fts(&self) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM notes_fts", [])?;
        let notes: Vec<(String, String, String, String, bool)> = {
            let mut stmt =
                tx.prepare("SELECT NoteId, Title, Content, Tags, IsMarkdown<>0 FROM notes")?;
            let v = stmt
                .query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get::<_, i64>(4)? != 0,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            v
        };
        for (id, title, content, tags, is_md) in &notes {
            update_fts(&tx, id, title, content, tags, *is_md)?;
        }
        tx.commit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigram_cjk_run() {
        // unigrams then bigrams, in source order
        assert_eq!(
            bigram_tokenize("笔记搜索"),
            ["笔", "记", "搜", "索", "笔记", "记搜", "搜索"]
        );
        // 1-char run → lone unigram (1–2 char queries must hit)
        assert_eq!(bigram_tokenize("笔"), ["笔"]);
        assert_eq!(bigram_tokenize("笔记"), ["笔", "记", "笔记"]);
    }

    #[test]
    fn bigram_latin_and_mixed() {
        // latin runs are bigrammed too — substring semantics ("e" must
        // hit "Meeting", JS-CLI parity)
        assert_eq!(
            bigram_tokenize("Hello, Rust"),
            [
                "h", "e", "l", "l", "o", "he", "el", "ll", "lo", "r", "u", "s", "t", "ru", "us",
                "st"
            ]
        );
        // one mixed CJK+latin run: bigrams cross the script boundary
        // (用r … t写)
        assert_eq!(
            bigram_tokenize("用Rust写笔记"),
            [
                "用", "r", "u", "s", "t", "写", "笔", "记", "用r", "ru", "us", "st", "t写", "写笔",
                "笔记"
            ]
        );
    }

    #[test]
    fn query_side_is_bigram_phrase() {
        assert_eq!(fts_expr("笔记搜索").as_deref(), Some("\"笔记 记搜 搜索\""));
        assert_eq!(fts_expr("索").as_deref(), Some("\"索\""));
        assert_eq!(fts_expr("e").as_deref(), Some("\"e\""));
        assert_eq!(
            fts_expr("Meeting").as_deref(),
            Some("\"me ee et ti in ng\"")
        );
        assert_eq!(fts_expr("  ,, "), None);
    }

    #[test]
    fn strip_html_drops_tags_and_decodes_entities() {
        assert_eq!(
            strip_html("<p>Hello&nbsp;&amp;welcome</p>"),
            " Hello &welcome "
        );
        assert_eq!(strip_html("plain"), "plain");
    }

    #[test]
    fn rrf_fusion_order_and_via() {
        let fts = ["a", "b", "c"].map(String::from).to_vec();
        let vec = ["b", "d"].map(String::from).to_vec();
        let fused = rrf_fuse(&fts, &vec);
        // b: 1/60 + 1/61 > a: 1/60 (tie-broken after), then c, then d
        assert_eq!(fused[0].0.as_str(), "b");
        assert_eq!(fused[0].2, vec!["fts", "vec"]);
        let ids: Vec<&str> = fused.iter().map(|(id, _, _)| id.as_str()).collect();
        assert_eq!(
            ids,
            ["b", "a", "d", "c"],
            "d (vec rank 1, 1/61) beats c (fts rank 2, 1/62)"
        );
        assert_eq!(fused[2].2, vec!["vec"], "d at index 2, vec-only");
        // single path = that path's order
        let only = rrf_fuse(&fts, &[]);
        assert_eq!(
            only.iter().map(|(i, _, _)| i.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn rrf_scores() {
        let fused = rrf_fuse(&["a".into()], &["a".into()]);
        assert!((fused[0].1 - (1.0 / 60.0 + 1.0 / 60.0)).abs() < 1e-9);
    }
}

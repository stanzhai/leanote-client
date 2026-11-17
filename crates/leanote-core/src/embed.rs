//! Embedding client + vector index (sqlite-vec) for hybrid search.
//!
//! OpenAI-compatible `POST {base_url}/embeddings`, ureq (same client
//! family as sync.rs). The vec0 table `vec_notes(note_id, embedding
//! FLOAT[N])` is created on first index build; `search_meta` remembers the
//! model/dim so a config change drops and rebuilds the table. Core never
//! persists [`EmbeddingConfig`] — callers (desktop Prefs) own it.

use crate::Store;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Embedding endpoint config. Serialized by the caller (Prefs); core only
/// reads it. Empty strings = unset (callers treat that as "disabled").
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub dim: Option<u32>,
}

impl EmbeddingConfig {
    pub fn is_set(&self) -> bool {
        !self.base_url.is_empty() && !self.model.is_empty()
    }
}

/// Embed texts against an OpenAI-compatible `/embeddings` endpoint.
/// Returns embeddings in input order (sorted by the response's `index`
/// field when present). One HTTP call per invocation — batch at the call
/// site (16 per batch is the convention).
pub fn embed_texts(cfg: &EmbeddingConfig, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!("{}/embeddings", cfg.base_url.trim_end_matches('/'));
    let agent = crate::sync::default_agent();
    let resp: serde_json::Value = agent
        .post(&url)
        .header("Authorization", format!("Bearer {}", cfg.api_key))
        .send_json(&serde_json::json!({"model": cfg.model, "input": texts}))
        .map_err(|e| format!("embeddings request failed: {e}"))?
        .body_mut()
        .read_json::<serde_json::Value>()
        .map_err(|e| format!("embeddings response unreadable: {e}"))?;
    let mut items: Vec<(usize, Vec<f32>)> = resp
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| "embeddings response has no data array".to_string())?
        .iter()
        .enumerate()
        .map(|(fallback_idx, item)| {
            let idx = item
                .get("index")
                .and_then(|i| i.as_u64())
                .map(|i| i as usize)
                .unwrap_or(fallback_idx);
            let emb = item
                .get("embedding")
                .and_then(|e| e.as_array())
                .ok_or_else(|| "embedding item missing vector".to_string())?
                .iter()
                .map(|v| {
                    v.as_f64()
                        .map(|f| f as f32)
                        .ok_or_else(|| "embedding element not numeric".to_string())
                })
                .collect::<Result<Vec<f32>, _>>()?;
            Ok((idx, emb))
        })
        .collect::<Result<Vec<_>, String>>()?;
    items.sort_by_key(|(i, _)| *i);
    if items.len() != texts.len() {
        return Err(format!(
            "embeddings returned {} vectors for {} inputs",
            items.len(),
            texts.len()
        ));
    }
    Ok(items.into_iter().map(|(_, v)| v).collect())
}

pub(crate) fn vec_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// KNN over vec_notes; any error (table absent, extension down, dimension
/// mismatch) surfaces as Err and the caller degrades to FTS.
pub(crate) fn knn(conn: &Connection, query: &[f32], k: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT note_id FROM vec_notes WHERE embedding MATCH ?1 AND k = ?2")?;
    let rows = stmt.query_map(params![vec_bytes(query), k], |r| r.get::<_, String>(0))?;
    rows.collect()
}

/// The text a note is embedded on (title + plain body — same plain_text
/// rule as FTS) and its staleness hash.
fn embedding_input(title: &str, content: &str, is_markdown: bool) -> String {
    format!("{}\n{}", title, crate::search::plain_text(content, is_markdown))
}

fn content_hash(s: &str) -> String {
    use std::hash::{Hash, Hasher};
    // ponytail: DefaultHasher is deterministic within a binary; the hash is
    // only ever compared against values this same version wrote, so
    // cross-version stability doesn't matter. Re-embed on upgrade at worst.
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn meta_get(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM search_meta WHERE key=?1",
        params![key],
        |r| r.get::<_, String>(0),
    )
    .optional()
}

fn meta_set(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO search_meta (key, value) VALUES (?1,?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Batch size for embedding calls.
const BATCH: usize = 16;

/// (note_id, embedding input) for every live note whose stored hash is
/// missing or no longer matches — the stale set both indexing APIs share.
fn compute_stale(conn: &Connection) -> Result<Vec<(String, String)>, String> {
    let notes: Vec<(String, String, String, bool)> = {
        let mut stmt = conn
            .prepare(
                "SELECT NoteId, Title, Content, IsMarkdown<>0 FROM notes
                 WHERE IsTrash=0 AND LocalIsDelete=0 AND IsDeleted=0",
            )
            .map_err(|e| e.to_string())?;
        let v = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)? != 0,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        v
    };
    let hashes: std::collections::HashMap<String, String> = {
        let mut stmt = conn
            .prepare("SELECT note_id, content_hash FROM embeddings")
            .map_err(|e| e.to_string())?;
        let v = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        v.into_iter().collect()
    };
    Ok(notes
        .iter()
        .filter_map(|(id, title, content, is_md)| {
            let input = embedding_input(title, content, *is_md);
            let h = content_hash(&input);
            hashes
                .get(id)
                .map_or(true, |old| *old != h)
                .then(|| (id.clone(), input))
        })
        .collect())
}

impl Store {
    /// Incremental vector-index upkeep: embed notes whose content_hash is
    /// missing or stale, in batches of 16, upserting vec_notes/embeddings.
    /// Ensures the vec0 table + search_meta match `cfg` (model/dim change →
    /// drop + recreate). `progress(done, total)` fires per batch. Returns
    /// the number of notes embedded. Errors (API down, vec unavailable)
    /// abort the run — this is an explicit maintenance call, unlike search
    /// which degrades silently.
    pub fn embed_stale(
        &mut self,
        cfg: &EmbeddingConfig,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<usize, String> {
        if !self.vec_loaded {
            return Err("sqlite-vec extension unavailable".into());
        }
        if !cfg.is_set() {
            return Err("embedding config incomplete".into());
        }
        let conn = &self.conn;
        // dim: configured, else probed with one embed call
        let dim = match cfg.dim {
            Some(d) => d as usize,
            None => embed_texts(cfg, &["probe"])?
                .first()
                .map(|v| v.len())
                .ok_or("empty probe embedding")?,
        };
        if !(1..=8192).contains(&dim) {
            return Err(format!("embedding dim {dim} out of range"));
        }
        // model/dim change → full rebuild of the vector side
        let cur_model = meta_get(conn, "embedding_model").map_err(|e| e.to_string())?;
        let cur_dim = meta_get(conn, "embedding_dim")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<usize>().ok());
        if cur_model.as_deref() != Some(cfg.model.as_str()) || cur_dim != Some(dim) {
            conn.execute("DROP TABLE IF EXISTS vec_notes", [])
                .map_err(|e| e.to_string())?;
            conn.execute("DELETE FROM embeddings", [])
                .map_err(|e| e.to_string())?;
        }
        conn.execute(
            &format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS vec_notes USING vec0(
                     note_id TEXT PRIMARY KEY, embedding FLOAT[{dim}])"
            ),
            [],
        )
        .map_err(|e| e.to_string())?;
        meta_set(conn, "embedding_model", &cfg.model).map_err(|e| e.to_string())?;
        meta_set(conn, "embedding_dim", &dim.to_string()).map_err(|e| e.to_string())?;

        let stale = compute_stale(conn)?;



        let total = stale.len();
        let mut done = 0usize;
        let mut embedded = 0usize;
        let now = crate::now_ms();
        for chunk in stale.chunks(BATCH) {
            let inputs: Vec<&str> = chunk.iter().map(|(_, i)| i.as_str()).collect();
            let vectors = embed_texts(cfg, &inputs)?;
            let tx = conn
                .unchecked_transaction()
                .map_err(|e| e.to_string())?;
            for ((id, input), v) in chunk.iter().zip(vectors.iter()) {
                // vec0 has no UPSERT/REPLACE: delete + insert
                tx.execute("DELETE FROM vec_notes WHERE note_id=?1", params![id])
                    .map_err(|e| e.to_string())?;
                tx.execute(
                    "INSERT INTO vec_notes (note_id, embedding) VALUES (?1, ?2)",
                    params![id, vec_bytes(v)],
                )
                .map_err(|e| e.to_string())?;
                tx.execute(
                    "INSERT INTO embeddings (note_id, content_hash, updated_at) VALUES (?1,?2,?3)
                     ON CONFLICT(note_id) DO UPDATE SET
                        content_hash=excluded.content_hash, updated_at=excluded.updated_at",
                    params![id, content_hash(input), now],
                )
                .map_err(|e| e.to_string())?;
            }
            tx.commit().map_err(|e| e.to_string())?;
            done += chunk.len();
            embedded += chunk.len();
            progress(done, total);
        }
        Ok(embedded)
    }

    /// Full vector re-index: forget every stored hash so all live notes
    /// re-embed (then [`Store::embed_stale`] does the work).
    pub fn reindex_embedding(
        &mut self,
        cfg: &EmbeddingConfig,
        progress: impl FnMut(usize, usize),
    ) -> Result<usize, String> {
        self.conn
            .execute("DELETE FROM embeddings", [])
            .map_err(|e| e.to_string())?;
        self.embed_stale(cfg, progress)
    }
}

impl Store {
    /// (indexed, total, model_name) for the settings UI: embedding rows
    /// vs live notes, plus the indexed model from search_meta.
    pub fn index_stats(&self) -> (usize, usize, String) {
        let count = |sql: &str| {
            self.conn
                .query_row(sql, [], |r| r.get::<_, i64>(0))
                .unwrap_or(0) as usize
        };
        let indexed = count("SELECT COUNT(*) FROM embeddings");
        let total = count(
            "SELECT COUNT(*) FROM notes
             WHERE IsTrash=0 AND LocalIsDelete=0 AND IsDeleted=0",
        );
        let model = meta_get(&self.conn, "embedding_model")
            .ok()
            .flatten()
            .unwrap_or_default();
        (indexed, total, model)
    }

    /// The background indexer's trigger: true when any live note needs
    /// (re-)embedding under `cfg`, or the indexed model differs. False
    /// when the vec extension/config is unavailable (nothing actionable).
    pub fn embedding_index_stale(&self, cfg: &EmbeddingConfig) -> bool {
        if !self.vec_loaded || !cfg.is_set() {
            return false;
        }
        let cur_model = meta_get(&self.conn, "embedding_model").ok().flatten();
        if cur_model.as_deref() != Some(cfg.model.as_str()) {
            return true;
        }
        compute_stale(&self.conn).map_or(true, |s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_serde_defaults_roundtrip() {
        let cfg: EmbeddingConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg, EmbeddingConfig::default());
        assert!(!cfg.is_set());
        let full = EmbeddingConfig {
            base_url: "http://x".into(),
            api_key: "k".into(),
            model: "m".into(),
            dim: Some(768),
        };
        let back: EmbeddingConfig =
            serde_json::from_str(&serde_json::to_string(&full).unwrap()).unwrap();
        assert_eq!(full, back);
    }

    #[test]
    fn embed_texts_fails_cleanly_on_unreachable_host() {
        let cfg = EmbeddingConfig {
            base_url: "http://127.0.0.1:1".into(),
            api_key: String::new(),
            model: "m".into(),
            dim: None,
        };
        assert!(embed_texts(&cfg, &["x"]).is_err());
    }
}

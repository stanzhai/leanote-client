//! Hybrid search core (FTS5 + sqlite-vec + RRF): migration backfill,
//! write-seam upkeep, Chinese bigram behavior end-to-end, degradation,
//! and a live end-to-end vector path against a local /embeddings server.

use leanote_core::embed::{embed_texts, EmbeddingConfig};
use leanote_core::search::search_hybrid;
use leanote_core::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("leanote-hybrid-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn store(name: &str) -> Store {
    Store::open(tmpdir(name), "user1".to_string()).expect("open store")
}

fn add(store: &Store, title: &str, content: &str, tags: &[&str], is_markdown: bool) -> Note {
    store
        .add_note(NewNote {
            title: title.to_string(),
            content: content.to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            notebook_title: "Notes".to_string(),
            is_markdown,
        })
        .expect("add note")
}

fn ids(hits: &[SearchHit]) -> Vec<String> {
    hits.iter().map(|h| h.note.note_id.0.clone()).collect()
}

#[test]
fn migration_v2_to_v3_backfills_fts() {
    let dir = tmpdir("migrate");
    let s = Store::open(dir.clone(), "user1".into()).unwrap();
    add(&s, "迁移标题", "迁移内容 body", &[], true);
    drop(s);
    // simulate a pre-v3 db: no FTS table, version 2
    let s = Store::open(dir.clone(), "user1".into()).unwrap();
    s.raw_exec("DROP TABLE notes_fts").unwrap();
    s.raw_exec("PRAGMA user_version=2").unwrap();
    drop(s);

    let s = Store::open(dir, "user1".into()).unwrap();
    assert_eq!(s.raw_query_row("PRAGMA user_version").unwrap(), 3);
    assert_eq!(s.search("迁移", 10).unwrap().len(), 1);
    assert_eq!(s.search("body", 10).unwrap().len(), 1, "backfilled body");
}

#[test]
fn seam_add_update_and_search_upkeep() {
    let s = store("seams");
    let n = add(&s, "笔记标题", "正文包含 苹果 apple", &["水果"], true);

    // CJK: full run, mid-run 2-char, and 1-char queries all hit
    assert_eq!(
        ids(&s.search("笔记", 10).unwrap()),
        vec![n.note_id.0.clone()]
    );
    assert_eq!(
        ids(&s.search("记标", 10).unwrap()),
        vec![n.note_id.0.clone()],
        "mid-run substring"
    );
    assert_eq!(s.search("题", 10).unwrap().len(), 1, "1-char query");
    assert_eq!(s.search("苹果", 10).unwrap().len(), 1);
    assert_eq!(s.search("apple", 10).unwrap().len(), 1, "latin word");
    assert_eq!(s.search("笔标", 10).unwrap().len(), 0, "non-adjacent chars");
    assert_eq!(s.search("zzz", 10).unwrap().len(), 0);

    // update_note: FTS row follows the new content
    s.update_note(
        &n.note_id,
        NoteUpdate {
            content: Some("换成 banana 香蕉".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(s.search("苹果", 10).unwrap().len(), 0, "old term gone");
    assert_eq!(s.search("banana", 10).unwrap().len(), 1);
    assert_eq!(s.search("香蕉", 10).unwrap().len(), 1);
}

#[test]
fn seam_sync_apply_and_content_phase() {
    let s = store("sync-apply");
    let now = 1_700_000_000_000i64;
    let row = serde_json::json!({
        "NoteId": "srv123", "NotebookId": "nb1", "Title": "服务器笔记",
        "Tags": ["同步"], "IsMarkdown": true, "IsTrash": false, "IsDeleted": false,
        "Usn": 5, "CreatedTime": "2023-11-14T22:13:20Z", "UpdatedTime": "2023-11-14T22:13:20Z",
    });
    s.apply_sync_notes(std::slice::from_ref(&row), now).unwrap();
    // metadata phase: title/tags searchable, body empty
    assert_eq!(s.search("服务器", 10).unwrap().len(), 1);
    assert_eq!(s.search("同步", 10).unwrap().len(), 1);
    assert_eq!(s.search("正文关键词", 10).unwrap().len(), 0);
    // content phase fills the body
    s.set_note_content_by_server_id("srv123", "这里出现 正文关键词")
        .unwrap();
    assert_eq!(s.search("正文关键词", 10).unwrap().len(), 1);

    // IsDeleted rows drop out of the FTS index
    let del = serde_json::json!({
        "NoteId": "srv123", "IsDeleted": true,
    });
    s.apply_sync_notes(std::slice::from_ref(&del), now).unwrap();
    assert_eq!(s.search("服务器", 10).unwrap().len(), 0);
}

#[test]
fn trash_and_deleted_filtered_from_search() {
    let s = store("trash-filter");
    let n = add(&s, "目标", "content xyz", &[], true);
    s.trash_note(&n.note_id).unwrap();
    assert_eq!(s.search("目标", 10).unwrap().len(), 0);
    assert_eq!(search_hybrid(&s, None, "目标", 10).len(), 0);
}

#[test]
fn html_note_body_stripped_before_indexing() {
    let s = store("html-strip");
    add(
        &s,
        "富文本",
        "<p>这里有 <b>加粗关键词</b> 和标签</p>",
        &[],
        false,
    );
    assert_eq!(
        s.search("加粗关键词", 10).unwrap().len(),
        1,
        "inside-tag text indexed"
    );
    assert_eq!(
        s.search("b加粗", 10).unwrap().len(),
        0,
        "markup not indexed as text"
    );
}

#[test]
fn degrade_without_config_and_with_unreachable_embed() {
    let s = store("degrade");
    let n = add(&s, "降级测试", "plain body", &[], true);
    // no config → FTS-only
    let hits = search_hybrid(&s, None, "降级", 10);
    assert_eq!(ids(&hits), vec![n.note_id.0.clone()]);
    assert_eq!(hits[0].matched_via, vec!["fts"]);
    // config present but unreachable → still FTS results, no error
    let cfg = EmbeddingConfig {
        base_url: "http://127.0.0.1:1".to_string(),
        api_key: "k".to_string(),
        model: "m".to_string(),
        dim: Some(2),
    };
    let hits = search_hybrid(&s, Some(&cfg), "降级", 10);
    assert_eq!(ids(&hits), vec![n.note_id.0.clone()]);
    assert_eq!(hits[0].matched_via, vec!["fts"], "vec silently skipped");
}

#[test]
fn empty_and_punctuation_only_queries_return_nothing() {
    let s = store("empty-query");
    add(&s, "T", "body", &[], true);
    assert!(search_hybrid(&s, None, "", 10).is_empty());
    assert!(search_hybrid(&s, None, "  ,, ", 10).is_empty());
}

#[test]
fn limit_truncates() {
    let s = store("limit");
    for i in 0..5 {
        add(&s, &format!("共享标题{i}"), "同一段正文", &[], true);
    }
    assert_eq!(search_hybrid(&s, None, "共享标题", 3).len(), 3);
}

#[test]
fn reindex_fts_repairs_drift() {
    let s = store("reindex");
    let n = add(&s, "修复", "原始内容", &[], true);
    s.raw_exec("DELETE FROM notes_fts").unwrap();
    // search-time reconciliation self-heals drift (external DB writers
    // bypass the Rust write seams) — no explicit reindex needed
    assert_eq!(
        ids(&s.search("原始内容", 10).unwrap()),
        vec![n.note_id.0.clone()]
    );
    // reindex_fts remains the explicit full rebuild
    s.reindex_fts().unwrap();
    assert_eq!(
        ids(&s.search("原始内容", 10).unwrap()),
        vec![n.note_id.0.clone()]
    );
}

// --- live end-to-end vector path against a local embeddings server ---

/// Minimal OpenAI-compatible /embeddings server: vectors keyed on the
/// input text ("apple" → [1,0], else [0,1]). Serves several requests
/// (index build, query embed, rebuild), one connection each.
fn spawn_embed_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for _ in 0..8 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            loop {
                let n = stream.read(&mut tmp).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
                let s = String::from_utf8_lossy(&buf);
                if let Some(pos) = s.find("\r\n\r\n") {
                    let cl = s
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if buf.len() >= pos + 4 + cl {
                        break;
                    }
                }
            }
            let body = String::from_utf8_lossy(&buf);
            let json: serde_json::Value =
                serde_json::from_str(body.split("\r\n\r\n").nth(1).unwrap_or(""))
                    .unwrap_or_default();
            let data: Vec<serde_json::Value> = json
                .get("input")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .enumerate()
                        .map(|(i, t)| {
                            let vec: Vec<f32> = if t.as_str().unwrap_or("").contains("apple") {
                                vec![1.0, 0.0]
                            } else {
                                vec![0.0, 1.0]
                            };
                            serde_json::json!({"index": i, "embedding": vec})
                        })
                        .collect()
                })
                .unwrap_or_default();
            let payload = serde_json::json!({"data": data}).to_string();
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                )
                .as_bytes(),
            );
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn cfg_for(model: &str) -> EmbeddingConfig {
    EmbeddingConfig {
        base_url: "http://127.0.0.1:1".to_string(),
        api_key: String::new(),
        model: model.to_string(),
        dim: None,
    }
}

#[test]
fn hybrid_e2e_vec_and_fts_fused_with_rrf() {
    let mut s = store("e2e");
    let apple = add(&s, "Apple pie", "how to bake an apple pie", &[], true);
    let other = add(
        &s,
        "unrelated",
        "completely different topic about databases",
        &[],
        true,
    );
    let cfg = EmbeddingConfig {
        base_url: spawn_embed_server(),
        api_key: "k".to_string(),
        model: "test-model".to_string(),
        dim: Some(2),
    };

    // build the vector index (2 notes → 1 batch request)
    assert_eq!(s.embed_stale(&cfg, |_, _| {}).unwrap(), 2);
    assert_eq!(s.index_stats().0, 2, "index_stats counts embeddings");
    assert_eq!(s.index_stats().2, "test-model");

    // idempotent: hashes fresh on the second run
    assert_eq!(s.embed_stale(&cfg, |_, _| {}).unwrap(), 0);
    assert!(!s.embedding_index_stale(&cfg), "nothing stale after index");
    assert!(
        s.embedding_index_stale(&cfg_for("other-model")),
        "model change → stale"
    );

    let hits = search_hybrid(&s, Some(&cfg), "apple", 10);
    assert_eq!(hits[0].note.note_id, apple.note_id, "apple tops RRF");
    assert!(hits[0].matched_via.contains(&"fts"));
    assert!(
        hits[0].matched_via.contains(&"vec"),
        "apple hit via both paths"
    );
    let via_other = hits
        .iter()
        .find(|h| h.note.note_id == other.note_id)
        .unwrap();
    assert_eq!(via_other.matched_via, vec!["vec"], "db note only via KNN");

    // model change → table dropped and rebuilt (same dim; the fake server
    // only produces 2-dim vectors)
    let cfg2 = EmbeddingConfig {
        base_url: spawn_embed_server(),
        api_key: "k".to_string(),
        model: "test-model-2".to_string(),
        dim: Some(2),
    };
    s.reindex_embedding(&cfg2, |_, _| {}).unwrap();
    assert_eq!(s.index_stats().0, 2, "rebuild re-embedded all");
    assert_eq!(s.index_stats().2, "test-model-2");
}

#[test]
fn embed_client_hits_the_wire() {
    let cfg = EmbeddingConfig {
        base_url: spawn_embed_server(),
        api_key: "k".to_string(),
        model: "m".to_string(),
        dim: None,
    };
    let out = embed_texts(&cfg, &["apple", "database"]).unwrap();
    assert_eq!(out[0], vec![1.0, 0.0]);
    assert_eq!(out[1], vec![0.0, 1.0]);
}

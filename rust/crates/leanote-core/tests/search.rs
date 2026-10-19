// Ticket 05: unified search core — ranking, excerpt window, case/substring,
// Chinese input. Semantics mirrored from src/leanote_cli.js cmdSearch.

use leanote_core::{search_notes, Note};

fn note(title: &str, content: &str, tags: &[&str], desc: &str) -> Note {
    Note {
        note_id: leanote_core::Id("000000000000000000000001".into()),
        user_id: "user1".into(),
        notebook_id: leanote_core::Id("nb".into()),
        title: title.into(),
        content: content.into(),
        desc: desc.into(),
        img_src: String::new(),
        tags: tags.iter().map(|s| s.to_string()).collect(),
        abstract_: String::new(),
        is_markdown: true,
        is_trash: false,
        is_dirty: false,
        is_new: false,
        is_deleted: false,
        is_blog: false,
        star: false,
        created_time: 0,
        updated_time: 0,
    }
}

#[test]
fn weights_title3_tags2_content1() {
    let notes = vec![
        note("plain", "has kw", &[], ""),
        note("has kw", "no", &[], ""),
        note("plain", "no", &["kw"], ""),
        note("has kw", "has kw", &["kw"], ""),
    ];
    let hits = search_notes(&notes, "kw", 10);
    let scores: Vec<u32> = hits.iter().map(|h| h.score).collect();
    assert_eq!(scores, vec![6, 3, 2, 1], "Title 3 / Tags 2 / Content 1");
}

#[test]
fn ranking_desc_score_then_input_order() {
    let notes = vec![
        note("a kw", "", &[], ""), // 3
        note("x", "kw", &[], ""),  // 1
        note("b kw", "", &[], ""), // 3
    ];
    let hits = search_notes(&notes, "kw", 10);
    let titles: Vec<&str> = hits.iter().map(|h| h.note.title.as_str()).collect();
    assert_eq!(titles, vec!["a kw", "b kw", "x"]);
}

#[test]
fn case_insensitive_substring() {
    let notes = vec![
        note("Meeting Notes", "agenda", &[], ""),
        note("other", "see the MEETING minutes", &[], ""),
        note("third", "nothing", &["pre-meeting"], ""),
    ];
    assert_eq!(search_notes(&notes, "meeting", 10).len(), 3);
    assert_eq!(search_notes(&notes, "Meeting", 10).len(), 3);
    assert_eq!(search_notes(&notes, "eet", 10).len(), 3, "substring, not word");
    assert_eq!(search_notes(&notes, "zzz", 10).len(), 0);
}

#[test]
fn chinese_input_matches_and_excerpts() {
    let notes = vec![
        note("会议记录", "今天讨论了项目排期", &["项目"], ""),
        note("无关", "没有关键词", &[], ""),
    ];
    let hits = search_notes(&notes, "项目", 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].score, 3, "Title miss, Content 1 + Tags 2");
    assert_eq!(hits[0].note.title, "会议记录");
    // content hit exists → windowed excerpt, not Desc
    assert!(hits[0].excerpt.contains("项目排期"));
}

#[test]
fn excerpt_window_30_before_kwlen50_after() {
    // 40 chars before the kw, kw at index 40, then 60 after.
    let before: String = "0123456789".repeat(4); // 40 chars
    let kw = "KEY";
    let after: String = "abcdefghij".repeat(6); // 60 chars
    let content = format!("{before}{kw}{after}");
    let n = note("t", &content, &[], "");
    let hits = search_notes(std::slice::from_ref(&n), "key", 10);
    assert_eq!(hits.len(), 1);
    let e = &hits[0].excerpt;
    // start = 40-30 = 10 → "..." + 30 chars, then kw, then 50 of 60 → "..."
    assert!(e.starts_with("..."));
    assert!(e.ends_with("..."));
    assert!(!e.contains("0123456789012345678901234567890"), "first 10 chars cut");
    assert!(e.contains("KEY"));
    let stripped = e.trim_start_matches('.').trim_end_matches('.');
    // 30 + 3 + 50 = 83 content chars inside
    assert_eq!(stripped.chars().count(), 83);
}

#[test]
fn excerpt_at_string_start_no_leading_dots() {
    let n = note("t", "KEY then a fairly long tail of text that goes past fifty chars easily", &[], "");
    let hits = search_notes(std::slice::from_ref(&n), "key", 10);
    let e = &hits[0].excerpt;
    assert!(!e.starts_with("..."), "idx-30 clamps to 0, no leading dots");
    assert!(e.starts_with("KEY"));
    assert!(e.ends_with("..."), "tail still cut");
}

#[test]
fn excerpt_falls_back_to_desc80_when_content_misses() {
    let n = note("the kw title", "no hit in body", &[], &"x".repeat(200));
    let hits = search_notes(std::slice::from_ref(&n), "kw", 10);
    assert_eq!(hits[0].excerpt.chars().count(), 80);
    assert_eq!(hits[0].excerpt, "x".repeat(80));
}

#[test]
fn excerpt_newlines_become_spaces() {
    let n = note("t", "first line\nKEY second line", &[], "");
    let hits = search_notes(std::slice::from_ref(&n), "key", 10);
    assert!(!hits[0].excerpt.contains('\n'));
    assert!(hits[0].excerpt.contains("first line KEY"));
}

#[test]
fn limit_truncates_and_empty_keyword_no_hits() {
    let notes: Vec<Note> = (0..5).map(|i| note(&format!("kw{i}"), "", &[], "")).collect();
    assert_eq!(search_notes(&notes, "kw", 3).len(), 3);
    assert!(search_notes(&notes, "", 10).is_empty());
}

#[test]
fn excerpt_uses_utf16_units_like_js() {
    // 4-byte CJK chars before the keyword: JS substring works in UTF-16
    // code units; a Rust char-index port would differ. 40 CJK chars = 80
    // units, so the window starts at unit 50 = char 25.
    let before: String = "中".repeat(40);
    let content = format!("{before}KEY tail");
    let n = note("t", &content, &[], "");
    let hits = search_notes(std::slice::from_ref(&n), "key", 10);
    let e = &hits[0].excerpt;
    assert!(e.starts_with("..."));
    // window contains exactly 15 CJK chars then KEY (unit start 50 → char 25)
    let inner = e.trim_start_matches('.').trim_end_matches('.');
    assert!(inner.starts_with(&"中".repeat(15)));
    assert!(inner.contains("KEY"));
}

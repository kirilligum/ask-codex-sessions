// TEST-004
use ask_codex_sessions::normalize::normalize_thread;
use ask_codex_sessions::types::ThreadMeta;
use std::path::PathBuf;

#[test]
fn test_rollout_becomes_dialogue_chunks_with_entity_text() {
    let thread = ThreadMeta {
        thread_id: "019cc49c-0918-7c11-9a8a-630c28b9b443".to_string(),
        rollout_path: PathBuf::from("tests/fixtures/current-session.jsonl"),
        created_at: 1_772_825_086,
        cwd: PathBuf::from("/home/kirill/p/ask-codex-sessions"),
        title: "fixture".to_string(),
        git_branch: None,
        git_origin_url: None,
    };
    let chunks = normalize_thread(&thread).expect("current-session fixture should normalize");
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|chunk| !chunk.dialogue_text.contains("# AGENTS.md instructions for")));
    assert!(chunks.iter().all(|chunk| !chunk.dialogue_text.contains("I’m checking a few primary sources on search-tool behavior before recommending a stack")));
    assert!(chunks.iter().all(|chunk| chunk.source_start_line > 0));
    assert!(chunks.iter().all(|chunk| chunk.source_end_line >= chunk.source_start_line));
    assert!(chunks.iter().any(|chunk| chunk.entity_text.contains("state_5.sqlite")));
    assert!(chunks
        .iter()
        .all(|chunk| !chunk.entity_text.contains("what tech stack did we choose for the codex session search tool")));
    assert!(chunks.iter().any(|chunk| chunk.dialogue_text.contains("Rust")));
}

// TEST-004
use ask_codex_sessions::normalize::{normalize_thread, normalize_thread_with_stats};
use ask_codex_sessions::types::ThreadMeta;
use std::path::PathBuf;
use tempfile::tempdir;

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

#[test]
fn test_rollout_with_nul_corruption_still_normalizes() {
    let temp = tempdir().expect("temp dir should exist");
    let rollout_path = temp.path().join("corrupted.jsonl");
    std::fs::write(
        &rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-08T20:00:00.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"firebase orchestration interface\"}]}}\n",
            "\0\0\0\0\0\0\0\0\n",
            "{\"timestamp\":\"2026-03-08T20:00:01.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Use createTaskRunner from src/index.js and src/types.d.ts.\"}],\"phase\":\"final_answer\"}}\n"
        ),
    )
    .expect("corrupted rollout fixture should write");

    let thread = ThreadMeta {
        thread_id: "corrupted-thread".to_string(),
        rollout_path,
        created_at: 1_772_825_086,
        cwd: PathBuf::from("/home/kirill/firebase-orchestration"),
        title: "corrupted fixture".to_string(),
        git_branch: None,
        git_origin_url: None,
    };

    let normalized = normalize_thread_with_stats(&thread).expect("best-effort parser should succeed");
    assert_eq!(normalized.stats.skipped_invalid_lines, 1);
    assert_eq!(normalized.chunks.len(), 1);
    assert!(normalized.chunks[0].dialogue_text.contains("firebase orchestration interface"));
    assert!(normalized.chunks[0].dialogue_text.contains("createTaskRunner"));
}

#[test]
fn test_event_messages_recover_useful_conversation() {
    let temp = tempdir().expect("temp dir should exist");
    let rollout_path = temp.path().join("event-only.jsonl");
    std::fs::write(
        &rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-08T20:00:00.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"what is the firebase orchestration interface\",\"images\":[],\"local_images\":[],\"text_elements\":[]}}\n",
            "{\"timestamp\":\"2026-03-08T20:00:01.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"I will inspect the interface files.\",\"phase\":\"commentary\"}}\n",
            "{\"timestamp\":\"2026-03-08T20:00:02.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"last_agent_message\":\"Use createTaskRunner from src/index.js with types from src/types.d.ts.\"}}\n"
        ),
    )
    .expect("event rollout fixture should write");

    let thread = ThreadMeta {
        thread_id: "event-thread".to_string(),
        rollout_path,
        created_at: 1_772_825_086,
        cwd: PathBuf::from("/home/kirill/firebase-orchestration"),
        title: "event fixture".to_string(),
        git_branch: None,
        git_origin_url: None,
    };

    let normalized = normalize_thread_with_stats(&thread).expect("event messages should normalize");
    assert_eq!(normalized.stats.skipped_invalid_lines, 0);
    assert_eq!(normalized.chunks.len(), 1);
    assert!(normalized.chunks[0].dialogue_text.contains("what is the firebase orchestration interface"));
    assert!(normalized.chunks[0].dialogue_text.contains("createTaskRunner"));
    assert!(!normalized.chunks[0].dialogue_text.contains("I will inspect the interface files."));
}

#[test]
fn test_duplicate_response_item_and_event_messages_are_deduped() {
    let temp = tempdir().expect("temp dir should exist");
    let rollout_path = temp.path().join("deduped.jsonl");
    std::fs::write(
        &rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-08T20:00:00.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"firebase orchestration interface\"}]}}\n",
            "{\"timestamp\":\"2026-03-08T20:00:00.100Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"firebase orchestration interface\",\"images\":[],\"local_images\":[],\"text_elements\":[]}}\n",
            "{\"timestamp\":\"2026-03-08T20:00:01.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Use createTaskRunner from src/index.js.\"}],\"phase\":\"final_answer\"}}\n",
            "{\"timestamp\":\"2026-03-08T20:00:01.100Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"last_agent_message\":\"Use createTaskRunner from src/index.js.\"}}\n"
        ),
    )
    .expect("dedupe rollout fixture should write");

    let thread = ThreadMeta {
        thread_id: "dedupe-thread".to_string(),
        rollout_path,
        created_at: 1_772_825_086,
        cwd: PathBuf::from("/home/kirill/firebase-orchestration"),
        title: "dedupe fixture".to_string(),
        git_branch: None,
        git_origin_url: None,
    };

    let normalized = normalize_thread(&thread).expect("dedupe fixture should normalize");
    assert_eq!(normalized.len(), 1);
    assert_eq!(
        normalized[0].dialogue_text.matches("firebase orchestration interface").count(),
        1
    );
    assert_eq!(
        normalized[0]
            .dialogue_text
            .matches("Use createTaskRunner from src/index.js.")
            .count(),
        1
    );
}

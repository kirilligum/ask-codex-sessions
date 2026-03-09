// TEST-011
use ask_codex_sessions::config::Config;
use ask_codex_sessions::debug::DebugEvents;
use ask_codex_sessions::gemini::{default_mock_dir, GeminiClient};
use ask_codex_sessions::search::SearchPipeline;
use ask_codex_sessions::types::{QueryPreset, SearchMode, SearchRequest};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[test]
fn test_search_skips_corrupted_thread_and_returns_good_results() {
    let temp = tempdir().expect("temp dir should exist");
    let db_path = temp.path().join("threads.sqlite");
    let good_rollout_path = temp.path().join("good.jsonl");
    let bad_rollout_path = temp.path().join("bad.jsonl");

    std::fs::write(
        &good_rollout_path,
        concat!(
            "{\"timestamp\":\"2026-03-08T20:00:00.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"what is the firebase orchestration interface\"}]}}\n",
            "{\"timestamp\":\"2026-03-08T20:00:01.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"The interface uses createTaskRunner from src/index.js and types from src/types.d.ts.\"}],\"phase\":\"final_answer\"}}\n"
        ),
    )
    .expect("good rollout should write");
    std::fs::write(&bad_rollout_path, b"\0\0\0\0\0\n").expect("bad rollout should write");

    let connection = Connection::open(&db_path).expect("sqlite should open");
    connection
        .execute_batch(
            "
            CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                rollout_path TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                cwd TEXT NOT NULL,
                title TEXT NOT NULL,
                git_branch TEXT,
                git_origin_url TEXT
            );
            ",
        )
        .expect("threads table should create");
    connection
        .execute(
            "INSERT INTO threads (id, rollout_path, created_at, cwd, title, git_branch, git_origin_url)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL)",
            (
                "good-thread",
                good_rollout_path.to_string_lossy().to_string(),
                1_772_850_000_i64,
                "/home/kirill/firebase-orchestration",
                "good thread",
            ),
        )
        .expect("good row should insert");
    connection
        .execute(
            "INSERT INTO threads (id, rollout_path, created_at, cwd, title, git_branch, git_origin_url)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL)",
            (
                "bad-thread",
                bad_rollout_path.to_string_lossy().to_string(),
                1_772_840_000_i64,
                "/home/kirill/firebase-orchestration",
                "bad thread",
            ),
        )
        .expect("bad row should insert");

    let config = Config {
        state_db_path: db_path,
        sessions_root: temp.path().to_path_buf(),
        gemini_model: "gemini-3-flash-preview".to_string(),
        rerank_limit: 5,
    };
    let debug = DebugEvents::enabled();
    let client = GeminiClient::with_mock_dir(
        "gemini-3-flash-preview",
        default_mock_dir(Path::new(".")),
    )
    .with_debug(debug.clone());
    let pipeline = SearchPipeline::new(config, client).with_debug(debug.clone());
    let request = SearchRequest {
        query: "firebase orchestration interface".to_string(),
        preset: QueryPreset::Search,
        mode: SearchMode::Lexical,
        cwd_filter: Some(PathBuf::from("/home/kirill/firebase-orchestration")),
        timeframe_start: None,
        limit: 3,
    };

    let results = pipeline.search(&request).expect("search should skip bad thread");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].thread_id, "good-thread");
    assert!(results[0].snippet.contains("createTaskRunner"));

    let lines = debug.lines();
    assert!(lines.iter().any(|line| {
        line.contains("skipped 1 invalid rollout line(s) across 1 thread(s)")
            && line.contains("bad-thread")
            && line.contains("bad.jsonl")
    }));
    assert!(lines.iter().any(|line| {
        line.contains("skipped 1 thread(s) with only setup boilerplate or unsupported content")
            && line.contains("bad-thread")
            && line.contains("bad.jsonl")
    }));
}

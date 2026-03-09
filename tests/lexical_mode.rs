// TEST-010
use ask_codex_sessions::config::Config;
use ask_codex_sessions::gemini::{default_mock_dir, GeminiClient};
use ask_codex_sessions::search::SearchPipeline;
use ask_codex_sessions::types::{QueryPreset, SearchMode, SearchRequest};
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[test]
fn test_lexical_mode_finds_current_session_without_gemini_planner() {
    let config = Config {
        state_db_path: PathBuf::from("tests/fixtures/state_5.sqlite"),
        sessions_root: PathBuf::from("tests/fixtures"),
        gemini_model: "gemini-3-flash-preview".to_string(),
        rerank_limit: 5,
    };
    let client = GeminiClient::with_mock_dir(
        "gemini-3-flash-preview",
        default_mock_dir(Path::new(".")),
    );
    let pipeline = SearchPipeline::new(config, client);
    let request = SearchRequest {
        query: "what tech stack did we choose for the session search tool".to_string(),
        preset: QueryPreset::Search,
        mode: SearchMode::Lexical,
        cwd_filter: Some(PathBuf::from("/home/kirill/p/ask-codex-sessions")),
        timeframe_start: None,
        limit: 3,
    };

    let results = pipeline.search(&request).expect("lexical search should succeed");
    assert!(!results.is_empty());
    assert_eq!(results[0].thread_id, "019cc49c-0918-7c11-9a8a-630c28b9b443");
    assert!(
        results[0].snippet.to_ascii_lowercase().contains("rust")
            || results[0].snippet.to_ascii_lowercase().contains("sqlite")
    );
    let unique_threads = results.iter().map(|result| result.thread_id.as_str()).collect::<HashSet<_>>();
    assert_eq!(unique_threads.len(), results.len());
}

#[test]
fn test_limit_zero_means_no_limit_for_lexical_search() {
    let temp = tempdir().expect("temp dir should exist");
    let db_path = temp.path().join("threads.sqlite");
    let rollout_a = temp.path().join("a.jsonl");
    let rollout_b = temp.path().join("b.jsonl");

    std::fs::write(
        &rollout_a,
        concat!(
            "{\"timestamp\":\"2026-03-08T20:00:00.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"firebase orchestration interface\"}]}}\n",
            "{\"timestamp\":\"2026-03-08T20:00:01.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"First answer uses createTaskRunner from src/index.js.\"}],\"phase\":\"final_answer\"}}\n"
        ),
    )
    .expect("rollout a should write");
    std::fs::write(
        &rollout_b,
        concat!(
            "{\"timestamp\":\"2026-03-08T20:01:00.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"firebase orchestration interface\"}]}}\n",
            "{\"timestamp\":\"2026-03-08T20:01:01.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Second answer uses approveManualStep from src/task-runner.js.\"}],\"phase\":\"final_answer\"}}\n"
        ),
    )
    .expect("rollout b should write");

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
    for (id, rollout_path, created_at, title) in [
        ("thread-a", rollout_a.as_path(), 1_772_850_000_i64, "thread a"),
        ("thread-b", rollout_b.as_path(), 1_772_850_100_i64, "thread b"),
    ] {
        connection
            .execute(
                "INSERT INTO threads (id, rollout_path, created_at, cwd, title, git_branch, git_origin_url)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL)",
                (
                    id,
                    rollout_path.to_string_lossy().to_string(),
                    created_at,
                    "/home/kirill/firebase-orchestration",
                    title,
                ),
            )
            .expect("thread row should insert");
    }

    let config = Config {
        state_db_path: db_path,
        sessions_root: temp.path().to_path_buf(),
        gemini_model: "gemini-3-flash-preview".to_string(),
        rerank_limit: 5,
    };
    let client = GeminiClient::with_mock_dir(
        "gemini-3-flash-preview",
        default_mock_dir(Path::new(".")),
    );
    let pipeline = SearchPipeline::new(config, client);
    let request = SearchRequest {
        query: "firebase orchestration interface".to_string(),
        preset: QueryPreset::Search,
        mode: SearchMode::Lexical,
        cwd_filter: Some(PathBuf::from("/home/kirill/firebase-orchestration")),
        timeframe_start: None,
        limit: 0,
    };

    let results = pipeline
        .search(&request)
        .expect("lexical search with limit 0 should succeed");
    assert_eq!(results.len(), 2, "limit 0 should return all matching threads");
    let unique_threads = results.iter().map(|result| result.thread_id.as_str()).collect::<HashSet<_>>();
    assert_eq!(unique_threads.len(), 2);
}

// TEST-011
use ask_codex_sessions::config::Config;
use ask_codex_sessions::gemini::{default_mock_dir, GeminiClient};
use ask_codex_sessions::search::SearchPipeline;
use ask_codex_sessions::types::{QueryPreset, SearchMode, SearchRequest};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[test]
fn test_llm_search_mode_finds_current_session_by_chunk_judging() {
    let config = Config {
        state_db_path: PathBuf::from("tests/fixtures/state_5.sqlite"),
        sessions_root: PathBuf::from("tests/fixtures"),
        gemini_model: "gemini-3-flash-preview".to_string(),
        candidate_limit: 6,
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
        mode: SearchMode::Llm,
        cwd_filter: Some(PathBuf::from("/home/kirill/p/ask-codex-sessions")),
        timeframe_start: None,
        limit: 3,
    };

    let results = pipeline.search(&request).expect("llm search should succeed");
    assert!(!results.is_empty());
    assert_eq!(results[0].thread_id, "019cc49c-0918-7c11-9a8a-630c28b9b443");
    assert!(
        results[0].snippet.to_ascii_lowercase().contains("rust")
            || results[0].snippet.to_ascii_lowercase().contains("sqlite")
            || results[0].snippet.to_ascii_lowercase().contains("gemini")
    );
    let unique_threads = results.iter().map(|result| result.thread_id.as_str()).collect::<HashSet<_>>();
    assert_eq!(unique_threads.len(), results.len());
}

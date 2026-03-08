// TEST-009
use ask_codex_sessions::config::Config;
use ask_codex_sessions::debug::DebugEvents;
use ask_codex_sessions::gemini::{default_mock_dir, GeminiClient};
use ask_codex_sessions::search::SearchPipeline;
use ask_codex_sessions::types::{QueryPreset, SearchMode, SearchRequest};
use std::path::{Path, PathBuf};

#[test]
fn test_debug_trace_reports_pipeline_stages() {
    let config = Config {
        state_db_path: PathBuf::from("tests/fixtures/state_5.sqlite"),
        sessions_root: PathBuf::from("tests/fixtures"),
        gemini_model: "gemini-3-flash-preview".to_string(),
        candidate_limit: 6,
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
        query: "what tech stack did we choose for the session search tool".to_string(),
        preset: QueryPreset::LatestSpec,
        mode: SearchMode::Hybrid,
        cwd_filter: Some(PathBuf::from("/home/kirill/p/ask-codex-sessions")),
        timeframe_start: None,
        limit: 2,
    };

    let results = pipeline.search(&request).expect("debug pipeline search should succeed");
    assert!(!results.is_empty());

    let lines = debug.lines();
    assert!(lines.iter().any(|line| line.contains("search start preset=LatestSpec mode=Hybrid")));
    assert!(lines.iter().any(|line| line.contains("loaded 2 threads")));
    assert!(lines.iter().any(|line| line.contains("filtered to 2 threads")));
    assert!(lines.iter().any(|line| line.contains("normalized")));
    assert!(lines.iter().any(|line| line.contains("observed terms count=")));
    assert!(lines.iter().any(|line| line.contains("gemini query-plan result")));
    assert!(lines.iter().any(|line| line.contains("fts search produced")));
    assert!(lines.iter().any(|line| line.contains("gemini rerank result")));
    assert!(lines.iter().any(|line| line.contains("search done results=")));
}

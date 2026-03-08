// TEST-008
use ask_codex_sessions::output::{build_output_artifact, write_output_artifact, OutputArtifact};
use ask_codex_sessions::types::{
    QueryPreset, ResultSummary, ScoreDetails, SearchMode, SearchRequest, SearchResult, SummaryBundle,
};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;
use time::macros::datetime;

#[test]
fn test_output_contains_json_artifact_with_resume_path_and_quote() {
    let request = SearchRequest {
        query: "what tech stack did we choose for the session search tool".to_string(),
        preset: QueryPreset::LatestSpec,
        mode: SearchMode::Hybrid,
        cwd_filter: Some(PathBuf::from("/home/kirill/p/ask-codex-sessions")),
        timeframe_start: Some(1_772_000_000),
        limit: 3,
    };
    let results = vec![SearchResult {
        session_id: "019cc49c-0918-7c11-9a8a-630c28b9b443".to_string(),
        thread_id: "019cc49c-0918-7c11-9a8a-630c28b9b443".to_string(),
        title: "Session search architecture discussion".to_string(),
        created_at: 1_772_825_086,
        rollout_path: PathBuf::from("/home/kirill/p/ask-codex-sessions/tests/fixtures/current-session.jsonl"),
        chunk_id: "019cc49c-0918-7c11-9a8a-630c28b9b443:4".to_string(),
        source_start_line: 101,
        source_end_line: 104,
        score: ScoreDetails {
            final_score: 9.5,
            bm25_raw: Some(-4.2),
            phrase_matches: 1,
            entity_matches: 2,
            dialogue_matches: 3,
            recency_bonus: 0.35,
            noise_penalty: 0.0,
            llm_rerank_position: Some(0),
            llm_batch_index: None,
            llm_batch_rank: None,
        },
        snippet: "For this project, I’d use Rust and keep the search local with SQLite, BM25, Gemini query planning, and Gemini reranking.".to_string(),
        matched_terms: vec!["rust".to_string(), "sqlite".to_string(), "gemini".to_string()],
        word_count: 20,
        entity_count: 6,
    }];
    let summaries = SummaryBundle {
        overall_summary: "Rust, SQLite, and Gemini were chosen for the tool.".to_string(),
        result_summaries: vec![ResultSummary {
            text_id: "019cc49c-0918-7c11-9a8a-630c28b9b443:4".to_string(),
            summary: "This result states the selected implementation stack directly.".to_string(),
        }],
    };

    let artifact = build_output_artifact(
        &request,
        &results,
        Some(&summaries),
        Some("Use Rust with SQLite and Gemini for this tool."),
        datetime!(2026-03-06 20:30:00 UTC),
    )
    .expect("output artifact should build");
    let dir = tempdir().expect("tempdir should exist");
    let path = write_output_artifact(dir.path(), &artifact).expect("artifact should write");

    assert!(path.exists());
    assert!(path.parent().unwrap().ends_with("ask-codex-session-responses"));
    let content = fs::read_to_string(&path).expect("artifact file should be readable");
    let written: OutputArtifact = serde_json::from_str(&content).expect("artifact file should parse");

    assert_eq!(written.query, request.query);
    assert_eq!(written.result_count, 1);
    assert_eq!(written.summary.as_deref(), Some("Rust, SQLite, and Gemini were chosen for the tool."));
    assert_eq!(written.answer.as_deref(), Some("Use Rust with SQLite and Gemini for this tool."));
    assert_eq!(written.results[0].session_id, "019cc49c-0918-7c11-9a8a-630c28b9b443");
    assert_eq!(
        written.results[0].resume_command,
        "codex resume 019cc49c-0918-7c11-9a8a-630c28b9b443"
    );
    assert_eq!(
        written.results[0].session_path,
        PathBuf::from("/home/kirill/p/ask-codex-sessions/tests/fixtures/current-session.jsonl")
    );
    assert_eq!(written.results[0].text_id, "019cc49c-0918-7c11-9a8a-630c28b9b443:4");
    assert_eq!(written.results[0].source_start_line, 101);
    assert_eq!(written.results[0].source_end_line, 104);
    assert!(written.results[0].quote.contains("Rust"));
    assert_eq!(
        written.results[0].summary.as_deref(),
        Some("This result states the selected implementation stack directly.")
    );
    assert_eq!(written.results[0].score.bm25_raw, Some(-4.2));
    assert!(written.results[0].metadata.matched_terms.contains(&"rust".to_string()));
}

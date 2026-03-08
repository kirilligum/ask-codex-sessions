// TEST-012
use ask_codex_sessions::gemini::{default_mock_dir, GeminiClient};
use ask_codex_sessions::types::{ResultSummary, ScoreDetails, SearchResult, SummaryBundle};
use std::path::{Path, PathBuf};

#[test]
fn test_gemini_summary_and_answer_use_mock_fixtures() {
    let client = GeminiClient::with_mock_dir(
        "gemini-3-flash-preview",
        default_mock_dir(Path::new(".")),
    );
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

    let summaries = client
        .summarize_results("what tech stack did we choose for the session search tool", &results)
        .expect("summary synthesis should load from mock");
    assert_eq!(
        summaries,
        SummaryBundle {
            overall_summary: "Rust, SQLite, and Gemini were chosen for the tool.".to_string(),
            result_summaries: vec![ResultSummary {
                text_id: "019cc49c-0918-7c11-9a8a-630c28b9b443:4".to_string(),
                summary: "This result states the selected implementation stack directly.".to_string(),
            }],
        }
    );

    let answer = client
        .answer_query("what tech stack did we choose for the session search tool", &results)
        .expect("answer synthesis should load from mock");
    assert_eq!(answer, "Use Rust with SQLite and Gemini for this tool.");
}

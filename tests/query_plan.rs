// TEST-006
use ask_codex_sessions::gemini::{default_mock_dir, GeminiClient};
use ask_codex_sessions::types::QueryPreset;
use std::path::Path;

#[test]
fn test_gemini_query_plan_uses_only_observed_terms() {
    let client = GeminiClient::with_mock_dir(
        "gemini-3-flash-preview",
        default_mock_dir(Path::new(".")),
    );
    let observed_terms = vec![
        "rust".to_string(),
        "sqlite".to_string(),
        "gemini".to_string(),
        "hybrid".to_string(),
        "retrieval".to_string(),
        "session".to_string(),
        "search".to_string(),
        "tool".to_string(),
    ];
    let plan = client
        .generate_query_plan(
            "what tech stack did we choose for the session search tool",
            &observed_terms,
            QueryPreset::Search,
        )
        .expect("mock Gemini query plan should load");
    assert!(plan.keywords.contains(&"rust".to_string()));
    assert!(plan.keywords.contains(&"session".to_string()));
    assert!(plan.phrases.contains(&"session search tool".to_string()));
}

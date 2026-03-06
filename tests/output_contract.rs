// TEST-008
use ask_codex_sessions::output::render_output;
use ask_codex_sessions::types::SearchResult;
use std::fs;

#[test]
fn test_output_contains_verbatim_citation_and_handoff() {
    let fixture = fs::read_to_string("tests/fixtures/results/current-session.json")
        .expect("result fixture should load");
    let results: Vec<SearchResult> = serde_json::from_str(&fixture).expect("result fixture should parse");
    let rendered = render_output("what tech stack did we choose for the session search tool", &results)
        .expect("output should render");

    assert!(rendered.contains("019cc49c-0918-7c11-9a8a-630c28b9b443"));
    assert!(rendered.contains("tests/fixtures/current-session.jsonl"));
    assert!(rendered.contains("Use these cited prior-session findings:"));
    assert!(rendered.contains("Rust"));
}

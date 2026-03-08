use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryPreset {
    Search,
    LatestSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchMode {
    Hybrid,
    Lexical,
    Llm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadMeta {
    pub thread_id: String,
    pub rollout_path: PathBuf,
    pub created_at: i64,
    pub cwd: PathBuf,
    pub title: String,
    pub git_branch: Option<String>,
    pub git_origin_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub chunk_id: String,
    pub thread_id: String,
    pub ordinal: usize,
    pub source_start_line: usize,
    pub source_end_line: usize,
    pub user_text: String,
    pub assistant_text: String,
    pub dialogue_text: String,
    pub entity_text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryPlan {
    pub keywords: Vec<String>,
    pub phrases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub query: String,
    pub preset: QueryPreset,
    pub mode: SearchMode,
    pub cwd_filter: Option<PathBuf>,
    pub timeframe_start: Option<i64>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ScoreDetails {
    pub final_score: f64,
    pub bm25_raw: Option<f64>,
    pub phrase_matches: usize,
    pub entity_matches: usize,
    pub dialogue_matches: usize,
    pub recency_bonus: f64,
    pub noise_penalty: f64,
    pub llm_rerank_position: Option<usize>,
    pub llm_batch_index: Option<usize>,
    pub llm_batch_rank: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub session_id: String,
    pub thread_id: String,
    pub title: String,
    pub created_at: i64,
    pub rollout_path: PathBuf,
    pub chunk_id: String,
    pub source_start_line: usize,
    pub source_end_line: usize,
    pub score: ScoreDetails,
    pub snippet: String,
    pub matched_terms: Vec<String>,
    pub word_count: usize,
    pub entity_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchCandidate {
    pub chunk: Chunk,
    pub score: ScoreDetails,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultSummary {
    pub text_id: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryBundle {
    pub overall_summary: String,
    pub result_summaries: Vec<ResultSummary>,
}

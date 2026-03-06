use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryPreset {
    Search,
    LatestSpec,
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
    pub cwd_filter: Option<PathBuf>,
    pub timeframe_start: Option<i64>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    pub thread_id: String,
    pub created_at: i64,
    pub rollout_path: PathBuf,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub thread_id: String,
    pub title: String,
    pub created_at: i64,
    pub rollout_path: PathBuf,
    pub chunk_id: String,
    pub score: f64,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchCandidate {
    pub chunk: Chunk,
    pub score: f64,
}

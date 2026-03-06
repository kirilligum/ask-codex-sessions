use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub state_db_path: PathBuf,
    pub sessions_root: PathBuf,
    pub gemini_model: String,
    pub candidate_limit: usize,
    pub rerank_limit: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            state_db_path: PathBuf::from("/home/kirill/.codex/state_5.sqlite"),
            sessions_root: PathBuf::from("/home/kirill/.codex/sessions"),
            gemini_model: "gemini-3-flash-preview".to_string(),
            candidate_limit: 8,
            rerank_limit: 5,
        }
    }
}

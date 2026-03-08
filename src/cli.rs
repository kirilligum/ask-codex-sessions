use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "ask-codex-sessions")]
#[command(about = "Search prior Codex CLI sessions stored locally under ~/.codex.")]
#[command(
    after_help = "Examples:\n  ask-codex-sessions bm25llm --cwd /path/to/repo --since-days 90 \"firebase orchestration interface\"\n  ask-codex-sessions bm25llm-recent --sum -a \"what was the latest spec for the interface\"\n  ask-codex-sessions bm25 --limit 3 \"rust sqlite gemini\"\n  ask-codex-sessions llm --debug \"find discussions about simplifying the interface\""
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Subcommand, PartialEq, Eq)]
pub enum Command {
    #[command(name = "bm25llm", about = "Hybrid search: Gemini query plan + SQLite FTS/BM25 + Gemini rerank")]
    Bm25llm(QueryArgs),
    #[command(name = "bm25llm-recent", about = "Hybrid search biased toward the newest relevant discussion or spec")]
    Bm25llmRecent(QueryArgs),
    #[command(name = "bm25", about = "Pure local BM25/FTS search, no Gemini calls")]
    Bm25(QueryArgs),
    #[command(name = "llm", about = "LLM-only chunk review: Gemini judges filtered chunks directly")]
    Llm(QueryArgs),
}

#[derive(Debug, Clone, Args, PartialEq, Eq)]
pub struct QueryArgs {
    #[arg(
        value_name = "QUERY",
        help = "Natural-language question or topic to search for in prior sessions"
    )]
    pub query: String,

    #[arg(long, help = "Print pipeline stages, plans, and ranking details to stderr")]
    pub debug: bool,

    #[arg(long = "sum", help = "Add summary fields to the JSON output artifact")]
    pub sum: bool,

    #[arg(
        short = 'a',
        long = "answer",
        help = "Add a top-level answer to the original query in the JSON output artifact"
    )]
    pub answer: bool,

    #[arg(long, value_name = "PATH", help = "Restrict search to sessions from this working directory or repo path")]
    pub cwd: Option<PathBuf>,

    #[arg(long, value_name = "DAYS", help = "Only search sessions from the last N days")]
    pub since_days: Option<u32>,

    #[arg(long, default_value_t = 5, help = "Maximum number of ranked results to return")]
    pub limit: usize,
}

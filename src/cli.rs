use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "ask-codex-sessions")]
#[command(about = "Search prior Codex CLI sessions")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Subcommand, PartialEq, Eq)]
pub enum Command {
    Search(QueryArgs),
    LatestSpec(QueryArgs),
}

#[derive(Debug, Clone, Args, PartialEq, Eq)]
pub struct QueryArgs {
    #[arg(value_name = "QUERY")]
    pub query: String,

    #[arg(long)]
    pub cwd: Option<PathBuf>,

    #[arg(long)]
    pub since_days: Option<u32>,

    #[arg(long, default_value_t = 5)]
    pub limit: usize,
}

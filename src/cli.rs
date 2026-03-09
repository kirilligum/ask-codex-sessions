use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "ask-codex-sessions")]
#[command(
    about = "Search prior Codex CLI sessions stored locally under ~/.codex. Defaults to bm25llm with --since-days 30 and --answer when no mode is given."
)]
#[command(
    override_usage = "ask-codex-sessions [OPTIONS] <QUERY>\n       ask-codex-sessions <COMMAND>"
)]
#[command(
    after_help = "Defaults:\n  no mode given: bm25llm -t 30 -a\n  -C, --cwd: current working directory\n  -l, --limit: 5\n  -o, --out-dir: unset, so JSON is printed to stdout\n\nExamples:\n  ask-codex-sessions -C /path/to/repo -t 90 \"firebase orchestration interface\" | jq '.results[0]'\n  ask-codex-sessions bm25 -C /path/to/repo \"rust sqlite gemini\" | jq '.results[0]'\n  file=\"$(ask-codex-sessions -o ./responses -C /path/to/repo -t 90 'firebase orchestration interface')\"\n  jq '.results[0]' \"$file\"\n  ask-codex-sessions bm25llm-recent -s -a \"what was the latest spec for the interface\"\n  ask-codex-sessions llm -d \"find discussions about simplifying the interface\""
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

const DEFAULT_SUBCOMMAND: &str = "bm25llm";
const DEFAULT_SINCE_DAYS: &str = "30";

#[derive(Debug, Clone, Args, PartialEq, Eq)]
pub struct QueryArgs {
    #[arg(
        value_name = "QUERY",
        help = "Natural-language question or topic to search for in prior sessions"
    )]
    pub query: String,

    #[arg(short = 'd', long, help = "Print pipeline stages, plans, and ranking details to stderr [default: disabled]")]
    pub debug: bool,

    #[arg(short = 's', long = "sum", help = "Add summary fields to the JSON output artifact [default: disabled]")]
    pub sum: bool,

    #[arg(
        short = 'a',
        long = "answer",
        help = "Add a top-level answer to the original query in the JSON output artifact [default: disabled on explicit modes; enabled when no mode is given]"
    )]
    pub answer: bool,

    #[arg(short = 'C', long, value_name = "PATH", help = "Restrict search to sessions from this working directory or repo path [default: current working directory]")]
    pub cwd: Option<PathBuf>,

    #[arg(short = 't', long, value_name = "DAYS", help = "Only search sessions from the last N days [default: unset on explicit modes; 30 when no mode is given]")]
    pub since_days: Option<u32>,

    #[arg(short = 'l', long, default_value_t = 5, help = "Maximum number of ranked results to return")]
    pub limit: usize,

    #[arg(short = 'o', long = "out-dir", value_name = "PATH", help = "Write the JSON artifact into this directory and print only the file path [default: unset, so JSON is printed to stdout]")]
    pub out_dir: Option<PathBuf>,
}

pub fn parse_cli() -> Cli {
    Cli::parse_from(preprocess_args(std::env::args_os()))
}

pub fn try_parse_cli_from<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    Cli::try_parse_from(preprocess_args(args))
}

fn preprocess_args<I, T>(args: I) -> Vec<OsString>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    if args.len() <= 1 {
        return args;
    }

    let first = args
        .get(1)
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if matches!(
        first,
        "bm25llm" | "bm25llm-recent" | "bm25" | "llm" | "help" | "-h" | "--help"
    ) {
        return args;
    }

    let mut rewritten = Vec::with_capacity(args.len() + 4);
    rewritten.push(args[0].clone());
    rewritten.push(OsString::from(DEFAULT_SUBCOMMAND));

    if !has_flag(&args[1..], ["-a", "--answer"].as_slice()) {
        rewritten.push(OsString::from("-a"));
    }

    if !has_flag_with_value(&args[1..], ["-t", "--since-days"].as_slice()) {
        rewritten.push(OsString::from("--since-days"));
        rewritten.push(OsString::from(DEFAULT_SINCE_DAYS));
    }

    rewritten.extend_from_slice(&args[1..]);
    rewritten
}

fn has_flag(args: &[OsString], names: &[&str]) -> bool {
    args.iter()
        .any(|arg| arg.to_str().is_some_and(|value| names.contains(&value)))
}

fn has_flag_with_value(args: &[OsString], names: &[&str]) -> bool {
    args.iter().any(|arg| {
        arg.to_str().is_some_and(|value| {
            names.contains(&value)
                || names
                    .iter()
                    .any(|name| value.starts_with(&format!("{name}=")))
        })
    })
}

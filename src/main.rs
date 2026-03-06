use ask_codex_sessions::cli::{Cli, Command};
use ask_codex_sessions::config::Config;
use ask_codex_sessions::gemini::GeminiClient;
use ask_codex_sessions::output::render_output;
use ask_codex_sessions::search::SearchPipeline;
use ask_codex_sessions::types::{QueryPreset, SearchRequest};
use clap::Parser;
use std::path::PathBuf;
use time::OffsetDateTime;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::default();
    let gemini = GeminiClient::new(config.gemini_model.clone());
    let pipeline = SearchPipeline::new(config.clone(), gemini);

    let request = match cli.command {
        Command::Search(args) => SearchRequest {
            query: args.query,
            preset: QueryPreset::Search,
            cwd_filter: Some(resolve_cwd(args.cwd)?),
            timeframe_start: since_days_to_unix(args.since_days)?,
            limit: args.limit,
        },
        Command::LatestSpec(args) => SearchRequest {
            query: args.query,
            preset: QueryPreset::LatestSpec,
            cwd_filter: Some(resolve_cwd(args.cwd)?),
            timeframe_start: since_days_to_unix(args.since_days)?,
            limit: args.limit,
        },
    };

    let results = pipeline.search(&request)?;
    println!("{}", render_output(&request.query, &results)?);
    Ok(())
}

fn resolve_cwd(cwd: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    match cwd {
        Some(cwd) => Ok(cwd),
        None => Ok(std::env::current_dir()?),
    }
}

fn since_days_to_unix(days: Option<u32>) -> anyhow::Result<Option<i64>> {
    let Some(days) = days else {
        return Ok(None);
    };
    let now = OffsetDateTime::now_utc().unix_timestamp();
    Ok(Some(now - i64::from(days) * 86_400))
}

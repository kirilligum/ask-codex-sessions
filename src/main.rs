use ask_codex_sessions::cli::{Cli, Command};
use ask_codex_sessions::config::Config;
use ask_codex_sessions::debug::DebugEvents;
use ask_codex_sessions::gemini::GeminiClient;
use ask_codex_sessions::output::{build_output_artifact, write_output_artifact};
use ask_codex_sessions::search::SearchPipeline;
use ask_codex_sessions::types::{QueryPreset, SearchMode, SearchRequest};
use clap::Parser;
use std::path::PathBuf;
use time::OffsetDateTime;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::default();
    let (preset, mode, args) = match cli.command {
        Command::Bm25llm(args) => (QueryPreset::Search, SearchMode::Hybrid, args),
        Command::Bm25llmRecent(args) => (QueryPreset::LatestSpec, SearchMode::Hybrid, args),
        Command::Bm25(args) => (QueryPreset::Search, SearchMode::Lexical, args),
        Command::Llm(args) => (QueryPreset::Search, SearchMode::Llm, args),
    };
    let debug = if args.debug {
        DebugEvents::enabled()
    } else {
        DebugEvents::disabled()
    };
    let gemini = GeminiClient::new(config.gemini_model.clone()).with_debug(debug.clone());
    let pipeline = SearchPipeline::new(config.clone(), gemini).with_debug(debug);

    let request = SearchRequest {
        query: args.query,
        preset,
        mode,
        cwd_filter: Some(resolve_cwd(args.cwd)?),
        timeframe_start: since_days_to_unix(args.since_days)?,
        limit: args.limit,
    };

    let results = pipeline.search(&request)?;
    let synthesis_client = GeminiClient::new(config.gemini_model.clone());
    let summaries = if args.sum {
        Some(synthesis_client.summarize_results(&request.query, &results)?)
    } else {
        None
    };
    let answer = if args.answer {
        Some(synthesis_client.answer_query(&request.query, &results)?)
    } else {
        None
    };
    let artifact = build_output_artifact(
        &request,
        &results,
        summaries.as_ref(),
        answer.as_deref(),
        OffsetDateTime::now_utc(),
    )?;
    let output_path = write_output_artifact(&std::env::current_dir()?, &artifact)?;
    println!("{}", output_path.display());
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

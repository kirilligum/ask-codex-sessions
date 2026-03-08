use crate::types::{QueryPreset, ResultSummary, SearchMode, SearchRequest, SearchResult, SummaryBundle};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use time::format_description::well_known::Iso8601;
use time::macros::format_description;
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub word_count: usize,
    pub entity_count: usize,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactResult {
    pub rank: usize,
    pub session_id: String,
    pub thread_id: String,
    pub resume_command: String,
    pub session_path: PathBuf,
    pub text_id: String,
    pub source_start_line: usize,
    pub source_end_line: usize,
    pub title: String,
    pub created_at: i64,
    pub created_at_iso: String,
    pub quote: String,
    pub summary: Option<String>,
    pub score: crate::types::ScoreDetails,
    pub metadata: ArtifactMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputArtifact {
    pub query: String,
    pub preset: QueryPreset,
    pub mode: SearchMode,
    pub created_at: String,
    pub cwd_filter: Option<PathBuf>,
    pub timeframe_start: Option<i64>,
    pub result_count: usize,
    pub summary: Option<String>,
    pub answer: Option<String>,
    pub results: Vec<ArtifactResult>,
}

pub fn build_output_artifact(
    request: &SearchRequest,
    results: &[SearchResult],
    summaries: Option<&SummaryBundle>,
    answer: Option<&str>,
    now: OffsetDateTime,
) -> Result<OutputArtifact> {
    let result_summaries = summaries
        .map(|bundle| {
            bundle
                .result_summaries
                .iter()
                .map(|summary| (summary.text_id.as_str(), summary.summary.as_str()))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    let artifact_results = results
        .iter()
        .enumerate()
        .map(|(index, result)| {
            Ok(ArtifactResult {
                rank: index + 1,
                session_id: result.session_id.clone(),
                thread_id: result.thread_id.clone(),
                resume_command: format!("codex resume {}", result.session_id),
                session_path: result.rollout_path.clone(),
                text_id: result.chunk_id.clone(),
                source_start_line: result.source_start_line,
                source_end_line: result.source_end_line,
                title: result.title.clone(),
                created_at: result.created_at,
                created_at_iso: format_timestamp(result.created_at)?,
                quote: result.snippet.clone(),
                summary: result_summaries
                    .get(result.chunk_id.as_str())
                    .map(|summary| summary.to_string()),
                score: result.score.clone(),
                metadata: ArtifactMetadata {
                    word_count: result.word_count,
                    entity_count: result.entity_count,
                    matched_terms: result.matched_terms.clone(),
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(OutputArtifact {
        query: request.query.clone(),
        preset: request.preset,
        mode: request.mode,
        created_at: now.format(&Iso8601::DATE_TIME_OFFSET)?,
        cwd_filter: request.cwd_filter.clone(),
        timeframe_start: request.timeframe_start,
        result_count: artifact_results.len(),
        summary: summaries.map(|bundle| bundle.overall_summary.clone()),
        answer: answer.map(ToOwned::to_owned),
        results: artifact_results,
    })
}

pub fn write_output_artifact(base_dir: &Path, artifact: &OutputArtifact) -> Result<PathBuf> {
    let output_dir = base_dir.join("ask-codex-session-responses");
    write_output_artifact_in_dir(&output_dir, artifact)
}

pub fn write_output_artifact_in_dir(output_dir: &Path, artifact: &OutputArtifact) -> Result<PathBuf> {
    fs::create_dir_all(&output_dir)?;
    let filename = format!(
        "{}-{:?}-{:?}-{}.json",
        OffsetDateTime::now_utc().format(&format_description!(
            "[year][month][day]T[hour][minute][second]Z"
        ))?,
        artifact.preset,
        artifact.mode,
        slugify(&artifact.query),
    )
    .replace(':', "")
    .replace('+', "-")
    .replace('?', "");
    let path = output_dir.join(filename);
    let json = render_output_artifact(artifact)?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn render_output_artifact(artifact: &OutputArtifact) -> Result<String> {
    Ok(serde_json::to_string_pretty(artifact)?)
}

fn slugify(query: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in query.chars().flat_map(|ch| ch.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').chars().take(64).collect()
}

fn format_timestamp(timestamp: i64) -> Result<String> {
    let datetime = OffsetDateTime::from_unix_timestamp(timestamp)?;
    Ok(datetime.format(&Iso8601::DATE_TIME_OFFSET)?)
}

#[allow(dead_code)]
fn _result_summary_map(summaries: &[ResultSummary]) -> HashMap<&str, &str> {
    summaries
        .iter()
        .map(|summary| (summary.text_id.as_str(), summary.summary.as_str()))
        .collect()
}

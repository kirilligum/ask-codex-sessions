use crate::debug::DebugEvents;
use crate::types::{QueryPlan, QueryPreset, SearchCandidate, SearchResult, SummaryBundle};
use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct GeminiClient {
    model: String,
    mock_dir: Option<PathBuf>,
    debug: DebugEvents,
}

#[derive(Debug, Deserialize)]
struct CliEnvelope {
    response: String,
}

#[derive(Debug, Deserialize)]
struct RerankResponse {
    ordered_indexes: Vec<usize>,
}

#[derive(Debug, Deserialize)]
struct AnswerResponse {
    answer: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MockRerankResponse {
    Ordered { ordered_indexes: Vec<usize> },
    Preferred { preferred_terms: Vec<String> },
}

#[derive(Debug, Serialize)]
struct RerankCandidate<'a> {
    index: usize,
    thread_id: &'a str,
    chunk_id: &'a str,
    text: &'a str,
}

impl GeminiClient {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            mock_dir: None,
            debug: DebugEvents::disabled(),
        }
    }

    pub fn with_mock_dir(model: impl Into<String>, mock_dir: impl Into<PathBuf>) -> Self {
        Self {
            model: model.into(),
            mock_dir: Some(mock_dir.into()),
            debug: DebugEvents::disabled(),
        }
    }

    pub fn with_debug(mut self, debug: DebugEvents) -> Self {
        self.debug = debug;
        self
    }

    pub fn generate_query_plan(
        &self,
        query: &str,
        observed_terms: &[String],
        preset: QueryPreset,
    ) -> Result<QueryPlan> {
        self.debug.log(format!(
            "gemini query-plan start preset={preset:?} observed_terms={} query_chars={}",
            observed_terms.len(),
            query.len()
        ));
        let prompt = format!(
            "Return JSON only with keys keywords and phrases.\n\
             Query preset: {:?}\n\
             User query: {}\n\
             Allowed observed terms: {}\n\
             Rules:\n\
             - keywords must use only words from the user query or the allowed observed terms\n\
             - include 4 to 10 keywords\n\
             - include 0 to 3 phrases from the query when useful\n\
             - do not include explanations\n\
             - do not use markdown fences",
            preset,
            query,
            observed_terms.join(", "),
        );
        let mut plan: QueryPlan = self.run_json("query_plan.json", &prompt)?;
        sanitize_query_plan(&mut plan);
        validate_query_plan(query, observed_terms, &plan)?;
        self.debug.log(format!(
            "gemini query-plan result keywords={:?} phrases={:?}",
            plan.keywords, plan.phrases
        ));
        Ok(plan)
    }

    pub fn rerank(&self, query: &str, candidates: &[SearchCandidate]) -> Result<Vec<usize>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        self.debug.log(format!(
            "gemini rerank start candidates={} query_chars={}",
            candidates.len(),
            query.len()
        ));

        if let Some(mock_dir) = &self.mock_dir {
            let content = fs::read_to_string(mock_dir.join("rerank.json"))
                .with_context(|| format!("failed to read Gemini mock fixture {}", mock_dir.join("rerank.json").display()))?;
            let response: MockRerankResponse = serde_json::from_str(&content)?;
            let ordered_indexes = match response {
                MockRerankResponse::Ordered { ordered_indexes } => ordered_indexes,
                MockRerankResponse::Preferred { preferred_terms } => {
                    let preferred_terms = preferred_terms
                        .into_iter()
                        .map(|term| term.to_ascii_lowercase())
                        .collect::<Vec<_>>();
                    let mut scored = candidates
                        .iter()
                        .enumerate()
                        .map(|(index, candidate)| {
                            let lowered = candidate.chunk.dialogue_text.to_ascii_lowercase();
                            let score = preferred_terms
                                .iter()
                                .map(|term| lowered.matches(term).count())
                                .sum::<usize>();
                            (index, score)
                        })
                        .collect::<Vec<_>>();
                    scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
                    scored.into_iter().map(|(index, _)| index).collect()
                }
            };
            self.debug
                .log(format!("gemini rerank result ordered_indexes={ordered_indexes:?}"));
            return Ok(ordered_indexes);
        }

        let prompt = format!(
            "Return JSON only with key ordered_indexes.\n\
             Query: {}\n\
             Candidates: {}\n\
             Do not use markdown fences.",
            query,
            serde_json::to_string(
                &candidates
                    .iter()
                    .enumerate()
                    .map(|(index, candidate)| RerankCandidate {
                        index,
                        thread_id: &candidate.chunk.thread_id,
                        chunk_id: &candidate.chunk.chunk_id,
                        text: &candidate.chunk.dialogue_text,
                    })
                    .collect::<Vec<_>>(),
            )?,
        );

        let response: RerankResponse = self.run_json("rerank.json", &prompt)?;
        self.debug
            .log(format!("gemini rerank result ordered_indexes={:?}", response.ordered_indexes));
        Ok(response.ordered_indexes)
    }

    pub fn summarize_results(&self, query: &str, results: &[SearchResult]) -> Result<SummaryBundle> {
        if results.is_empty() {
            return Ok(SummaryBundle {
                overall_summary: String::new(),
                result_summaries: Vec::new(),
            });
        }
        self.debug.log(format!(
            "gemini summarize start results={} query_chars={}",
            results.len(),
            query.len()
        ));
        let prompt = format!(
            "Return JSON only with keys overall_summary and result_summaries.\n\
             Query: {}\n\
             Results: {}\n\
             Rules:\n\
             - result_summaries must be an array of objects with keys text_id and summary\n\
             - each summary should be one or two concise sentences\n\
             - overall_summary should be concise\n\
             - do not use markdown fences",
            query,
            serde_json::to_string(results)?,
        );
        let summary: SummaryBundle = self.run_json("summaries.json", &prompt)?;
        self.debug.log(format!(
            "gemini summarize result overall_summary_chars={} result_summaries={}",
            summary.overall_summary.len(),
            summary.result_summaries.len()
        ));
        Ok(summary)
    }

    pub fn answer_query(&self, query: &str, results: &[SearchResult]) -> Result<String> {
        if results.is_empty() {
            return Ok(String::new());
        }
        self.debug.log(format!(
            "gemini answer start results={} query_chars={}",
            results.len(),
            query.len()
        ));
        let prompt = format!(
            "Return JSON only with key answer.\n\
             Query: {}\n\
             Results: {}\n\
             Rules:\n\
             - answer the original query directly using only the supplied results\n\
             - keep the answer concise\n\
             - do not use markdown fences",
            query,
            serde_json::to_string(results)?,
        );
        let answer: AnswerResponse = self.run_json("answer.json", &prompt)?;
        self.debug
            .log(format!("gemini answer result chars={}", answer.answer.len()));
        Ok(answer.answer)
    }

    fn run_json<T: DeserializeOwned>(&self, mock_file: &str, prompt: &str) -> Result<T> {
        if let Some(mock_dir) = &self.mock_dir {
            self.debug
                .log(format!("gemini mock read fixture={}", mock_dir.join(mock_file).display()));
            let content = fs::read_to_string(mock_dir.join(mock_file))
                .with_context(|| format!("failed to read Gemini mock fixture {}", mock_dir.join(mock_file).display()))?;
            return Ok(serde_json::from_str(&content)?);
        }

        let started = Instant::now();
        self.debug.log(format!(
            "gemini cli exec model={} prompt_bytes={}",
            self.model,
            prompt.len()
        ));
        let output = Command::new("gemini")
            .arg("-m")
            .arg(&self.model)
            .arg("-p")
            .arg(prompt)
            .arg("-o")
            .arg("json")
            .output()
            .context("failed to execute gemini CLI")?;

        if !output.status.success() {
            return Err(anyhow!(
                "gemini CLI failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        let stdout = String::from_utf8(output.stdout).context("gemini stdout was not valid UTF-8")?;
        let start = stdout.find('{').ok_or_else(|| anyhow!("gemini output did not contain JSON"))?;
        let envelope: CliEnvelope = serde_json::from_str(&stdout[start..])
            .context("failed to parse Gemini CLI envelope")?;
        self.debug.log(format!(
            "gemini cli done model={} elapsed_ms={}",
            self.model,
            started.elapsed().as_millis()
        ));
        parse_json_fragment(&envelope.response).context("failed to parse Gemini JSON payload")
    }
}

fn sanitize_query_plan(plan: &mut QueryPlan) {
    plan.keywords = plan
        .keywords
        .iter()
        .map(|term| term.trim().to_string())
        .filter(|term| !term.is_empty())
        .collect();
    plan.phrases = plan
        .phrases
        .iter()
        .map(|term| term.trim().to_string())
        .filter(|term| !term.is_empty())
        .collect();
}

fn validate_query_plan(query: &str, observed_terms: &[String], plan: &QueryPlan) -> Result<()> {
    let original_terms = tokenize(query);
    let observed_terms = observed_terms
        .iter()
        .flat_map(|term| tokenize(term))
        .collect::<HashSet<_>>();

    for keyword in &plan.keywords {
        for token in tokenize(keyword) {
            if !original_terms.contains(&token) && !observed_terms.contains(&token) {
                return Err(anyhow!("query plan used unobserved term: {token}"));
            }
        }
    }

    Ok(())
}

fn tokenize(text: &str) -> HashSet<String> {
    text.split(|char: char| !char.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 3)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

pub fn default_mock_dir(root: &Path) -> PathBuf {
    root.join("tests/fixtures/gemini")
}

fn parse_json_fragment<T: DeserializeOwned>(text: &str) -> Result<T> {
    let trimmed = text.trim();
    if let Ok(parsed) = serde_json::from_str(trimmed) {
        return Ok(parsed);
    }

    let stripped = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|value| value.trim())
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    if let Ok(parsed) = serde_json::from_str(stripped) {
        return Ok(parsed);
    }

    for (open, close) in [('{', '}'), ('[', ']')] {
        if let (Some(start), Some(end)) = (stripped.find(open), stripped.rfind(close)) {
            if end > start {
                let fragment = &stripped[start..=end];
                if let Ok(parsed) = serde_json::from_str(fragment) {
                    return Ok(parsed);
                }
            }
        }
    }

    Err(anyhow!("no JSON fragment found"))
}

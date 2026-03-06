use crate::config::Config;
use crate::gemini::GeminiClient;
use crate::index::SearchIndex;
use crate::normalize::normalize_thread;
use crate::source::{filter_threads, load_threads};
use crate::types::{QueryPlan, QueryPreset, SearchCandidate, SearchRequest, SearchResult, ThreadMeta};
use anyhow::Result;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub struct SearchPipeline {
    config: Config,
    gemini: GeminiClient,
}

impl SearchPipeline {
    pub fn new(config: Config, gemini: GeminiClient) -> Self {
        Self { config, gemini }
    }

    pub fn search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>> {
        let threads = load_threads(&self.config.state_db_path)?;
        let filtered_threads = filter_threads(
            &threads,
            request.cwd_filter.as_deref(),
            request.timeframe_start,
        );

        let mut chunks = Vec::new();
        let mut thread_lookup = HashMap::new();
        for thread in filtered_threads {
            let thread_chunks = normalize_thread(&thread)?;
            if !thread_chunks.is_empty() {
                thread_lookup.insert(thread.thread_id.clone(), thread.clone());
                chunks.extend(thread_chunks);
            }
        }

        let index = SearchIndex::build(&thread_lookup.values().cloned().collect::<Vec<_>>(), &chunks)?;
        let observed_terms = observed_terms(&request.query, &chunks, 96);
        let plan = self
            .gemini
            .generate_query_plan(&request.query, &observed_terms, request.preset)?;

        let keyword_hits = keyword_bonus_map(&plan);
        let sqlite_query = build_fts_query(&plan);
        let latest_spec = matches!(request.preset, QueryPreset::LatestSpec);
        let mut candidates = index.search(
            &sqlite_query,
            &keyword_hits,
            &plan.phrases,
            latest_spec,
            self.config.candidate_limit.max(request.limit),
        )?;
        candidates.truncate(self.config.candidate_limit);

        let ordered_indexes = self.gemini.rerank(&request.query, &candidates)?;
        let reranked = apply_rerank(candidates, &ordered_indexes);
        Ok(materialize_results(reranked, &thread_lookup, &plan, request.limit))
    }
}

fn apply_rerank(candidates: Vec<SearchCandidate>, ordered_indexes: &[usize]) -> Vec<SearchCandidate> {
    let mut by_index = candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| (index, candidate))
        .collect::<HashMap<_, _>>();
    let mut ordered = Vec::new();
    for index in ordered_indexes {
        if let Some(candidate) = by_index.remove(index) {
            ordered.push(candidate);
        }
    }
    let mut rest = by_index.into_iter().collect::<Vec<_>>();
    rest.sort_by_key(|(index, _)| *index);
    ordered.extend(rest.into_iter().map(|(_, candidate)| candidate));
    ordered
}

fn materialize_results(
    candidates: Vec<SearchCandidate>,
    thread_lookup: &HashMap<String, ThreadMeta>,
    plan: &QueryPlan,
    limit: usize,
) -> Vec<SearchResult> {
    let mut seen_threads = HashSet::new();
    candidates
        .into_iter()
        .filter_map(|candidate| {
            let thread = thread_lookup.get(&candidate.chunk.thread_id)?;
            if !seen_threads.insert(thread.thread_id.clone()) {
                return None;
            }
            Some(SearchResult {
                thread_id: thread.thread_id.clone(),
                title: thread.title.clone(),
                created_at: thread.created_at,
                rollout_path: thread.rollout_path.clone(),
                chunk_id: candidate.chunk.chunk_id.clone(),
                score: candidate.score,
                snippet: build_snippet(&candidate.chunk.dialogue_text, plan),
            })
        })
        .take(limit)
        .collect()
}

fn build_snippet(text: &str, plan: &QueryPlan) -> String {
    let compact = text.replace('\n', " ");
    let trimmed = compact.trim();
    let lowered = trimmed.to_ascii_lowercase();

    for phrase in &plan.phrases {
        if let Some(index) = lowered.rfind(&phrase.to_ascii_lowercase()) {
            return slice_snippet(trimmed, index, phrase.len());
        }
    }

    for keyword in &plan.keywords {
        if let Some(index) = lowered.rfind(&keyword.to_ascii_lowercase()) {
            return slice_snippet(trimmed, index, keyword.len());
        }
    }

    slice_snippet(trimmed, 0, 0)
}

fn slice_snippet(text: &str, anchor: usize, anchor_len: usize) -> String {
    if text.len() <= 260 {
        return text.to_string();
    }

    let start = anchor.saturating_sub(90);
    let end = (anchor + anchor_len + 170).min(text.len());
    let mut snippet = text[start..end].trim().to_string();
    if start > 0 {
        snippet.insert_str(0, "...");
    }
    if end < text.len() {
        snippet.push_str("...");
    }
    snippet
}

fn keyword_bonus_map(plan: &QueryPlan) -> HashMap<String, bool> {
    let phrases = plan
        .phrases
        .iter()
        .map(|phrase| phrase.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    plan.keywords
        .iter()
        .map(|keyword| {
            let lowered = keyword.to_ascii_lowercase();
            let exact_only = phrases.contains(&lowered) || looks_technical(keyword);
            (lowered, exact_only)
        })
        .collect()
}

fn looks_technical(value: &str) -> bool {
    value.contains('/')
        || value.contains('_')
        || value.contains('-')
        || value.contains('.')
        || value.chars().any(|char| char.is_ascii_digit())
        || value.chars().skip(1).any(|char| char.is_ascii_uppercase())
}

fn observed_terms(query: &str, chunks: &[crate::types::Chunk], limit: usize) -> Vec<String> {
    let query_terms = tokenize(query);
    let mut scores = HashMap::<String, (usize, usize, usize)>::new();

    for chunk in chunks {
        let text = format!("{} {}", chunk.dialogue_text, chunk.entity_text);
        let chunk_terms = token_re()
            .find_iter(&text)
            .map(|token| token.as_str().to_ascii_lowercase())
            .filter(|value| value.len() >= 3 && !STOPWORDS.contains(&value.as_str()))
            .collect::<HashSet<_>>();
        let chunk_overlap = chunk_terms
            .iter()
            .filter(|term| query_terms.contains(*term))
            .count();

        for term in chunk_terms {
            let entry = scores.entry(term.clone()).or_default();
            entry.0 += 1;
            if looks_technical(&term) {
                entry.1 += 1;
            }
            if chunk_overlap > 0 {
                entry.2 += chunk_overlap;
            }
        }
    }

    let mut terms = scores.into_iter().collect::<Vec<_>>();
    terms.sort_by(|left, right| {
        right
            .1
            .2
            .cmp(&left.1.2)
            .then_with(|| right.1.1.cmp(&left.1.1))
            .then_with(|| right.1.0.cmp(&left.1.0))
            .then_with(|| left.0.cmp(&right.0))
    });
    terms.into_iter().take(limit).map(|(term, _)| term).collect()
}

fn build_fts_query(plan: &QueryPlan) -> String {
    let mut parts = Vec::new();
    for phrase in &plan.phrases {
        parts.push(format!("\"{}\"", phrase.replace('"', " ")));
    }
    for keyword in &plan.keywords {
        parts.push(escape_fts_token(keyword));
    }
    parts.join(" OR ")
}

fn escape_fts_token(token: &str) -> String {
    if token.chars().all(|char| char.is_ascii_alphanumeric() || char == '_') {
        token.to_ascii_lowercase()
    } else {
        format!("\"{}\"", token.replace('"', " "))
    }
}

fn token_re() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"[A-Za-z0-9_./:-]{3,}").expect("valid regex"))
}

fn tokenize(text: &str) -> HashSet<String> {
    token_re()
        .find_iter(text)
        .map(|token| token.as_str().to_ascii_lowercase())
        .filter(|token| token.len() >= 3 && !STOPWORDS.contains(&token.as_str()))
        .collect()
}

const STOPWORDS: &[&str] = &[
    "the", "and", "that", "with", "this", "from", "have", "into", "your", "what", "were",
    "would", "there", "they", "them", "then", "than", "when", "where", "which", "using",
    "also", "just", "like", "does", "should", "about", "here", "because", "want", "need",
    "for", "are", "you", "not", "can", "only", "user", "use", "but", "one", "all", "how",
    "still", "while", "both", "each", "those", "will", "must", "too", "much", "more", "most",
    "into", "their", "them", "than", "being", "been", "such", "already", "very", "over",
    "does", "did", "our", "out", "why", "who", "whose", "where", "these", "some", "same",
];

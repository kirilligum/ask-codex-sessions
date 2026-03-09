use crate::config::Config;
use crate::debug::DebugEvents;
use crate::gemini::GeminiClient;
use crate::index::SearchIndex;
use crate::normalize::normalize_thread_with_stats;
use crate::source::{filter_threads, load_threads};
use crate::types::{
    Chunk, QueryPlan, QueryPreset, ScoreDetails, SearchCandidate, SearchMode, SearchRequest,
    SearchResult, ThreadMeta,
};
use anyhow::Result;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::Instant;

pub struct SearchPipeline {
    config: Config,
    gemini: GeminiClient,
    debug: DebugEvents,
}

impl SearchPipeline {
    pub fn new(config: Config, gemini: GeminiClient) -> Self {
        Self {
            config,
            gemini,
            debug: DebugEvents::disabled(),
        }
    }

    pub fn with_debug(mut self, debug: DebugEvents) -> Self {
        self.debug = debug;
        self
    }

    pub fn search(&self, request: &SearchRequest) -> Result<Vec<SearchResult>> {
        self.debug.log(format!(
            "search start preset={:?} mode={:?} query={:?} cwd_filter={} since={:?} limit={}",
            request.preset,
            request.mode,
            request.query,
            request
                .cwd_filter
                .as_deref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            request.timeframe_start,
            request.limit
        ));
        let load_started = Instant::now();
        let threads = load_threads(&self.config.state_db_path)?;
        self.debug.log(format!(
            "loaded {} threads from {} in {}ms",
            threads.len(),
            self.config.state_db_path.display(),
            load_started.elapsed().as_millis()
        ));
        let filtered_threads = filter_threads(
            &threads,
            request.cwd_filter.as_deref(),
            request.timeframe_start,
        );
        self.debug.log(format!(
            "filtered to {} threads after cwd/time constraints",
            filtered_threads.len()
        ));

        let normalize_started = Instant::now();
        let (chunks, thread_lookup) = normalize_threads(&self.debug, filtered_threads);
        self.debug.log(format!(
            "normalized {} chunks across {} threads in {}ms",
            chunks.len(),
            thread_lookup.len(),
            normalize_started.elapsed().as_millis()
        ));
        if chunks.is_empty() {
            self.debug
                .log("no valid chunks remained after normalization; returning no results");
            return Ok(Vec::new());
        }

        let latest_spec = matches!(request.preset, QueryPreset::LatestSpec);
        let results = match request.mode {
            SearchMode::Hybrid => self.hybrid_search(request, &chunks, &thread_lookup, latest_spec)?,
            SearchMode::Lexical => self.lexical_search(request, &chunks, &thread_lookup, latest_spec)?,
            SearchMode::Llm => self.llm_search(request, &chunks, &thread_lookup, latest_spec)?,
        };
        self.debug
            .log(format!("search done results={}", results.len()));
        Ok(results)
    }

    fn hybrid_search(
        &self,
        request: &SearchRequest,
        chunks: &[Chunk],
        thread_lookup: &HashMap<String, ThreadMeta>,
        latest_spec: bool,
    ) -> Result<Vec<SearchResult>> {
        let index = build_index(&self.debug, thread_lookup, chunks)?;
        let observed_terms = observed_terms(&request.query, chunks, 96);
        let observed_preview = observed_terms
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        self.debug.log(format!(
            "observed terms count={} preview=[{}]",
            observed_terms.len(),
            observed_preview
        ));

        let plan = self
            .gemini
            .generate_query_plan(&request.query, &observed_terms, request.preset)?;
        let keyword_hits = keyword_bonus_map(&plan);
        let sqlite_query = build_fts_query(&plan);
        self.debug.log(format!("fts query={sqlite_query}"));

        let search_started = Instant::now();
        let candidates = index.search(
            &sqlite_query,
            &keyword_hits,
            &plan.phrases,
            latest_spec,
            effective_result_limit(request.limit, chunks.len()),
        )?;
        self.debug.log(format!(
            "fts search produced {} candidates in {}ms",
            candidates.len(),
            search_started.elapsed().as_millis()
        ));
        log_candidate_preview(&self.debug, &candidates);

        let ordered_indexes = self.gemini.rerank(&request.query, &candidates)?;
        let reranked = apply_rerank(candidates, &ordered_indexes);
        Ok(materialize_results(reranked, thread_lookup, &plan, request.limit))
    }

    fn lexical_search(
        &self,
        request: &SearchRequest,
        chunks: &[Chunk],
        thread_lookup: &HashMap<String, ThreadMeta>,
        latest_spec: bool,
    ) -> Result<Vec<SearchResult>> {
        let plan = lexical_query_plan(&request.query, chunks);
        self.debug.log(format!(
            "lexical query-plan keywords={:?} phrases={:?}",
            plan.keywords, plan.phrases
        ));
        let index = build_index(&self.debug, thread_lookup, chunks)?;
        let keyword_hits = keyword_bonus_map(&plan);
        let sqlite_query = build_fts_query(&plan);
        self.debug.log(format!("fts query={sqlite_query}"));

        let search_started = Instant::now();
        let candidates = index.search(
            &sqlite_query,
            &keyword_hits,
            &plan.phrases,
            latest_spec,
            effective_result_limit(request.limit, chunks.len()),
        )?;
        let mut candidates = candidates;
        apply_query_echo_penalty(&mut candidates, &request.query);
        self.debug.log(format!(
            "fts search produced {} candidates in {}ms",
            candidates.len(),
            search_started.elapsed().as_millis()
        ));
        log_candidate_preview(&self.debug, &candidates);
        Ok(materialize_results(candidates, thread_lookup, &plan, request.limit))
    }

    fn llm_search(
        &self,
        request: &SearchRequest,
        chunks: &[Chunk],
        thread_lookup: &HashMap<String, ThreadMeta>,
        latest_spec: bool,
    ) -> Result<Vec<SearchResult>> {
        let plan = lexical_query_plan(&request.query, chunks);
        self.debug.log(format!(
            "llm-search query-plan keywords={:?} phrases={:?}",
            plan.keywords, plan.phrases
        ));
        let candidates = llm_rank_all_chunks(
            &self.gemini,
            &self.debug,
            &request.query,
            chunks,
            latest_spec,
            self.config.rerank_limit.max(1),
        )?;
        log_candidate_preview(&self.debug, &candidates);
        Ok(materialize_results(candidates, thread_lookup, &plan, request.limit))
    }
}

fn build_index(
    debug: &DebugEvents,
    thread_lookup: &HashMap<String, ThreadMeta>,
    chunks: &[Chunk],
) -> Result<SearchIndex> {
    let index_started = Instant::now();
    let index = SearchIndex::build(&thread_lookup.values().cloned().collect::<Vec<_>>(), chunks)?;
    debug.log(format!(
        "built in-memory search index for {} chunks in {}ms",
        chunks.len(),
        index_started.elapsed().as_millis()
    ));
    Ok(index)
}

fn apply_query_echo_penalty(candidates: &mut [SearchCandidate], query: &str) {
    let normalized_query = normalize_whitespace(query);
    if normalized_query.is_empty() {
        return;
    }

    for candidate in candidates.iter_mut() {
        let normalized_dialogue = normalize_whitespace(&candidate.chunk.dialogue_text);
        if !normalized_dialogue.contains(&normalized_query) {
            continue;
        }

        let penalty = if candidate.chunk.dialogue_text.contains(&format!("\"{query}\""))
            || candidate.chunk.dialogue_text.contains(&format!("`{query}`"))
        {
            4.0
        } else {
            2.0
        };
        candidate.score.noise_penalty += penalty;
        candidate.score.final_score -= penalty;
    }

    candidates.sort_by(|left, right| right.score.final_score.total_cmp(&left.score.final_score));
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn normalize_threads(
    debug: &DebugEvents,
    filtered_threads: Vec<ThreadMeta>,
) -> (Vec<Chunk>, HashMap<String, ThreadMeta>) {
    let mut chunks = Vec::new();
    let mut thread_lookup = HashMap::new();
    let mut invalid_line_total = 0usize;
    let mut invalid_line_threads = Vec::new();
    let mut setup_only_threads = Vec::new();
    let mut normalization_failures = Vec::new();
    for thread in filtered_threads {
        match normalize_thread_with_stats(&thread) {
            Ok(normalized) => {
                if normalized.stats.skipped_invalid_lines > 0 {
                    invalid_line_total += normalized.stats.skipped_invalid_lines;
                    invalid_line_threads.push((
                        thread.thread_id.clone(),
                        thread.rollout_path.display().to_string(),
                        normalized.stats.skipped_invalid_lines,
                    ));
                }
                if !normalized.chunks.is_empty() {
                    thread_lookup.insert(thread.thread_id.clone(), thread.clone());
                    chunks.extend(normalized.chunks);
                } else {
                    setup_only_threads.push((
                        thread.thread_id.clone(),
                        thread.rollout_path.display().to_string(),
                    ));
                }
            }
            Err(error) => {
                normalization_failures.push((
                    thread.thread_id.clone(),
                    thread.rollout_path.display().to_string(),
                    format!("{error:#}"),
                ));
            }
        }
    }

    if invalid_line_total > 0 {
        debug.log(format!(
            "skipped {} invalid rollout line(s) across {} thread(s): {}",
            invalid_line_total,
            invalid_line_threads.len(),
            summarize_invalid_line_examples(&invalid_line_threads)
        ));
    }

    if !setup_only_threads.is_empty() {
        debug.log(format!(
            "skipped {} thread(s) with only setup boilerplate or unsupported content: {}",
            setup_only_threads.len(),
            summarize_thread_examples(&setup_only_threads)
        ));
    }

    if !normalization_failures.is_empty() {
        debug.log(format!(
            "skipped {} thread(s) because rollout normalization failed: {}",
            normalization_failures.len(),
            summarize_failed_examples(&normalization_failures)
        ));
    }

    (chunks, thread_lookup)
}

fn summarize_invalid_line_examples(entries: &[(String, String, usize)]) -> String {
    let preview = entries
        .iter()
        .take(3)
        .map(|(thread_id, path, count)| format!("{thread_id} ({count} in {path})"))
        .collect::<Vec<_>>()
        .join(", ");
    let remaining = entries.len().saturating_sub(3);
    if remaining == 0 {
        preview
    } else {
        format!("{preview}, +{remaining} more")
    }
}

fn summarize_thread_examples(entries: &[(String, String)]) -> String {
    let preview = entries
        .iter()
        .take(3)
        .map(|(thread_id, path)| format!("{thread_id} ({path})"))
        .collect::<Vec<_>>()
        .join(", ");
    let remaining = entries.len().saturating_sub(3);
    if remaining == 0 {
        preview
    } else {
        format!("{preview}, +{remaining} more")
    }
}

fn summarize_failed_examples(entries: &[(String, String, String)]) -> String {
    let preview = entries
        .iter()
        .take(3)
        .map(|(thread_id, path, error)| format!("{thread_id} ({path}: {error})"))
        .collect::<Vec<_>>()
        .join(", ");
    let remaining = entries.len().saturating_sub(3);
    if remaining == 0 {
        preview
    } else {
        format!("{preview}, +{remaining} more")
    }
}

fn llm_rank_all_chunks(
    gemini: &GeminiClient,
    debug: &DebugEvents,
    query: &str,
    chunks: &[Chunk],
    latest_spec: bool,
    batch_size: usize,
) -> Result<Vec<SearchCandidate>> {
    if chunks.is_empty() {
        return Ok(Vec::new());
    }

    let batch_size = batch_size.max(1);
    let mut newest = i64::MIN;
    let mut oldest = i64::MAX;
    for chunk in chunks {
        newest = newest.max(chunk.created_at);
        oldest = oldest.min(chunk.created_at);
    }

    let mut candidates = Vec::new();
    for (batch_index, batch) in chunks.chunks(batch_size).enumerate() {
        debug.log(format!(
            "llm-search batch {}/{} size={}",
            batch_index + 1,
            chunks.len().div_ceil(batch_size),
            batch.len()
        ));
        let batch_candidates = batch
            .iter()
            .cloned()
            .map(|chunk| SearchCandidate {
                chunk,
                score: ScoreDetails::default(),
            })
            .collect::<Vec<_>>();
        let ordered_indexes = gemini.rerank(query, &batch_candidates)?;
        let rank_map = ordered_indexes
            .into_iter()
            .enumerate()
            .map(|(rank, index)| (index, rank))
            .collect::<HashMap<_, _>>();

        for (index, chunk) in batch.iter().enumerate() {
            let rank = rank_map.get(&index).copied().unwrap_or(batch.len());
            let mut final_score = (batch.len().saturating_sub(rank)) as f64;
            let recency_scale = if newest == oldest {
                1.0
            } else {
                (chunk.created_at - oldest) as f64 / (newest - oldest) as f64
            };
            let recency_bonus = if latest_spec {
                recency_scale * 1.5
            } else {
                recency_scale * 0.35
            };
            final_score += recency_bonus;
            candidates.push(SearchCandidate {
                chunk: chunk.clone(),
                score: ScoreDetails {
                    final_score,
                    bm25_raw: None,
                    phrase_matches: 0,
                    entity_matches: 0,
                    dialogue_matches: 0,
                    recency_bonus,
                    noise_penalty: 0.0,
                    llm_rerank_position: Some(rank),
                    llm_batch_index: Some(batch_index),
                    llm_batch_rank: Some(rank),
                },
            });
        }
    }

    candidates.sort_by(|left, right| right.score.final_score.total_cmp(&left.score.final_score));
    Ok(candidates)
}

fn log_candidate_preview(debug: &DebugEvents, candidates: &[SearchCandidate]) {
    let candidate_preview = candidates
        .iter()
        .take(5)
        .map(|candidate| {
            format!(
                "{}:{} score={:.3}",
                candidate.chunk.thread_id, candidate.chunk.ordinal, candidate.score.final_score
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    if !candidate_preview.is_empty() {
        debug.log(format!("candidate preview=[{candidate_preview}]"));
    }
}

fn apply_rerank(candidates: Vec<SearchCandidate>, ordered_indexes: &[usize]) -> Vec<SearchCandidate> {
    let mut by_index = candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| (index, candidate))
        .collect::<HashMap<_, _>>();
    let mut ordered = Vec::new();
    for (rank, index) in ordered_indexes.iter().enumerate() {
        if let Some(mut candidate) = by_index.remove(index) {
            candidate.score.llm_rerank_position = Some(rank);
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
    let results = candidates
        .into_iter()
        .filter_map(|candidate| {
            let thread = thread_lookup.get(&candidate.chunk.thread_id)?;
            if !seen_threads.insert(thread.thread_id.clone()) {
                return None;
            }
            let matched_terms = matched_terms_for_chunk(&candidate.chunk, plan);
            let word_count = count_words(&candidate.chunk.dialogue_text);
            let entity_count = count_words(&candidate.chunk.entity_text);
            Some(SearchResult {
                session_id: thread.thread_id.clone(),
                thread_id: thread.thread_id.clone(),
                title: thread.title.clone(),
                created_at: thread.created_at,
                rollout_path: thread.rollout_path.clone(),
                chunk_id: candidate.chunk.chunk_id.clone(),
                source_start_line: candidate.chunk.source_start_line,
                source_end_line: candidate.chunk.source_end_line,
                score: candidate.score,
                snippet: build_snippet(&candidate.chunk.dialogue_text, plan),
                matched_terms,
                word_count,
                entity_count,
            })
        })
        .collect::<Vec<_>>();

    if limit == 0 {
        results
    } else {
        results.into_iter().take(limit).collect()
    }
}

fn effective_result_limit(request_limit: usize, chunk_count: usize) -> usize {
    if request_limit == 0 {
        chunk_count.max(1)
    } else {
        request_limit.max(1)
    }
}

fn matched_terms_for_chunk(chunk: &Chunk, plan: &QueryPlan) -> Vec<String> {
    let lower_dialogue = chunk.dialogue_text.to_ascii_lowercase();
    let lower_entity = chunk.entity_text.to_ascii_lowercase();
    let mut matches = Vec::new();

    for phrase in &plan.phrases {
        let phrase_lower = phrase.to_ascii_lowercase();
        if lower_dialogue.contains(&phrase_lower) || lower_entity.contains(&phrase_lower) {
            matches.push(phrase.clone());
        }
    }

    for keyword in &plan.keywords {
        let keyword_lower = keyword.to_ascii_lowercase();
        if (lower_dialogue.contains(&keyword_lower) || lower_entity.contains(&keyword_lower))
            && !matches.iter().any(|existing| existing.eq_ignore_ascii_case(keyword))
        {
            matches.push(keyword.clone());
        }
    }

    matches
}

fn build_snippet(text: &str, plan: &QueryPlan) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let lowered = compact.to_ascii_lowercase();

    for phrase in &plan.phrases {
        if let Some(index) = lowered.rfind(&phrase.to_ascii_lowercase()) {
            return slice_snippet(&compact, index, phrase.len());
        }
    }

    for keyword in &plan.keywords {
        if let Some(index) = lowered.rfind(&keyword.to_ascii_lowercase()) {
            return slice_snippet(&compact, index, keyword.len());
        }
    }

    slice_snippet(&compact, 0, 0)
}

fn slice_snippet(text: &str, anchor: usize, anchor_len: usize) -> String {
    let spans = non_space_re()
        .find_iter(text)
        .map(|entry| (entry.start(), entry.end()))
        .collect::<Vec<_>>();
    if spans.is_empty() {
        return String::new();
    }
    if spans.len() <= 54 {
        return text.to_string();
    }

    let anchor_end = (anchor + anchor_len).min(text.len());
    let anchor_start_word = spans
        .iter()
        .position(|(_, end)| *end > anchor)
        .unwrap_or(0);
    let anchor_end_word = spans
        .iter()
        .rposition(|(start, _)| *start < anchor_end)
        .unwrap_or(anchor_start_word);

    let start_word = anchor_start_word.saturating_sub(18);
    let end_word = (anchor_end_word + 36).min(spans.len() - 1);
    let start = spans[start_word].0;
    let end = spans[end_word].1;
    let mut snippet = text[start..end].trim().to_string();
    if start > 0 {
        snippet.insert_str(0, "... ");
    }
    if end < text.len() {
        snippet.push_str(" ...");
    }
    snippet
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
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

fn observed_terms(query: &str, chunks: &[Chunk], limit: usize) -> Vec<String> {
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

fn lexical_query_plan(query: &str, chunks: &[Chunk]) -> QueryPlan {
    let ordered_terms = ordered_query_terms(query);
    let primary_terms = select_primary_query_terms(&ordered_terms, chunks, 4);
    let expansions = lexical_expansion_terms(&ordered_terms, chunks, 4);
    let mut keywords = Vec::new();
    let mut seen = HashSet::new();

    for term in primary_terms.into_iter().chain(expansions) {
        if seen.insert(term.clone()) {
            keywords.push(term);
        }
    }

    let mut phrases = Vec::new();
    if keywords.len() >= 2 {
        phrases.push(format!("{} {}", keywords[0], keywords[1]));
    }

    QueryPlan { keywords, phrases }
}

fn select_primary_query_terms(
    ordered_terms: &[String],
    chunks: &[Chunk],
    limit: usize,
) -> Vec<String> {
    let mut scored = ordered_terms
        .iter()
        .enumerate()
        .map(|(index, term)| (index, term.clone(), chunk_document_frequency(term, chunks)))
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| left.2.cmp(&right.2).then_with(|| left.0.cmp(&right.0)));
    let mut kept = scored
        .into_iter()
        .take(limit.max(1))
        .collect::<Vec<_>>();
    kept.sort_by_key(|entry| entry.0);
    kept.into_iter().map(|(_, term, _)| term).collect()
}

fn lexical_expansion_terms(
    query_terms: &[String],
    chunks: &[Chunk],
    limit: usize,
) -> Vec<String> {
    let query_set = query_terms.iter().cloned().collect::<HashSet<_>>();
    let mut document_frequency = HashMap::<String, usize>::new();
    let mut cooccurrence = HashMap::<String, usize>::new();

    for chunk in chunks {
        let overlap_terms = chunk_overlap_terms(chunk);
        let overlap = overlap_terms
            .iter()
            .filter(|term| query_set.contains(*term))
            .count();
        let chunk_terms = chunk_search_terms(chunk);

        for term in &chunk_terms {
            *document_frequency.entry(term.clone()).or_default() += 1;
            if overlap > 0 && !query_set.contains(term) {
                *cooccurrence.entry(term.clone()).or_default() += overlap;
            }
        }
    }

    let mut ranked = cooccurrence
        .into_iter()
        .map(|(term, overlap)| {
            let df = *document_frequency.get(&term).unwrap_or(&usize::MAX);
            (term, overlap, df)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| {
                let left_technical = looks_technical(&left.0);
                let right_technical = looks_technical(&right.0);
                right_technical.cmp(&left_technical)
            })
            .then_with(|| left.0.cmp(&right.0))
    });

    ranked
        .into_iter()
        .filter(|(term, _, _)| !STOPWORDS.contains(&term.as_str()))
        .map(|(term, _, _)| term)
        .take(limit)
        .collect()
}

fn chunk_document_frequency(term: &str, chunks: &[Chunk]) -> usize {
    chunks
        .iter()
        .filter(|chunk| chunk_overlap_terms(chunk).contains(term))
        .count()
}

fn chunk_search_terms(chunk: &Chunk) -> HashSet<String> {
    token_re()
        .find_iter(&format!("{} {}", chunk.assistant_text, chunk.entity_text))
        .map(|token| token.as_str().to_ascii_lowercase())
        .filter(|value| value.len() >= 3 && !STOPWORDS.contains(&value.as_str()))
        .collect()
}

fn chunk_overlap_terms(chunk: &Chunk) -> HashSet<String> {
    token_re()
        .find_iter(&chunk.dialogue_text)
        .map(|token| token.as_str().to_ascii_lowercase())
        .filter(|value| value.len() >= 3 && !STOPWORDS.contains(&value.as_str()))
        .collect()
}

fn ordered_query_terms(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut terms = Vec::new();
    for token in token_re().find_iter(query) {
        let token = token.as_str().to_ascii_lowercase();
        if token.len() < 3 || STOPWORDS.contains(&token.as_str()) {
            continue;
        }
        if seen.insert(token.clone()) {
            terms.push(token);
        }
    }
    terms
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

fn non_space_re() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\S+").expect("valid regex"))
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

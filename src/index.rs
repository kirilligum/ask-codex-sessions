use crate::types::{Chunk, ScoreDetails, SearchCandidate, ThreadMeta};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::collections::HashMap;

pub struct SearchIndex {
    connection: Connection,
}

impl SearchIndex {
    pub fn build(threads: &[ThreadMeta], chunks: &[Chunk]) -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(
            "
            CREATE TABLE sessions (
                thread_id TEXT PRIMARY KEY,
                created_at INTEGER NOT NULL,
                cwd TEXT NOT NULL,
                title TEXT NOT NULL,
                rollout_path TEXT NOT NULL,
                git_branch TEXT,
                git_origin_url TEXT
            );
            CREATE TABLE chunks (
                chunk_id TEXT PRIMARY KEY,
                thread_id TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                source_start_line INTEGER NOT NULL,
                source_end_line INTEGER NOT NULL,
                user_text TEXT NOT NULL,
                assistant_text TEXT NOT NULL,
                dialogue_text TEXT NOT NULL,
                entity_text TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE VIRTUAL TABLE fts_chunks USING fts5(
                chunk_id UNINDEXED,
                user_text,
                assistant_text,
                entity_text,
                tokenize = 'unicode61'
            );
            ",
        )?;

        let transaction = connection.unchecked_transaction()?;
        {
            let mut insert_session = transaction.prepare(
                "INSERT INTO sessions
                (thread_id, created_at, cwd, title, rollout_path, git_branch, git_origin_url)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for thread in threads {
                insert_session.execute(params![
                    thread.thread_id,
                    thread.created_at,
                    thread.cwd.to_string_lossy(),
                    thread.title,
                    thread.rollout_path.to_string_lossy(),
                    thread.git_branch,
                    thread.git_origin_url,
                ])?;
            }

            let mut insert_chunk = transaction.prepare(
                "INSERT INTO chunks
                (chunk_id, thread_id, ordinal, source_start_line, source_end_line, user_text, assistant_text, dialogue_text, entity_text, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            let mut insert_fts = transaction.prepare(
                "INSERT INTO fts_chunks (chunk_id, user_text, assistant_text, entity_text)
                VALUES (?1, ?2, ?3, ?4)",
            )?;

            for chunk in chunks {
                insert_chunk.execute(params![
                    chunk.chunk_id,
                    chunk.thread_id,
                    chunk.ordinal as i64,
                    chunk.source_start_line as i64,
                    chunk.source_end_line as i64,
                    chunk.user_text,
                    chunk.assistant_text,
                    chunk.dialogue_text,
                    chunk.entity_text,
                    chunk.created_at,
                ])?;
                insert_fts.execute(params![
                    chunk.chunk_id,
                    chunk.user_text,
                    chunk.assistant_text,
                    chunk.entity_text
                ])?;
            }
        }
        transaction.commit()?;

        Ok(Self { connection })
    }

    pub fn table_names(&self) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "SELECT name FROM sqlite_master
             WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut table_names = Vec::new();
        for row in rows {
            table_names.push(row?);
        }
        Ok(table_names)
    }

    pub fn search(
        &self,
        query: &str,
        keyword_hits: &HashMap<String, bool>,
        phrase_hits: &[String],
        latest_spec: bool,
        limit: usize,
    ) -> Result<Vec<SearchCandidate>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                c.chunk_id,
                c.thread_id,
                c.ordinal,
                c.source_start_line,
                c.source_end_line,
                c.user_text,
                c.assistant_text,
                c.dialogue_text,
                c.entity_text,
                c.created_at,
                bm25(fts_chunks, 0.0, 1.0, 1.3) AS score
            FROM fts_chunks
            JOIN chunks c ON c.chunk_id = fts_chunks.chunk_id
            WHERE fts_chunks MATCH ?1
            ORDER BY score
            LIMIT ?2
            ",
        )?;

        let rows = statement.query_map([query, &limit.to_string()], |row| {
            let chunk = Chunk {
                chunk_id: row.get(0)?,
                thread_id: row.get(1)?,
                ordinal: row.get::<_, i64>(2)? as usize,
                source_start_line: row.get::<_, i64>(3)? as usize,
                source_end_line: row.get::<_, i64>(4)? as usize,
                user_text: row.get(5)?,
                assistant_text: row.get(6)?,
                dialogue_text: row.get(7)?,
                entity_text: row.get(8)?,
                created_at: row.get(9)?,
            };
            let bm25: f64 = row.get(10)?;
            Ok((chunk, bm25))
        })?;

        let mut candidates = Vec::new();
        let mut newest = i64::MIN;
        let mut oldest = i64::MAX;

        let mut raw_rows = Vec::new();
        for row in rows {
            let (chunk, bm25) = row?;
            newest = newest.max(chunk.created_at);
            oldest = oldest.min(chunk.created_at);
            raw_rows.push((chunk, bm25));
        }

        for (chunk, bm25) in raw_rows {
            let lower_user = chunk.user_text.to_lowercase();
            let lower_assistant = chunk.assistant_text.to_lowercase();
            let lower_entity = chunk.entity_text.to_lowercase();
            let mut final_score = -bm25;
            let mut phrase_matches = 0usize;
            let mut entity_matches = 0usize;
            let mut dialogue_matches = 0usize;

            for phrase in phrase_hits {
                let lower_phrase = phrase.to_lowercase();
                if lower_assistant.contains(&lower_phrase) || lower_entity.contains(&lower_phrase) {
                    final_score += 2.0;
                    phrase_matches += 1;
                } else if lower_user.contains(&lower_phrase) {
                    final_score += 0.15;
                    phrase_matches += 1;
                }
            }

            for (keyword, exact_only) in keyword_hits {
                let lower_keyword = keyword.to_lowercase();
                if lower_entity.contains(&lower_keyword) {
                    if *exact_only {
                        final_score += 1.25;
                    } else {
                        final_score += 0.15;
                    }
                    entity_matches += 1;
                } else if !exact_only && lower_assistant.contains(&lower_keyword) {
                    final_score += 0.35;
                    dialogue_matches += 1;
                } else if !exact_only && lower_user.contains(&lower_keyword) {
                    final_score += 0.05;
                    dialogue_matches += 1;
                }
            }

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
            let noise_penalty =
                transcript_penalty(&chunk.dialogue_text) + structured_dump_penalty(&chunk.dialogue_text, &chunk.entity_text);
            final_score -= noise_penalty;
            candidates.push(SearchCandidate {
                chunk,
                score: ScoreDetails {
                    final_score,
                    bm25_raw: Some(bm25),
                    phrase_matches,
                    entity_matches,
                    dialogue_matches,
                    recency_bonus,
                    noise_penalty,
                    llm_rerank_position: None,
                    llm_batch_index: None,
                    llm_batch_rank: None,
                },
            });
        }

        candidates.sort_by(|left, right| right.score.final_score.total_cmp(&left.score.final_score));
        Ok(candidates)
    }
}

fn transcript_penalty(text: &str) -> f64 {
    let lower = text.to_ascii_lowercase();
    let mut penalty = 0.0;

    if lower.contains("error: unrecognized subcommand") {
        penalty += 3.0;
    }
    if lower.contains("usage: ask-codex-sessions") {
        penalty += 2.0;
    }
    if lower.contains("cargo run --") {
        penalty += 1.5;
    }
    if lower.contains("executed in") {
        penalty += 1.0;
    }
    if lower.contains("kirill@") {
        penalty += 1.0;
    }

    penalty
}

fn structured_dump_penalty(dialogue_text: &str, entity_text: &str) -> f64 {
    let mut penalty = 0.0;
    let key_value_markers = dialogue_text.matches("\":").count();
    let brace_markers = dialogue_text.matches('{').count() + dialogue_text.matches('}').count();
    let entity_terms = entity_text.split_whitespace().count();

    if key_value_markers >= 8 {
        penalty += 3.0;
    }
    if brace_markers >= 6 {
        penalty += 1.5;
    }
    if entity_terms > 150 {
        penalty += 2.0;
    } else if entity_terms > 80 {
        penalty += 1.0;
    }

    penalty
}

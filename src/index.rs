use crate::types::{Chunk, SearchCandidate, ThreadMeta};
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
                dialogue_text TEXT NOT NULL,
                entity_text TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE VIRTUAL TABLE fts_chunks USING fts5(
                chunk_id UNINDEXED,
                dialogue_text,
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
                "INSERT INTO chunks (chunk_id, thread_id, ordinal, dialogue_text, entity_text, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let mut insert_fts = transaction.prepare(
                "INSERT INTO fts_chunks (chunk_id, dialogue_text, entity_text)
                VALUES (?1, ?2, ?3)",
            )?;

            for chunk in chunks {
                insert_chunk.execute(params![
                    chunk.chunk_id,
                    chunk.thread_id,
                    chunk.ordinal as i64,
                    chunk.dialogue_text,
                    chunk.entity_text,
                    chunk.created_at,
                ])?;
                insert_fts.execute(params![chunk.chunk_id, chunk.dialogue_text, chunk.entity_text])?;
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
            SELECT c.chunk_id, c.thread_id, c.ordinal, c.dialogue_text, c.entity_text, c.created_at, bm25(fts_chunks) AS score
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
                dialogue_text: row.get(3)?,
                entity_text: row.get(4)?,
                created_at: row.get(5)?,
            };
            let bm25: f64 = row.get(6)?;
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
            let lower_dialogue = chunk.dialogue_text.to_lowercase();
            let lower_entity = chunk.entity_text.to_lowercase();
            let mut score = -bm25;

            for phrase in phrase_hits {
                let lower_phrase = phrase.to_lowercase();
                if lower_dialogue.contains(&lower_phrase) || lower_entity.contains(&lower_phrase) {
                    score += 2.0;
                }
            }

            for (keyword, exact_only) in keyword_hits {
                let lower_keyword = keyword.to_lowercase();
                if lower_entity.contains(&lower_keyword) {
                    score += 1.25;
                } else if !exact_only && lower_dialogue.contains(&lower_keyword) {
                    score += 0.35;
                }
            }

            let recency_scale = if newest == oldest {
                1.0
            } else {
                (chunk.created_at - oldest) as f64 / (newest - oldest) as f64
            };
            score += if latest_spec { recency_scale * 1.5 } else { recency_scale * 0.35 };
            candidates.push(SearchCandidate { chunk, score });
        }

        candidates.sort_by(|left, right| right.score.total_cmp(&left.score));
        Ok(candidates)
    }
}

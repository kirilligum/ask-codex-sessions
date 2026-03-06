use crate::types::ThreadMeta;
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

pub fn load_threads(db_path: &Path) -> Result<Vec<ThreadMeta>> {
    let connection = Connection::open(db_path)
        .with_context(|| format!("failed to open thread database at {}", db_path.display()))?;
    let mut statement = connection.prepare(
        "SELECT id, rollout_path, created_at, cwd, title, git_branch, git_origin_url
         FROM threads
         ORDER BY created_at DESC, id DESC",
    )?;

    let rows = statement.query_map([], |row| {
        Ok(ThreadMeta {
            thread_id: row.get(0)?,
            rollout_path: row.get::<_, String>(1)?.into(),
            created_at: row.get(2)?,
            cwd: row.get::<_, String>(3)?.into(),
            title: row.get(4)?,
            git_branch: row.get(5)?,
            git_origin_url: row.get(6)?,
        })
    })?;

    let mut threads = Vec::new();
    for row in rows {
        threads.push(row?);
    }
    Ok(threads)
}

pub fn filter_threads(
    threads: &[ThreadMeta],
    cwd_filter: Option<&Path>,
    timeframe_start: Option<i64>,
) -> Vec<ThreadMeta> {
    threads
        .iter()
        .filter(|thread| {
            let cwd_ok = cwd_filter.is_none_or(|cwd| thread.cwd == cwd);
            let time_ok = timeframe_start.is_none_or(|start| thread.created_at >= start);
            cwd_ok && time_ok
        })
        .cloned()
        .collect()
}

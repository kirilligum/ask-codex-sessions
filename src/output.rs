use crate::types::SearchResult;
use anyhow::Result;
use time::format_description::well_known::Iso8601;
use time::OffsetDateTime;

pub fn render_output(query: &str, results: &[SearchResult]) -> Result<String> {
    let mut lines = Vec::new();
    lines.push(format!("Query: {query}"));
    lines.push(String::new());

    for (index, result) in results.iter().enumerate() {
        lines.push(format!(
            "{}. {} ({})",
            index + 1,
            result.title,
            format_date(result.created_at)?
        ));
        lines.push(format!("   Thread: {}", result.thread_id));
        lines.push(format!("   Path: {}", result.rollout_path.display()));
        lines.push(format!("   Quote: {}", result.snippet));
        lines.push(String::new());
    }

    lines.push("Handoff:".to_string());
    lines.push(render_handoff_block(results)?);
    Ok(lines.join("\n"))
}

pub fn render_handoff_block(results: &[SearchResult]) -> Result<String> {
    let mut lines = Vec::new();
    lines.push("Use these cited prior-session findings:".to_string());
    for result in results {
        lines.push(format!(
            "- [{}] {} {}",
            format_date(result.created_at)?,
            result.thread_id,
            result.rollout_path.display()
        ));
        lines.push(format!("  Quote: {}", result.snippet));
    }
    Ok(lines.join("\n"))
}

fn format_date(timestamp: i64) -> Result<String> {
    let datetime = OffsetDateTime::from_unix_timestamp(timestamp)?;
    Ok(datetime.date().format(&Iso8601::DATE)?)
}

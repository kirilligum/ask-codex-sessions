use crate::types::{Chunk, ThreadMeta};
use anyhow::{Context, Result};
use regex::Regex;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::OnceLock;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizeStats {
    pub skipped_invalid_lines: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizedThread {
    pub chunks: Vec<Chunk>,
    pub stats: NormalizeStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Speaker {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
struct VisibleMessage {
    speaker: Speaker,
    text: String,
    line_number: usize,
}

pub fn normalize_thread(thread: &ThreadMeta) -> Result<Vec<Chunk>> {
    Ok(normalize_thread_with_stats(thread)?.chunks)
}

pub fn normalize_thread_with_stats(thread: &ThreadMeta) -> Result<NormalizedThread> {
    let file = File::open(&thread.rollout_path)
        .with_context(|| format!("failed to open rollout file {}", thread.rollout_path.display()))?;
    let reader = BufReader::new(file);

    let mut messages = Vec::new();
    let mut stats = NormalizeStats::default();
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        let candidate = line.trim_matches('\0').trim();
        if candidate.is_empty() {
            stats.skipped_invalid_lines += 1;
            continue;
        }
        let value: Value = match serde_json::from_str(candidate) {
            Ok(value) => value,
            Err(_) => {
                stats.skipped_invalid_lines += 1;
                continue;
            }
        };
        if let Some(message) = visible_message(&value, index + 1) {
            if !is_duplicate_message(messages.last(), &message) {
                messages.push(message);
            }
        }
    }

    Ok(NormalizedThread {
        chunks: build_chunks(thread, &messages)?,
        stats,
    })
}

fn visible_message(value: &Value, line_number: usize) -> Option<VisibleMessage> {
    match value.get("type")?.as_str()? {
        "response_item" => visible_response_item(value, line_number),
        "event_msg" => visible_event_message(value, line_number),
        _ => None,
    }
}

fn visible_response_item(value: &Value, line_number: usize) -> Option<VisibleMessage> {
    let payload = value.get("payload")?;
    if payload.get("type")?.as_str()? != "message" {
        return None;
    }

    let role = payload.get("role")?.as_str()?;
    let speaker = match role {
        "user" => Speaker::User,
        "assistant" => Speaker::Assistant,
        _ => return None,
    };
    if speaker == Speaker::Assistant && payload.get("phase").and_then(Value::as_str) == Some("commentary") {
        return None;
    }

    let mut parts = Vec::new();
    for item in payload.get("content")?.as_array()? {
        if let Some(text) = item.get("text").and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }
    }

    visible_message_from_text(speaker, parts.join("\n\n"), line_number)
}

fn visible_event_message(value: &Value, line_number: usize) -> Option<VisibleMessage> {
    let payload = value.get("payload")?;
    let payload_type = payload.get("type")?.as_str()?;
    match payload_type {
        "user_message" => {
            let text = payload.get("message")?.as_str()?.trim().to_string();
            visible_message_from_text(Speaker::User, text, line_number)
        }
        "agent_message" => {
            if payload.get("phase").and_then(Value::as_str) == Some("commentary") {
                return None;
            }
            let text = payload.get("message")?.as_str()?.trim().to_string();
            visible_message_from_text(Speaker::Assistant, text, line_number)
        }
        "task_complete" => {
            let text = payload
                .get("last_agent_message")
                .and_then(Value::as_str)?
                .trim()
                .to_string();
            visible_message_from_text(Speaker::Assistant, text, line_number)
        }
        _ => None,
    }
}

fn visible_message_from_text(
    speaker: Speaker,
    text: String,
    line_number: usize,
) -> Option<VisibleMessage> {
    if text.is_empty() || is_boilerplate(&text) {
        return None;
    }

    Some(VisibleMessage {
        speaker,
        text,
        line_number,
    })
}

fn is_duplicate_message(previous: Option<&VisibleMessage>, next: &VisibleMessage) -> bool {
    let Some(previous) = previous else {
        return false;
    };

    previous.speaker == next.speaker
        && normalize_message_text(&previous.text) == normalize_message_text(&next.text)
}

fn normalize_message_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn build_chunks(thread: &ThreadMeta, messages: &[VisibleMessage]) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();
    let mut current_user: Option<VisibleMessage> = None;
    let mut assistant_messages = Vec::new();

    let flush = |chunks: &mut Vec<Chunk>,
                 current_user: &mut Option<VisibleMessage>,
                 assistant_messages: &mut Vec<VisibleMessage>| {
        if let Some(user_message) = current_user.take() {
            let start_line = user_message.line_number;
            let end_line = assistant_messages
                .last()
                .map(|message| message.line_number)
                .unwrap_or(start_line);
            let user_text = user_message.text;
            let assistant_text = assistant_messages
                .iter()
                .map(|message| message.text.clone())
                .collect::<Vec<_>>()
                .join("\n\n");
            let mut dialogue_parts = vec![user_text.clone()];
            if !assistant_text.is_empty() {
                dialogue_parts.push(assistant_text.clone());
            }
            let dialogue_text = dialogue_parts.join("\n\n");
            if !dialogue_text.trim().is_empty() {
                let ordinal = chunks.len();
                chunks.push(Chunk {
                    chunk_id: format!("{}:{ordinal}", thread.thread_id),
                    thread_id: thread.thread_id.clone(),
                    ordinal,
                    source_start_line: start_line,
                    source_end_line: end_line,
                    user_text,
                    assistant_text,
                    entity_text: extract_entity_text(&dialogue_text),
                    dialogue_text,
                    created_at: thread.created_at,
                });
            }
            assistant_messages.clear();
        }
    };

    for message in messages {
        match message.speaker {
            Speaker::User => {
                flush(&mut chunks, &mut current_user, &mut assistant_messages);
                current_user = Some(message.clone());
            }
            Speaker::Assistant => {
                if current_user.is_some() {
                    assistant_messages.push(message.clone());
                }
            }
        }
    }

    flush(&mut chunks, &mut current_user, &mut assistant_messages);
    Ok(chunks)
}

fn is_boilerplate(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with("# AGENTS.md instructions for ")
        || trimmed.starts_with("<permissions instructions>")
        || trimmed.starts_with("<collaboration_mode>")
        || trimmed.contains("<environment_context>")
        || trimmed.starts_with("=== document:")
}

fn extract_entity_text(text: &str) -> String {
    let mut entities = BTreeSet::new();

    for capture in backtick_re().captures_iter(text) {
        if let Some(value) = capture.get(1).map(|entry| entry.as_str().trim()) {
            if is_entity_like(value) {
                entities.insert(value.to_string());
            }
        }
    }

    for found in path_re().find_iter(text) {
        entities.insert(found.as_str().to_string());
    }

    for found in technical_token_re().find_iter(text) {
        let value = found.as_str();
        if looks_technical(value) {
            entities.insert(value.to_string());
        }
    }

    entities.into_iter().collect::<Vec<_>>().join(" ")
}

fn is_entity_like(value: &str) -> bool {
    !value.is_empty()
        && (looks_technical(value)
            || value.contains("--")
            || value.contains("::")
            || value.starts_with("cargo ")
            || value.starts_with("codex ")
            || value.starts_with("gemini "))
}

fn looks_technical(value: &str) -> bool {
    value.contains('/')
        || value.contains('_')
        || value.contains('-')
        || value.contains('.')
        || value.chars().any(|char| char.is_ascii_digit())
        || value.chars().skip(1).any(|char| char.is_ascii_uppercase())
}

fn backtick_re() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"`([^`]+)`").expect("valid regex"))
}

fn path_re() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"/[A-Za-z0-9._/\-]+").expect("valid regex"))
}

fn technical_token_re() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b[A-Za-z0-9_./:-]{3,}\b").expect("valid regex"))
}

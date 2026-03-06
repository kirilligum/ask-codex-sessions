use crate::types::{Chunk, ThreadMeta};
use anyhow::{Context, Result};
use regex::Regex;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Speaker {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
struct VisibleMessage {
    speaker: Speaker,
    text: String,
}

pub fn normalize_thread(thread: &ThreadMeta) -> Result<Vec<Chunk>> {
    let file = File::open(&thread.rollout_path)
        .with_context(|| format!("failed to open rollout file {}", thread.rollout_path.display()))?;
    let reader = BufReader::new(file);

    let mut messages = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let value: Value = serde_json::from_str(&line)
            .with_context(|| format!("invalid rollout JSON in {}", thread.rollout_path.display()))?;
        if let Some(message) = visible_message(&value) {
            messages.push(message);
        }
    }

    build_chunks(thread, &messages)
}

fn visible_message(value: &Value) -> Option<VisibleMessage> {
    let payload = value.get("payload")?;
    if value.get("type")?.as_str()? != "response_item" || payload.get("type")?.as_str()? != "message" {
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

    if parts.is_empty() {
        return None;
    }

    let text = parts.join("\n\n");
    if is_boilerplate(&text) {
        return None;
    }

    Some(VisibleMessage { speaker, text })
}

fn build_chunks(thread: &ThreadMeta, messages: &[VisibleMessage]) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();
    let mut current_user: Option<String> = None;
    let mut assistant_messages = Vec::new();

    let flush = |chunks: &mut Vec<Chunk>,
                 current_user: &mut Option<String>,
                 assistant_messages: &mut Vec<String>| {
        if let Some(user_text) = current_user.take() {
            let mut dialogue_parts = vec![user_text];
            dialogue_parts.append(assistant_messages);
            let dialogue_text = dialogue_parts.join("\n\n");
            if !dialogue_text.trim().is_empty() {
                let ordinal = chunks.len();
                chunks.push(Chunk {
                    chunk_id: format!("{}:{ordinal}", thread.thread_id),
                    thread_id: thread.thread_id.clone(),
                    ordinal,
                    entity_text: extract_entity_text(&dialogue_text),
                    dialogue_text,
                    created_at: thread.created_at,
                });
            }
        }
    };

    for message in messages {
        match message.speaker {
            Speaker::User => {
                flush(&mut chunks, &mut current_user, &mut assistant_messages);
                current_user = Some(message.text.clone());
            }
            Speaker::Assistant => {
                if current_user.is_some() {
                    assistant_messages.push(message.text.clone());
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
            if !value.is_empty() {
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

//! "Enhance notes" with Claude. The user's Anthropic API key is kept in the
//! Windows Credential Manager, never in a file. Only text is sent: the
//! user's notes and the transcript, never audio.
//!
//! Rust has no official Anthropic SDK, so this calls the Messages API over
//! HTTP with streaming, and opts into server-side fallbacks so a request a
//! safety classifier declines is retried on another model instead of failing.

use crate::meetings::Meeting;
use std::io::{BufRead, BufReader};
use std::time::Duration;

const API: &str = "https://api.anthropic.com/v1";
const VERSION: &str = "2026-10-21";
const MODEL: &str = "claude-opus-5-5";

const SYSTEM: &str = "You write meeting notes. You get the user's own rough notes and a transcript of \
the meeting. Lines marked Me were said by the user; lines marked Them were said by the other \
participants (their names are not known unless someone says them).

Turn them into clear notes the user can share:
- The user's notes show what mattered to them. Keep every point they wrote, keep their order and \
structure where it makes sense, and fill in the details, numbers, names and decisions from the transcript.
- Add anything important from the transcript that the notes missed.
- Never invent facts, names, dates or commitments that are not in the transcript or the notes. If \
something in the transcript is unclear, say so briefly instead of guessing.
- The transcript comes from speech recognition and has mistakes; read past the obvious ones.

Write Markdown only, in this shape:
# A short title for the meeting
A summary of two or three sentences.
Then level-2 sections that fit this meeting, for example Key points, Decisions, Action items \
(written as \"- [ ] task (owner)\" when the owner is known) and Open questions. Leave out sections \
that would be empty.";

fn credential() -> Result<keyring::Entry, String> {
    keyring::Entry::new("Xnote", "anthropic-api-key").map_err(|e| e.to_string())
}

pub fn has_key() -> bool {
    credential().and_then(|c| c.get_password().map_err(|e| e.to_string())).is_ok()
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(300))
        .build()
}

/// The API's own error message, when it sent one.
fn api_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            let body: serde_json::Value = response.into_json().unwrap_or_default();
            let message = body["error"]["message"].as_str().unwrap_or("request failed");
            format!("{code}: {message}")
        }
        other => other.to_string(),
    }
}

/// Checks the key with a free call (listing models) before storing it.
pub fn set_key(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("empty key".into());
    }
    agent()
        .get(&format!("{API}/models?limit=1"))
        .set("x-api-key", key)
        .set("anthropic-version", VERSION)
        .call()
        .map_err(api_error)?;
    credential()?.set_password(key).map_err(|e| e.to_string())
}

pub fn clear_key() -> Result<(), String> {
    credential()?.delete_credential().map_err(|e| e.to_string())
}

fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

fn language_name(code: &str) -> &'static str {
    match code {
        "pt" => "Brazilian Portuguese",
        "es" => "Spanish",
        "en" => "English",
        _ => "the language most of the meeting was in",
    }
}

fn prompt(meeting: &Meeting, language: &str) -> String {
    let transcript: Vec<String> = meeting
        .transcript
        .iter()
        .map(|l| format!("[{}] {}: {}", clock(l.ms), if l.who == "me" { "Me" } else { "Them" }, l.text))
        .collect();
    let notes = if meeting.notes.trim().is_empty() { "(no notes)" } else { meeting.notes.trim() };
    format!(
        "<notes>\n{notes}\n</notes>\n\n<transcript>\n{}\n</transcript>\n\nWrite the notes in {}.",
        transcript.join("\n"),
        language_name(language)
    )
}

/// Streams the enhanced notes; `on_text` gets each piece as it arrives.
pub fn enhance(meeting: &Meeting, language: &str, on_text: &mut dyn FnMut(&str)) -> Result<String, String> {
    let key = credential()?.get_password().map_err(|_| "no API key".to_string())?;
    let body = serde_json::json!({
        "model": MODEL,
        "max_tokens": 32000,
        "stream": true,
        "fallbacks": "default",
        "output_config": { "effort": "medium" },
        "system": SYSTEM,
        "messages": [{ "role": "user", "content": prompt(meeting, language) }],
    });
    let response = agent()
        .post(&format!("{API}/messages"))
        .set("x-api-key", &key)
        .set("anthropic-version", VERSION)
        .set("anthropic-beta", "server-side-fallback-2026-07-01")
        .set("content-type", "application/json")
        .send_json(body)
        .map_err(api_error)?;

    read_stream(BufReader::new(response.into_reader()), on_text)
}

/// Reads the server-sent events of a streamed Messages response: collects
/// the text deltas and checks how the response ended.
fn read_stream(reader: impl BufRead, on_text: &mut dyn FnMut(&str)) -> Result<String, String> {
    let mut text = String::new();
    let mut stop_reason = String::new();
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Some(data) = line.strip_prefix("data: ") else { continue };
        let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else { continue };
        match event["type"].as_str() {
            Some("content_block_delta") if event["delta"]["type"] == "text_delta" => {
                let piece = event["delta"]["text"].as_str().unwrap_or_default();
                text.push_str(piece);
                on_text(piece);
            }
            Some("message_delta") => {
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    stop_reason = reason.to_string();
                }
            }
            Some("error") => {
                return Err(event["error"]["message"].as_str().unwrap_or("stream error").to_string());
            }
            _ => {}
        }
    }
    if stop_reason == "refusal" {
        return Err("refusal".into());
    }
    if text.trim().is_empty() {
        return Err("empty response".into());
    }
    Ok(text)
}

/// The "# Title" line of the enhanced notes, if any.
pub fn title_of(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meetings::Line;

    #[test]
    fn prompt_labels_speakers_and_times() {
        let meeting = Meeting {
            notes: "- budget".into(),
            transcript: vec![
                Line { ms: 65_000, who: "me".into(), text: "Let's close it Friday.".into() },
                Line { ms: 3_725_000, who: "them".into(), text: "Agreed.".into() },
            ],
            ..Default::default()
        };
        let p = prompt(&meeting, "pt");
        assert!(p.contains("[01:05] Me: Let's close it Friday."));
        assert!(p.contains("[1:02:05] Them: Agreed."));
        assert!(p.contains("Brazilian Portuguese"));
        assert!(p.contains("- budget"));
    }

    const STREAM: &str = "event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"# Sync\\n\"}}\n\n\
event: ping\n\
data: {\"type\":\"ping\"}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Done.\"}}\n\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n";

    #[test]
    fn reads_text_deltas_from_the_stream() {
        let mut pieces = Vec::new();
        let text = read_stream(STREAM.as_bytes(), &mut |p| pieces.push(p.to_string())).unwrap();
        assert_eq!(text, "# Sync\nDone.");
        assert_eq!(pieces.len(), 2);
    }

    #[test]
    fn reports_refusals_and_stream_errors() {
        let refusal = STREAM.replace("end_turn", "refusal");
        assert_eq!(read_stream(refusal.as_bytes(), &mut |_| {}).unwrap_err(), "refusal");
        let error = "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n";
        assert_eq!(read_stream(error.as_bytes(), &mut |_| {}).unwrap_err(), "Overloaded");
    }

    #[test]
    fn finds_the_title() {
        assert_eq!(title_of("# Sales sync\nSummary"), Some("Sales sync".into()));
        assert_eq!(title_of("No heading"), None);
    }
}

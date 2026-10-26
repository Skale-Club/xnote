//! "Enhance notes" with ChatGPT (the OpenAI API). It uses the same OpenAI key
//! as cloud transcription, kept in the Windows Credential Manager, never in a
//! file. Only text is sent: the user's notes and the transcript, never audio.
//!
//! Calls the Responses API over HTTP with streaming, so the notes appear as
//! they are written.

use crate::cloud;
use crate::meetings::Meeting;
use std::io::{BufRead, BufReader};
use std::time::Duration;

const API: &str = "https://api.openai.com/v1";
/// Near the flagship's quality at a fraction of its price; a one-hour
/// meeting costs about five cents.
const MODEL: &str = "gpt-6.1-sol";

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

fn openai() -> &'static cloud::Provider {
    cloud::provider("openai").expect("openai provider")
}

pub fn has_key() -> bool {
    cloud::has_key(openai())
}

/// Checks the key with a free call before storing it.
pub fn set_key(key: &str) -> Result<(), String> {
    cloud::set_key(openai(), key)
}

pub fn clear_key() -> Result<(), String> {
    cloud::clear_key(openai())
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
    let key = cloud::key_of(openai()).ok_or("no API key")?;
    let body = serde_json::json!({
        "model": MODEL,
        "instructions": SYSTEM,
        "input": prompt(meeting, language),
        "max_output_tokens": 16000,
        "stream": true,
        // Meetings can be private: don't keep the response on OpenAI's side.
        "store": false,
    });
    let response = agent()
        .post(&format!("{API}/responses"))
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(api_error)?;

    read_stream(BufReader::new(response.into_reader()), on_text)
}

/// Reads the server-sent events of a streamed response: collects the text
/// deltas and checks how the response ended.
fn read_stream(reader: impl BufRead, on_text: &mut dyn FnMut(&str)) -> Result<String, String> {
    let mut text = String::new();
    let mut refused = false;
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Some(data) = line.strip_prefix("data: ") else { continue };
        let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else { continue };
        match event["type"].as_str() {
            Some("response.output_text.delta") => {
                let piece = event["delta"].as_str().unwrap_or_default();
                text.push_str(piece);
                on_text(piece);
            }
            Some("response.refusal.delta") => refused = true,
            Some("response.incomplete") => {
                // Cut short by the length limit: keep what was written.
                if event["response"]["incomplete_details"]["reason"] == "content_filter" {
                    refused = true;
                }
            }
            Some("response.failed") => {
                let message = event["response"]["error"]["message"].as_str().unwrap_or("the request failed");
                return Err(message.to_string());
            }
            Some("error") => {
                let message = event["message"].as_str().or(event["error"]["message"].as_str());
                return Err(message.unwrap_or("stream error").to_string());
            }
            _ => {}
        }
    }
    if refused && text.trim().is_empty() {
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

    const STREAM: &str = "event: response.created\n\
data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"status\":\"in_progress\"}}\n\n\
event: response.output_text.delta\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"# Sync\\n\"}\n\n\
event: response.output_text.delta\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"Done.\"}\n\n\
event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\"}}\n";

    #[test]
    fn reads_text_deltas_from_the_stream() {
        let mut pieces = Vec::new();
        let text = read_stream(STREAM.as_bytes(), &mut |p| pieces.push(p.to_string())).unwrap();
        assert_eq!(text, "# Sync\nDone.");
        assert_eq!(pieces.len(), 2);
    }

    #[test]
    fn reports_refusals_and_errors() {
        let refusal = "data: {\"type\":\"response.refusal.delta\",\"delta\":\"I can't help with that.\"}\n";
        assert_eq!(read_stream(refusal.as_bytes(), &mut |_| {}).unwrap_err(), "refusal");
        let failed = "data: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\",\"error\":{\"code\":\"server_error\",\"message\":\"Overloaded\"}}}\n";
        assert_eq!(read_stream(failed.as_bytes(), &mut |_| {}).unwrap_err(), "Overloaded");
        let error = "data: {\"type\":\"error\",\"code\":\"rate_limit_exceeded\",\"message\":\"Slow down\"}\n";
        assert_eq!(read_stream(error.as_bytes(), &mut |_| {}).unwrap_err(), "Slow down");
    }

    #[test]
    fn keeps_text_cut_short_by_the_length_limit() {
        let cut = STREAM.replace(
            "event: response.completed\n",
            "data: {\"type\":\"response.incomplete\",\"response\":{\"status\":\"incomplete\",\"incomplete_details\":{\"reason\":\"max_output_tokens\"}}}\n\n",
        );
        assert_eq!(read_stream(cut.as_bytes(), &mut |_| {}).unwrap(), "# Sync\nDone.");
    }

    #[test]
    fn finds_the_title() {
        assert_eq!(title_of("# Sales sync\nSummary"), Some("Sales sync".into()));
        assert_eq!(title_of("No heading"), None);
    }
}

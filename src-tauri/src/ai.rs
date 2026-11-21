//! Meeting summaries ("enhanced notes"): the user's rough notes plus the
//! transcript become clear notes. Optional: Xnote records and transcribes
//! without any of this. Works with any provider in providers.rs, including
//! models that run on this computer (Ollama, LM Studio). Only text is sent,
//! never audio. The answer streams in, so the notes appear as they are written.

use crate::meetings::Meeting;
use crate::providers::{self, Api, Provider};
use std::io::{BufRead, BufReader};
use std::time::Duration;

pub(crate) const SYSTEM: &str = "You write meeting notes. You get the user's own rough notes and a transcript of \
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

fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

pub(crate) fn language_name(code: &str) -> &'static str {
    match code {
        "pt" => "Brazilian Portuguese",
        "es" => "Spanish",
        "en" => "English",
        _ => "the language most of the meeting was in",
    }
}

/// Extra guidance for each summary style; "general" adds none.
pub(crate) fn style_instructions(style: &str) -> &'static str {
    match style {
        "sales" => "This is a sales or client call. Add sections for the client's needs and pains, objections, budget and timing when mentioned, and clear next steps with owners and dates.",
        "one_on_one" => "This is a one-on-one. Add sections for how the person is doing, feedback given in both directions, growth or career topics, and agreed follow-ups.",
        "standup" => "This is a daily stand-up. Keep it very short: per person when names are known, what was done, what is next and any blockers.",
        "interview" => "This is an interview. Add sections for the candidate's background, strengths, concerns, answers to key questions and a neutral summary; do not make a hiring decision.",
        _ => "",
    }
}

pub(crate) const STYLES: [&str; 5] = ["general", "sales", "one_on_one", "standup", "interview"];

pub(crate) fn prompt(meeting: &Meeting, language: &str) -> String {
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
pub fn enhance(
    provider: &Provider,
    model: &str,
    meeting: &Meeting,
    language: &str,
    style: &str,
    on_text: &mut dyn FnMut(&str),
) -> Result<String, String> {
    if model.trim().is_empty() {
        return Err("no model".into());
    }
    let key = providers::request_key(provider)?;
    let mut input = prompt(meeting, language);
    let extra = style_instructions(style);
    if !extra.is_empty() {
        input = format!("{input}\n\n{extra}");
    }
    let (path, body) = match provider.api {
        Api::OpenAi => {
            let mut body = serde_json::json!({
                "model": model,
                "messages": [
                    { "role": "system", "content": SYSTEM },
                    { "role": "user", "content": input },
                ],
                "stream": true,
            });
            if provider.id == "openai" {
                // Disable stored response state; provider retention policies still apply.
                body["store"] = false.into();
            }
            ("chat/completions", body)
        }
        Api::Anthropic => (
            "messages",
            serde_json::json!({
                "model": model,
                "system": SYSTEM,
                "messages": [{ "role": "user", "content": input }],
                "max_tokens": 8192,
                "stream": true,
            }),
        ),
    };
    // Models on this computer can take a while before the first word.
    let agent = providers::agent(Duration::from_secs(if provider.local { 900 } else { 300 }));
    let url = format!("{}/{path}", provider.base_url);
    let response = providers::send_with_retry(provider, || {
        let request = agent.post(&url).set("Content-Type", "application/json");
        providers::authorize(provider, request, key.as_deref()).send_json(body.clone())
    })?;
    let reader = BufReader::new(response.into_reader());
    match provider.api {
        Api::OpenAi => read_chat_stream(reader, on_text),
        Api::Anthropic => read_anthropic_stream(reader, on_text),
    }
}

/// The JSON of each server-sent event's `data:` line.
fn events(reader: impl BufRead) -> impl Iterator<Item = Result<serde_json::Value, String>> {
    reader.lines().filter_map(|line| match line {
        Err(e) => Some(Err(e.to_string())),
        Ok(line) => {
            let data = line.strip_prefix("data:")?.trim();
            serde_json::from_str(data).ok().map(Ok)
        }
    })
}

fn finish(text: String, refused: bool) -> Result<String, String> {
    if refused && text.trim().is_empty() {
        return Err("refusal".into());
    }
    if text.trim().is_empty() {
        return Err("empty response".into());
    }
    Ok(text)
}

fn error_message(event: &serde_json::Value) -> Option<String> {
    let error = &event["error"];
    if error.is_null() {
        return None;
    }
    Some(error["message"].as_str().or(error.as_str()).unwrap_or("stream error").to_string())
}

/// OpenAI-style chat completion chunks. A reply cut short by the length
/// limit keeps what was written.
fn read_chat_stream(reader: impl BufRead, on_text: &mut dyn FnMut(&str)) -> Result<String, String> {
    let mut text = String::new();
    let mut refused = false;
    for event in events(reader) {
        let event = event?;
        if let Some(message) = error_message(&event) {
            return Err(message);
        }
        let choice = &event["choices"][0];
        if let Some(piece) = choice["delta"]["content"].as_str() {
            text.push_str(piece);
            on_text(piece);
        }
        if choice["delta"]["refusal"].is_string() || choice["finish_reason"] == "content_filter" {
            refused = true;
        }
    }
    finish(text, refused)
}

/// Anthropic Messages stream events.
fn read_anthropic_stream(reader: impl BufRead, on_text: &mut dyn FnMut(&str)) -> Result<String, String> {
    let mut text = String::new();
    let mut refused = false;
    for event in events(reader) {
        let event = event?;
        match event["type"].as_str() {
            Some("content_block_delta") => {
                if let Some(piece) = event["delta"]["text"].as_str() {
                    text.push_str(piece);
                    on_text(piece);
                }
            }
            Some("message_delta") if event["delta"]["stop_reason"] == "refusal" => refused = true,
            Some("error") => return Err(error_message(&event).unwrap_or_else(|| "stream error".into())),
            _ => {}
        }
    }
    finish(text, refused)
}

/// The "# Title" line of the enhanced notes, if any.
pub fn title_of(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|l| l.strip_prefix("# ")).map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
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

    const CHAT: &str = "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"# Sync\\n\"}}]}\n\n\
: keep-alive comment\n\
data: {\"choices\":[{\"delta\":{\"content\":\"Done.\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n";

    #[test]
    fn reads_chat_completion_chunks() {
        let mut pieces = Vec::new();
        let text = read_chat_stream(CHAT.as_bytes(), &mut |p| pieces.push(p.to_string())).unwrap();
        assert_eq!(text, "# Sync\nDone.");
        assert_eq!(pieces.len(), 2);
    }

    #[test]
    fn chat_stream_reports_errors_and_refusals() {
        let error = "data: {\"error\":{\"message\":\"Rate limit\",\"code\":429}}\n";
        assert_eq!(read_chat_stream(error.as_bytes(), &mut |_| {}).unwrap_err(), "Rate limit");
        let filtered = "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"content_filter\"}]}\n";
        assert_eq!(read_chat_stream(filtered.as_bytes(), &mut |_| {}).unwrap_err(), "refusal");
        assert_eq!(read_chat_stream("data: [DONE]\n".as_bytes(), &mut |_| {}).unwrap_err(), "empty response");
    }

    #[test]
    fn keeps_text_cut_short_by_the_length_limit() {
        let cut = CHAT.replace("\"stop\"", "\"length\"");
        assert_eq!(read_chat_stream(cut.as_bytes(), &mut |_| {}).unwrap(), "# Sync\nDone.");
    }

    const ANTHROPIC: &str = "event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"# Sync\\n\"}}\n\n\
event: ping\n\
data: {\"type\":\"ping\"}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Done.\"}}\n\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n";

    #[test]
    fn reads_anthropic_events() {
        assert_eq!(read_anthropic_stream(ANTHROPIC.as_bytes(), &mut |_| {}).unwrap(), "# Sync\nDone.");
        let overloaded = "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n";
        assert_eq!(read_anthropic_stream(overloaded.as_bytes(), &mut |_| {}).unwrap_err(), "Overloaded");
        let refusal = "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"refusal\"}}\n";
        assert_eq!(read_anthropic_stream(refusal.as_bytes(), &mut |_| {}).unwrap_err(), "refusal");
    }

    #[test]
    fn styles_add_their_own_guidance() {
        assert_eq!(style_instructions("general"), "");
        assert!(style_instructions("sales").contains("next steps"));
        assert!(STYLES.iter().all(|s| *s == "general" || !style_instructions(s).is_empty()));
    }

    #[test]
    fn finds_the_title() {
        assert_eq!(title_of("# Sales sync\nSummary"), Some("Sales sync".into()));
        assert_eq!(title_of("No heading"), None);
    }
}

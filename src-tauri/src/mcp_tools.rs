//! What Xnote offers over MCP (`Xnote.exe --mcp`): reading the saved
//! meetings and the transcription setup. Only reading: recording, editing
//! and deleting stay in the window. API keys are never shown.

use crate::mcp::{self, Tool};
use crate::meetings::{self, Meeting};
use serde_json::{json, Value};

const INSTRUCTIONS: &str = "Xnote records meetings and keeps, for each one, the user's notes, the \
transcript and the AI-enhanced notes, as files on this PC. list_meetings gives the meetings, newest \
first; get_meeting gives one in full; search_meetings finds text in all of them. Times: created is Unix \
milliseconds, transcript ms counts from the start of the meeting, who is \"me\" (microphone) or \"them\" \
(computer audio). Nothing is changed.";

/// Around `at` in `text`, cut at character boundaries.
fn snippet(text: &str, at: usize, length: usize) -> String {
    let start = text[..at].char_indices().rev().nth(60).map_or(0, |(i, _)| i);
    let end = text[at..].char_indices().nth(length + 60).map_or(text.len(), |(i, _)| at + i);
    let mut out = text[start..end].replace('\n', " ");
    if start > 0 {
        out.insert(0, '…');
    }
    if end < text.len() {
        out.push('…');
    }
    out
}

/// Where lowercase `query` is in `text`, ignoring case.
fn position(text: &str, query: &str) -> Option<usize> {
    let lower = text.to_lowercase();
    let at = lower.find(query)?;
    // Lowercasing can change byte lengths; then the passage starts at the top.
    Some(if lower.len() == text.len() && text.is_char_boundary(at) { at } else { 0 })
}

/// Where `query` shows up in `meeting`.
fn hits(meeting: &Meeting, query: &str) -> Vec<Value> {
    let query = query.to_lowercase();
    let mut found = Vec::new();
    for (field, text) in [("title", &meeting.title), ("notes", &meeting.notes), ("enhanced", &meeting.enhanced)] {
        if let Some(at) = position(text, &query) {
            found.push(json!({ "in": field, "text": snippet(text, at, query.len()) }));
        }
    }
    for line in &meeting.transcript {
        if position(&line.text, &query).is_some() {
            found.push(json!({ "in": "transcript", "ms": line.ms, "who": line.who, "text": line.text }));
        }
    }
    found
}

pub fn serve() {
    let store = std::rc::Rc::new(meetings::Store::default());
    let tools = vec![
        Tool::new(
            "list_meetings",
            "Saved meetings, newest first: id, title, created, duration in ms, whether it has enhanced notes.",
            mcp::no_input(),
            {
                let store = store.clone();
                move |_| Ok(json!(store.list()))
            },
        ),
        Tool::new(
            "get_meeting",
            "One meeting in full: title, the user's notes, the enhanced notes (Markdown) and the transcript.",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "Meeting id from list_meetings." } },
                "required": ["id"],
            }),
            {
                let store = store.clone();
                move |args| Ok(json!(store.get(&mcp::text(args, "id")?)?))
            },
        ),
        Tool::new(
            "search_meetings",
            "Finds text in every meeting's title, notes, enhanced notes and transcript. Returns the \
             meetings that match, newest first, with the matching passages.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Text to look for (any case)." },
                    "limit": { "type": "integer", "description": "Most meetings to return (default 20)." },
                },
                "required": ["query"],
            }),
            move |args| {
                let query = mcp::text(args, "query")?;
                let limit = mcp::number(args, "limit", 20) as usize;
                let mut results = Vec::new();
                for summary in store.list() {
                    let Ok(meeting) = store.get(&summary.id) else { continue };
                    let mut found = hits(&meeting, &query);
                    if found.is_empty() {
                        continue;
                    }
                    let total = found.len();
                    found.truncate(10);
                    results.push(json!({
                        "id": meeting.id,
                        "title": meeting.title,
                        "created": meeting.created,
                        "matches": total,
                        "passages": found,
                    }));
                    if results.len() == limit {
                        break;
                    }
                }
                Ok(json!(results))
            },
        ),
        Tool::new(
            "transcription_status",
            "How Xnote transcribes: local (whisper.cpp, with the installed builds and models and whether \
             an NVIDIA GPU is there) or cloud (the provider, and which providers have a key saved).",
            mcp::no_input(),
            |_| Ok(json!(crate::engine_info_of(crate::load_settings()))),
        ),
    ];
    mcp::serve("Xnote", "xnote.json", INSTRUCTIONS, tools);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_text_in_any_case_and_cuts_passages_safely() {
        let meeting = Meeting {
            title: "Reunião de orçamento".into(),
            notes: "a".repeat(200) + " Prazo final em março " + &"b".repeat(200),
            transcript: vec![meetings::Line { ms: 4200, who: "them".into(), text: "O ORÇAMENTO fecha hoje".into() }],
            ..Default::default()
        };
        let found = hits(&meeting, "orçamento");
        assert_eq!(found.len(), 2);
        assert_eq!(found[1]["ms"], 4200);
        let notes = hits(&meeting, "prazo");
        let text = notes[0]["text"].as_str().unwrap();
        assert!(text.starts_with('…') && text.ends_with('…') && text.contains("Prazo final"));
    }
}

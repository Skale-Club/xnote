//! What Xnote offers over MCP (`Xnote.exe --mcp`): reading the saved
//! meetings and the transcription setup, and saving a summary the assistant
//! wrote (when Settings allow it). Recording, editing notes and deleting stay
//! in the window. API keys are never shown.

use crate::mcp::{self, Prompt, Tool};
use crate::meetings::{self, Meeting};
use serde_json::{json, Value};

const INSTRUCTIONS: &str = "Xnote records meetings and keeps, for each one, the user's notes, the \
transcript and the AI-enhanced notes, as files on this PC. list_meetings gives the meetings, newest \
first; get_meeting gives one in full; search_meetings finds text in all of them. Times: created is Unix \
milliseconds, transcript ms counts from the start of the meeting, who is \"me\" (microphone) or \"them\" \
(computer audio). To summarize a meeting: read it with get_meeting, write the notes following \
save_summary's description, then save them with save_summary so they appear in Xnote (the \
summarize_meeting prompt does all of this). Nothing is ever deleted.";

/// Saves `markdown` as the meeting's enhanced notes; the "# Title" line
/// names an untitled meeting. Checks afterwards that the window, which may be
/// saving the same file, did not overwrite it.
fn save_summary(store: &meetings::Store, id: &str, markdown: &str) -> Result<Value, String> {
    if !crate::load_settings().assistants_can_write {
        return Err(
            "Saving summaries is turned off in Xnote (Settings → AI assistants). Show the summary to the user instead.".into()
        );
    }
    let markdown = markdown.trim();
    if markdown.is_empty() {
        return Err("Missing \"markdown\".".into());
    }
    if markdown.len() > 200_000 {
        return Err("The summary is too long.".into());
    }
    let title = crate::ai::title_of(markdown);
    for _ in 0..3 {
        let saved = store.update(id, |m| {
            m.mark_sent_to("AI assistant (MCP)");
            m.enhanced = markdown.to_string();
            if m.title.trim().is_empty() {
                if let Some(title) = &title {
                    m.title = title.clone();
                }
            }
        })?;
        std::thread::sleep(std::time::Duration::from_millis(300));
        if store.get(id)?.enhanced == markdown {
            return Ok(json!({ "saved": true, "id": saved.id, "title": saved.title }));
        }
    }
    Err("The meeting is being saved by Xnote right now; try again in a moment.".into())
}

/// The summarize_meeting prompt: Xnote's own summary instructions plus the
/// meeting, in the meeting language from Settings.
fn summary_prompt(store: &meetings::Store, args: &Value) -> Result<Value, String> {
    let meeting = store.get(&mcp::text(args, "id")?)?;
    if meeting.transcript.is_empty() && meeting.notes.trim().is_empty() {
        return Err("This meeting has no notes or transcript yet.".into());
    }
    let settings = crate::load_settings();
    let language = mcp::optional_text(args, "language").unwrap_or(settings.transcription_language.clone());
    let save = if settings.assistants_can_write {
        format!("When the notes are ready, save them with the save_summary tool (id \"{}\") so they appear in Xnote, then show them to me.", meeting.id)
    } else {
        "Show the notes to me; saving into Xnote is turned off.".to_string()
    };
    let style = crate::ai::style_instructions(&settings.summary_style);
    Ok(json!([mcp::user_message(format!(
        "{}\n\n{}\n\n{style}\n\n{save}",
        crate::ai::SYSTEM,
        crate::ai::prompt(&meeting, &language)
    ))]))
}

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
pub(crate) fn hits(meeting: &Meeting, query: &str) -> Vec<Value> {
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

/// The search_meetings tool.
fn search(store: &meetings::Store, args: &Value) -> Result<Value, String> {
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
}

pub fn serve() {
    let store = std::rc::Rc::new(meetings::Store::default());
    let prompt_store = store.clone();
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
            {
                let store = store.clone();
                move |args| search(&store, args)
            },
        ),
        Tool::new(
            "save_summary",
            "Saves meeting notes you wrote as the meeting's enhanced notes in Xnote (replacing earlier ones). \
             Write Markdown: '# A short title', a two or three sentence summary, then '## ' sections that fit \
             (Key points, Decisions, Action items as '- [ ] task (owner)', Open questions). Keep every point \
             of the user's own notes; never invent facts. An untitled meeting takes the title.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Meeting id from list_meetings." },
                    "markdown": { "type": "string", "description": "The notes, in Markdown." },
                },
                "required": ["id", "markdown"],
            }),
            {
                let store = store.clone();
                move |args| save_summary(&store, &mcp::text(args, "id")?, &mcp::text(args, "markdown")?)
            },
        )
        .changing(),
        Tool::new(
            "transcription_status",
            "How Xnote transcribes: local (whisper.cpp, with the installed builds and models and whether \
             an NVIDIA GPU is there) or cloud (the provider, and which providers have a key saved).",
            mcp::no_input(),
            |_| Ok(json!(crate::engine_info_of(crate::load_settings()))),
        ),
    ];
    let prompts = vec![Prompt::new(
        "summarize_meeting",
        "Turn a meeting's notes and transcript into clear notes with decisions and tasks, and save them in Xnote.",
        vec![
            ("id", "Meeting id (list_meetings gives them).", true),
            ("language", "Language code for the notes (pt, en, es); default: the meeting language in Xnote.", false),
        ],
        move |args| summary_prompt(&prompt_store, args),
    )];
    mcp::serve_with_prompts("Xnote", "xnote.json", INSTRUCTIONS, tools, prompts);
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

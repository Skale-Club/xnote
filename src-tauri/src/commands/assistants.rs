//! The "AI assistants" card in Settings: how to connect Claude, Codex,
//! Cursor and other MCP clients to `Xnote.exe --mcp`, and whether they may
//! save summaries into meetings.

use crate::{save_settings, AppState};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantInfo {
    /// This executable, for the configuration snippets.
    exe: String,
    can_write: bool,
}

#[tauri::command]
pub fn assistant_info(state: State<AppState>) -> AssistantInfo {
    AssistantInfo {
        exe: std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
        can_write: state.settings.lock().unwrap().assistants_can_write,
    }
}

#[tauri::command]
pub fn set_assistants_can_write(state: State<AppState>, allowed: bool) {
    let mut s = state.settings.lock().unwrap();
    s.assistants_can_write = allowed;
    save_settings(&s);
}

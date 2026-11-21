//! The Help section of Settings: version, the Data folder, audio waiting to
//! be sent, and diagnostics to paste into a bug report (never keys or text
//! from meetings).

use crate::{applog, pending, portable, providers, settings, AppState};
use serde::Serialize;
use std::process::Command;
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelpInfo {
    version: &'static str,
    data_folder: String,
    writable: bool,
    /// Speech segments waiting for the cloud.
    pending: usize,
}

#[tauri::command]
pub fn help_info() -> HelpInfo {
    HelpInfo {
        version: env!("CARGO_PKG_VERSION"),
        data_folder: portable::data_dir().display().to_string(),
        writable: portable::writable(),
        pending: pending::count(),
    }
}

#[tauri::command]
pub fn open_data_folder() {
    let _ = Command::new("explorer.exe").arg(portable::data_dir()).spawn();
}

#[tauri::command]
pub async fn retry_pending_now(app: AppHandle) -> Result<usize, String> {
    crate::blocking(move || {
        crate::retry_pending(&app);
        Ok(pending::count())
    })
    .await
}

fn windows_version() -> String {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if let Ok(out) = Command::new("cmd").args(["/C", "ver"]).creation_flags(CREATE_NO_WINDOW).output() {
            return String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    std::env::consts::OS.to_string()
}

/// Plain text for a bug report: setup and recent log lines, no secrets.
#[tauri::command]
pub fn diagnostics(app: AppHandle) -> String {
    let s = settings(&app);
    let ready: Vec<String> = providers::all(&s.custom_providers)
        .iter()
        .filter(|p| p.needs_key && providers::has_key(p))
        .map(|p| p.id.clone())
        .collect();
    let recording = app.state::<AppState>().session.lock().unwrap().is_some();
    format!(
        "Xnote {}\n{}\nTranscription: {} (engine {} / model {}, provider {} {})\nSummary: {} {}, automatic {}\n\
         Keys saved for: {}\nMeeting language: {}\nData folder writable: {}\nWaiting segments: {}\nRecording now: {}\n\nRecent log:\n{}",
        env!("CARGO_PKG_VERSION"),
        windows_version(),
        if s.mode().is_empty() { "not set up" } else { s.mode() },
        s.pack,
        s.model,
        s.provider,
        s.transcription_model,
        if s.summary_id().is_empty() { "off" } else { s.summary_id() },
        s.summary_model,
        s.auto_summary,
        if ready.is_empty() { "none".into() } else { ready.join(", ") },
        s.transcription_language,
        portable::writable(),
        pending::count(),
        recording,
        applog::tail(40),
    )
}

/// Opens the page to report a problem.
#[tauri::command]
pub fn open_issues() {
    let _ = Command::new("explorer.exe").arg("https://github.com/Skale-Club/xnote/issues/new/choose").spawn();
}

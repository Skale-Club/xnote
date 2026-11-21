//! Commands of the Providers screen and the setup: API keys, models, which
//! provider transcribes and which one (if any) writes the summaries.

use crate::providers::{self, Custom};
use crate::{blocking, engine_info, save_settings, settings, AppState, EngineInfo};
use std::process::Command;
use tauri::{AppHandle, Manager};

fn find(app: &AppHandle, id: &str) -> Result<providers::Provider, String> {
    settings(app).find(id).ok_or_else(|| "unknown provider".to_string())
}

/// Checks and stores a provider's key (a free call that lists models).
#[tauri::command]
pub async fn set_provider_key(app: AppHandle, provider: String, key: String) -> Result<EngineInfo, String> {
    let p = find(&app, &provider)?;
    blocking(move || providers::set_key(&p, &key)).await?;
    Ok(engine_info(&app))
}

#[tauri::command]
pub fn clear_provider_key(app: AppHandle, provider: String) -> Result<EngineInfo, String> {
    providers::clear_key(&find(&app, &provider)?)?;
    Ok(engine_info(&app))
}

/// The models a provider offers; for Ollama and LM Studio, the installed
/// ones (an error "not running" means the app is not open).
#[tauri::command]
pub async fn list_models(app: AppHandle, provider: String) -> Result<Vec<String>, String> {
    let p = find(&app, &provider)?;
    blocking(move || providers::models(&p)).await
}

/// Checks and stores a key, then transcribes with that provider.
#[tauri::command]
pub async fn set_cloud_key(app: AppHandle, provider: String, key: String, model: Option<String>) -> Result<EngineInfo, String> {
    let p = find(&app, &provider)?;
    blocking(move || providers::set_key(&p, &key)).await?;
    use_cloud(app, provider, model)
}

/// Transcribes in the cloud with a provider that is ready.
#[tauri::command]
pub fn use_cloud(app: AppHandle, provider: String, model: Option<String>) -> Result<EngineInfo, String> {
    let p = find(&app, &provider)?;
    if !p.can_transcribe {
        return Err("cannot transcribe".into());
    }
    if !providers::usable(&p) {
        return Err("no transcription key".into());
    }
    let model = model.unwrap_or_default().trim().to_string();
    if model.is_empty() && p.transcribe_model.is_empty() {
        return Err("no model".into());
    }
    let state = app.state::<AppState>();
    {
        let mut s = state.settings.lock().unwrap();
        s.transcription = "cloud".into();
        s.provider = provider;
        s.transcription_model = model;
        save_settings(&s);
    }
    *state.server.lock().unwrap() = None; // frees the graphics card
    Ok(engine_info(&app))
}

/// Chooses who writes the summaries. An empty provider turns them off:
/// Xnote then only records and transcribes.
#[tauri::command]
pub fn set_summary(app: AppHandle, provider: String, model: String, auto: bool) -> Result<EngineInfo, String> {
    if !provider.is_empty() {
        let p = find(&app, &provider)?;
        if !p.can_summarize {
            return Err("cannot summarize".into());
        }
    }
    let state = app.state::<AppState>();
    {
        let mut s = state.settings.lock().unwrap();
        s.summary_provider = Some(provider);
        s.summary_model = model.trim().to_string();
        s.auto_summary = auto;
        save_settings(&s);
    }
    Ok(engine_info(&app))
}

/// Adds any OpenAI-compatible address (a company gateway, a server on this
/// PC, another provider). The key, when given, is checked first.
#[tauri::command]
pub async fn add_custom_provider(
    app: AppHandle,
    name: String,
    base_url: String,
    key: Option<String>,
    can_transcribe: bool,
) -> Result<EngineInfo, String> {
    let name = name.trim().chars().take(40).collect::<String>();
    if name.is_empty() {
        return Err("empty name".into());
    }
    let base_url = providers::clean_base_url(&base_url)?;
    let key = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
    let taken: Vec<String> = settings(&app).custom_providers.iter().map(|c| c.id.clone()).collect();
    let custom = Custom { id: providers::custom_id(&name, &taken), name, base_url, needs_key: key.is_some(), can_transcribe };
    if let Some(key) = key {
        let provider = providers::all(std::slice::from_ref(&custom)).pop().unwrap();
        blocking(move || providers::set_key(&provider, &key)).await?;
    }
    let state = app.state::<AppState>();
    {
        let mut s = state.settings.lock().unwrap();
        s.custom_providers.push(custom);
        save_settings(&s);
    }
    Ok(engine_info(&app))
}

/// Removes a provider the user added, with its key. Whatever used it goes
/// back to a safe choice: summaries off, transcription to be set up again.
#[tauri::command]
pub fn remove_custom_provider(app: AppHandle, id: String) -> Result<EngineInfo, String> {
    let p = find(&app, &id)?;
    if !p.custom {
        return Err("built-in provider".into());
    }
    let _ = providers::clear_key(&p);
    let state = app.state::<AppState>();
    {
        let mut s = state.settings.lock().unwrap();
        s.custom_providers.retain(|c| c.id != id);
        if s.summary_id() == id {
            s.summary_provider = Some(String::new());
            s.summary_model.clear();
        }
        if s.provider == id {
            s.provider = "groq".into();
            s.transcription_model.clear();
            if s.transcription == "cloud" {
                s.transcription.clear();
            }
        }
        save_settings(&s);
    }
    Ok(engine_info(&app))
}

/// Opens a provider's key page (or download page) in the default browser.
#[tauri::command]
pub fn open_key_page(app: AppHandle, provider: String) {
    let Ok(p) = find(&app, &provider) else { return };
    if p.key_page.starts_with("https://") {
        let _ = Command::new("explorer.exe").arg(&p.key_page).spawn();
    }
}

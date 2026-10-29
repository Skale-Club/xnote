// Xnote: AI meeting notes for Windows, in the spirit of Granola. It records
// the microphone and the computer's audio (no bot joins the call),
// transcribes locally with whisper.cpp (or in the cloud with Groq or OpenAI,
// when the user picks that), and turns the user's rough notes plus the
// transcript into clear notes with ChatGPT (OpenAI API). Portable: everything
// it writes lives in Data\ next to the exe.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod audio;
mod cloud;
mod engine;
mod mcp;
mod mcp_tools;
mod meetings;
mod portable;
mod startup;
mod updater;

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::{fs, process::Command};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};

/// Settings kept in Data/xnote.json beside the executable.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Settings {
    language: String,
    /// "auto", "pt", "en" or "es"; also the language of the enhanced notes.
    transcription_language: String,
    /// "local" (whisper.cpp on this computer) or "cloud"; empty until chosen.
    transcription: String,
    /// Chosen whisper.cpp build ("cuda" / "cpu") and model ("best" / "light").
    pack: String,
    model: String,
    /// Cloud provider: "groq" or "openai".
    provider: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "en".into(),
            transcription_language: "auto".into(),
            transcription: String::new(),
            pack: String::new(),
            model: String::new(),
            provider: "groq".into(),
        }
    }
}

impl Settings {
    /// Settings from before the cloud option have an engine but no mode.
    fn mode(&self) -> &str {
        if self.transcription.is_empty() && !self.pack.is_empty() {
            "local"
        } else {
            &self.transcription
        }
    }
}

struct Session {
    id: String,
    recorder: audio::Recorder,
    worker: JoinHandle<()>,
    started: Instant,
    /// Length of earlier recordings in the same meeting.
    offset_ms: u64,
    levels_stop: Arc<AtomicBool>,
}

struct AppState {
    settings: Mutex<Settings>,
    store: meetings::Store,
    startup: startup::Coordinator,
    server: Mutex<Option<engine::Server>>,
    session: Mutex<Option<Session>>,
    levels: Arc<audio::Levels>,
}

fn save_settings(settings: &Settings) {
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(portable::data_dir().join("xnote.json"), text);
    }
}

fn load_settings() -> Settings {
    match fs::read_to_string(portable::data_dir().join("xnote.json")) {
        // Some editors save UTF-8 with a BOM, which serde_json rejects.
        Ok(text) => serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_default(),
        Err(_) => {
            let settings = Settings::default();
            save_settings(&settings);
            settings
        }
    }
}

fn settings(app: &AppHandle) -> Settings {
    app.state::<AppState>().settings.lock().unwrap().clone()
}

/// Runs blocking work off the UI thread.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
}

// ---------- settings ----------

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn set_language(state: State<AppState>, language: String) {
    let mut s = state.settings.lock().unwrap();
    s.language = language;
    save_settings(&s);
}

#[tauri::command]
fn set_transcription_language(state: State<AppState>, language: String) -> Result<(), String> {
    if !["auto", "pt", "en", "es"].contains(&language.as_str()) {
        return Err("unknown language".into());
    }
    let mut s = state.settings.lock().unwrap();
    s.transcription_language = language;
    save_settings(&s);
    Ok(())
}

// ---------- transcription engine ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineInfo {
    status: engine::Status,
    mode: String,
    pack: String,
    model: String,
    provider: String,
    /// Providers with a saved key.
    cloud_keys: Vec<&'static str>,
}

fn engine_info(app: &AppHandle) -> EngineInfo {
    engine_info_of(settings(app))
}

fn engine_info_of(s: Settings) -> EngineInfo {
    EngineInfo {
        status: engine::status(),
        mode: s.mode().to_string(),
        pack: s.pack,
        model: s.model,
        provider: s.provider,
        cloud_keys: cloud::PROVIDERS
            .iter()
            .filter(|p| cloud::has_key(p))
            .map(|p| p.id)
            .collect(),
    }
}

#[tauri::command]
fn engine_status(app: AppHandle) -> EngineInfo {
    engine_info(&app)
}

#[tauri::command]
async fn install_engine(app: AppHandle, pack: String, model: String) -> Result<EngineInfo, String> {
    let p = engine::pack(&pack).ok_or("unknown engine")?;
    let m = engine::model(&model).ok_or("unknown model")?;
    let progress_app = app.clone();
    blocking(move || {
        engine::install(p, m, &mut |item, done, total| {
            let _ = progress_app.emit(
                "engine-progress",
                json!({ "item": item, "done": done, "total": total }),
            );
        })
    })
    .await?;
    let state = app.state::<AppState>();
    {
        let mut s = state.settings.lock().unwrap();
        s.transcription = "local".into();
        s.pack = pack;
        s.model = model;
        save_settings(&s);
    }
    *state.server.lock().unwrap() = None; // restart with the new choice
    Ok(engine_info(&app))
}

/// Checks and stores a cloud provider's key, then transcribes with it.
#[tauri::command]
async fn set_cloud_key(
    app: AppHandle,
    provider: String,
    key: String,
) -> Result<EngineInfo, String> {
    let p = cloud::provider(&provider).ok_or("unknown provider")?;
    blocking(move || cloud::set_key(p, &key)).await?;
    use_cloud(app, provider)
}

/// Transcribes in the cloud with a provider whose key is already saved.
#[tauri::command]
fn use_cloud(app: AppHandle, provider: String) -> Result<EngineInfo, String> {
    let p = cloud::provider(&provider).ok_or("unknown provider")?;
    if !cloud::has_key(p) {
        return Err("no transcription key".into());
    }
    let state = app.state::<AppState>();
    {
        let mut s = state.settings.lock().unwrap();
        s.transcription = "cloud".into();
        s.provider = provider;
        save_settings(&s);
    }
    *state.server.lock().unwrap() = None; // frees the graphics card
    Ok(engine_info(&app))
}

#[tauri::command]
fn clear_cloud_key(app: AppHandle, provider: String) -> Result<EngineInfo, String> {
    let p = cloud::provider(&provider).ok_or("unknown provider")?;
    cloud::clear_key(p)?;
    Ok(engine_info(&app))
}

/// Opens a provider's API key page in the default browser.
#[tauri::command]
fn open_key_page(provider: String) {
    let url = match provider.as_str() {
        "groq" => "https://console.groq.com/keys",
        "openai" => "https://platform.openai.com/api-keys",
        "anthropic" => "https://console.anthropic.com/settings/keys",
        _ => return,
    };
    let _ = Command::new("explorer.exe").arg(url).spawn();
}

/// Where the recording's segments get transcribed.
enum Transcriber {
    Local(u16),
    Cloud(&'static cloud::Provider),
}

impl Transcriber {
    fn transcribe(&self, samples: &[f32], language: &str) -> Result<Option<String>, String> {
        match self {
            Transcriber::Local(port) => engine::transcribe(*port, samples, language),
            Transcriber::Cloud(provider) => cloud::transcribe(provider, samples, language),
        }
    }
}

/// The transcriber for the current settings; "engine" when none is set up.
fn transcriber(
    app: &AppHandle,
    attempt: &startup::Guard<'_>,
    progress: &mut dyn FnMut(&str),
) -> Result<Transcriber, String> {
    attempt.check()?;
    let s = settings(app);
    match s.mode() {
        "local" => ensure_server(app, attempt, progress).map(Transcriber::Local),
        "cloud" => {
            let p = cloud::provider(&s.provider)
                .filter(|p| cloud::has_key(p))
                .ok_or("engine")?;
            Ok(Transcriber::Cloud(p))
        }
        _ => Err("engine".into()),
    }
}

/// Starts (or reuses) the whisper server for the current settings.
fn ensure_server(
    app: &AppHandle,
    attempt: &startup::Guard<'_>,
    progress: &mut dyn FnMut(&str),
) -> Result<u16, String> {
    let s = settings(app);
    let status = engine::status();
    let pack = engine::pack(&s.pack)
        .filter(|p| status.packs.contains(&p.id))
        .ok_or("engine")?;
    let model = engine::model(&s.model)
        .filter(|m| status.models.contains(&m.id))
        .ok_or("engine")?;
    let key = (
        pack.id.to_string(),
        model.id.to_string(),
        s.transcription_language.clone(),
    );
    let state = app.state::<AppState>();
    let mut server = state.server.lock().unwrap();
    if let Some(running) = server.as_mut() {
        if running.key == key && running.ready() {
            return Ok(running.port);
        }
    }
    *server = None;
    let _ = app.emit("engine-loading", true);
    let started = engine::Server::start(
        pack,
        model,
        &s.transcription_language,
        attempt.flag(),
        progress,
    );
    let _ = app.emit("engine-loading", false);
    let started = started?;
    if pack.id == "cuda" && !started.on_gpu {
        let _ = app.emit("gpu-fallback", ());
    }
    let port = started.port;
    *server = Some(started);
    Ok(port)
}

// ---------- meetings ----------

#[tauri::command]
fn list_meetings(state: State<AppState>) -> Vec<meetings::Summary> {
    state.store.list()
}

#[tauri::command]
fn get_meeting(state: State<AppState>, id: String) -> Result<meetings::Meeting, String> {
    state.store.get(&id)
}

#[tauri::command]
fn create_meeting(state: State<AppState>) -> Result<meetings::Meeting, String> {
    state.store.create()
}

/// Saves what the page edits (title, the user's notes); the transcript is
/// only ever appended by the transcriber.
#[tauri::command]
fn update_meeting(
    state: State<AppState>,
    id: String,
    title: Option<String>,
    notes: Option<String>,
) -> Result<meetings::Meeting, String> {
    state.store.update(&id, |m| {
        if let Some(title) = title {
            m.title = title;
        }
        if let Some(notes) = notes {
            m.notes = notes;
        }
    })
}

#[tauri::command]
fn delete_meeting(state: State<AppState>, id: String) -> Result<(), String> {
    if state
        .session
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.id == id)
    {
        return Err("recording".into());
    }
    state.store.delete(&id)
}

// ---------- recording ----------

fn emit_state(app: &AppHandle, id: &str, phase: &str) {
    let _ = app.emit("recording-state", json!({ "id": id, "phase": phase }));
}

/// Starts recording into a meeting; returns warnings such as a missing
/// microphone.
#[tauri::command]
async fn start_recording(
    app: AppHandle,
    id: String,
    request_id: String,
) -> Result<Vec<String>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        if state.session.lock().unwrap().is_some() {
            return Err("already recording".into());
        }
        let attempt = state.startup.begin(&request_id)?;
        let mut progress = |stage: &str| {
            let _ = app.emit(
                "recording-preparation",
                json!({ "id": id, "requestId": request_id, "stage": stage }),
            );
        };
        progress("checking");
        let meeting = state.store.get(&id)?;
        let transcriber = transcriber(&app, &attempt, &mut progress)?;
        attempt.check()?;
        progress("audio");
        let language = settings(&app).transcription_language;

        let (tx, rx) = mpsc::channel::<audio::Segment>();
        let mut recorder = Some(audio::Recorder::start(tx, state.levels.clone())?);
        let warnings = recorder.as_ref().unwrap().warnings.clone();
        let offset_ms = meeting.duration_ms;

        let committed = attempt.commit(|| {
            // Transcribes segments in order of arrival; ends when the recorder's
            // streams are gone and the channel closes.
            let worker = {
                let (app, id) = (app.clone(), id.clone());
                std::thread::spawn(move || {
                    for segment in rx {
                        match transcriber.transcribe(&segment.samples, &language) {
                            Ok(Some(text)) => {
                                let line = meetings::Line {
                                    ms: offset_ms + segment.start_ms,
                                    who: if segment.who == audio::Who::Me {
                                        "me"
                                    } else {
                                        "them"
                                    }
                                    .into(),
                                    text,
                                };
                                let saved = app.state::<AppState>().store.update(&id, |m| {
                                    m.transcript.push(line.clone());
                                    m.transcript.sort_by_key(|l| l.ms);
                                });
                                if saved.is_ok() {
                                    let _ =
                                        app.emit("transcript", json!({ "id": id, "line": line }));
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                let _ = app.emit("transcribe-error", e);
                            }
                        }
                    }
                })
            };

            let levels_stop = Arc::new(AtomicBool::new(false));
            {
                let (app, stop, levels) = (app.clone(), levels_stop.clone(), state.levels.clone());
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let (me, them) = levels.get();
                        let _ = app.emit("levels", json!({ "me": me, "them": them }));
                        std::thread::sleep(Duration::from_millis(100));
                    }
                });
            }

            *state.session.lock().unwrap() = Some(Session {
                id: id.clone(),
                recorder: recorder.take().unwrap(),
                worker,
                started: Instant::now(),
                offset_ms,
                levels_stop,
            });
            emit_state(&app, &id, "recording");
        });
        if let Err(error) = committed {
            if let Some(recorder) = recorder {
                recorder.stop();
            }
            state.levels.reset();
            return Err(error);
        }
        Ok(warnings)
    })
    .await
}

/// Cancellation does not need the engine mutex, so it stays responsive while a
/// model is loading. A committed recording is stopped through stop_recording.
#[tauri::command]
fn cancel_recording_start(state: State<AppState>, request_id: String) -> bool {
    state.startup.cancel(&request_id)
}

/// Stops, waits for the last segments to be transcribed, saves the length.
#[tauri::command]
async fn stop_recording(app: AppHandle) -> Result<meetings::Meeting, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let session = state
            .session
            .lock()
            .unwrap()
            .take()
            .ok_or("not recording")?;
        emit_state(&app, &session.id, "finishing");
        session.levels_stop.store(true, Ordering::Relaxed);
        let elapsed = session.started.elapsed().as_millis() as u64;
        session.recorder.stop();
        let _ = session.worker.join();
        state.levels.reset();
        let meeting = state
            .store
            .update(&session.id, |m| m.duration_ms = session.offset_ms + elapsed);
        emit_state(&app, &session.id, "idle");
        meeting
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Recording {
    id: String,
    elapsed_ms: u64,
}

#[tauri::command]
fn recording_state(state: State<AppState>) -> Option<Recording> {
    state.session.lock().unwrap().as_ref().map(|s| Recording {
        id: s.id.clone(),
        elapsed_ms: s.offset_ms + s.started.elapsed().as_millis() as u64,
    })
}

// ---------- AI ----------

#[tauri::command]
fn has_api_key() -> bool {
    ai::has_key()
}

#[tauri::command]
async fn set_api_key(key: String) -> Result<(), String> {
    blocking(move || ai::set_key(&key)).await
}

#[tauri::command]
fn clear_api_key() -> Result<(), String> {
    ai::clear_key()
}

/// Streams the enhanced notes ("enhance-delta" events) and saves them. The
/// notes are written in the meeting language when one is set.
#[tauri::command]
async fn enhance(app: AppHandle, id: String) -> Result<meetings::Meeting, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let meeting = state.store.get(&id)?;
        if meeting.transcript.is_empty() && meeting.notes.trim().is_empty() {
            return Err("nothing to enhance".into());
        }
        let language = settings(&app).transcription_language;
        let markdown = ai::enhance(&meeting, &language, &mut |piece| {
            let _ = app.emit("enhance-delta", json!({ "id": id, "text": piece }));
        })?;
        let title = ai::title_of(&markdown);
        state.store.update(&id, |m| {
            m.enhanced = markdown.clone();
            if m.title.trim().is_empty() {
                if let Some(title) = title {
                    m.title = title;
                }
            }
        })
    })
    .await
}

/// Opens the Skale Club website in the default browser.
#[tauri::command]
fn open_site() {
    let _ = Command::new("explorer.exe")
        .arg("https://skale.club")
        .spawn();
}
/// Writes Markdown to `Data/exports` beside the exe and shows it in Explorer.
#[tauri::command]
fn save_export(name: String, markdown: String) -> Result<String, String> {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim()
        .chars()
        .take(80)
        .collect();
    let base = if safe.is_empty() {
        "meeting".to_string()
    } else {
        safe
    };
    let dir = portable::data_dir().join("exports");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut path = dir.join(format!("{base}.md"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{base} ({n}).md"));
        n += 1;
    }
    fs::write(&path, markdown).map_err(|e| e.to_string())?;
    let _ = Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn();
    Ok(path.display().to_string())
}

// ---------- updates ----------

/// Where the background updater stands, for the updates panel.
#[tauri::command]
fn update_status() -> updater::Status {
    updater::status()
}

/// Looks for a newer release now; the result arrives as `update-status`.
#[tauri::command]
fn check_update() {
    updater::check_now();
}

/// Installs the downloaded update now and restarts into it. A recording stops
/// as it does when the window closes.
#[tauri::command]
fn restart_to_update(app: AppHandle) -> Result<(), String> {
    updater::restart()?;
    app.exit(0);
    Ok(())
}

fn main() {
    updater::finish_previous();
    portable::migrate(&["xnote.json", "engine", "meetings", "exports"]);
    portable::sync_appinfo();
    if mcp::requested() {
        return mcp_tools::serve();
    }
    tauri::Builder::default()
        .manage(AppState {
            settings: Mutex::new(load_settings()),
            store: meetings::Store::default(),
            startup: startup::Coordinator::default(),
            server: Mutex::new(None),
            session: Mutex::new(None),
            levels: Arc::new(audio::Levels::default()),
        })
        .setup(|app| {
            let handle = app.handle().clone();
            updater::start(move |status| {
                let _ = handle.emit("update-status", status);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_language,
            set_transcription_language,
            engine_status,
            install_engine,
            set_cloud_key,
            use_cloud,
            clear_cloud_key,
            open_key_page,
            list_meetings,
            get_meeting,
            create_meeting,
            update_meeting,
            delete_meeting,
            start_recording,
            cancel_recording_start,
            stop_recording,
            recording_state,
            has_api_key,
            set_api_key,
            clear_api_key,
            enhance,
            save_export,
            open_site,
            update_status,
            check_update,
            restart_to_update
        ])
        .build(tauri::generate_context!())
        .expect("failed to start Xnote")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                let state = app.state::<AppState>();
                state.startup.cancel_current();
                if let Some(session) = state.session.lock().unwrap().take() {
                    session.levels_stop.store(true, Ordering::Relaxed);
                    session.recorder.stop();
                }
                *state.server.lock().unwrap() = None; // kills whisper-server
                updater::apply(); // a downloaded update replaces the exe
            }
        });
}

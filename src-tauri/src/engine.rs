//! Local transcription with whisper.cpp. The official Windows build and a
//! Whisper model are downloaded once into `Data/engine` beside the exe (checked
//! against pinned SHA-256 hashes), then `whisper-server.exe` runs in the
//! background and turns each speech segment into text. Audio never leaves
//! the computer.

use crate::audio::RATE;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const BUILD: &str = "b5454";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Pack {
    pub id: &'static str,
    /// Folder under engine\, named after the exact build so a change of
    /// build never reuses an older install.
    dir: &'static str,
    file: &'static str,
    sha256: &'static str,
}

/// whisper.cpp builds: CUDA for NVIDIA cards, plain CPU for everything else.
/// The CUDA 12.4 build ships cuBLAS; the 11.8 one doesn't, and without it
/// whisper.cpp silently runs on the CPU.
pub const PACKS: [Pack; 2] = [
    Pack {
        id: "cuda",
        dir: "bin-cuda12",
        file: "whisper-bin-win-cuda-12.4.0-x64.zip",
        sha256: "afef0b881c500958921c3f5523b50e59ee2ec9b6f5cbd25b324c51ed308a957a",
    },
    Pack {
        id: "cpu",
        dir: "bin-cpu",
        file: "whisper-bin-x64.zip",
        sha256: "6ba69e3482d7826214f90a6a9c84ca07782aec1e1d0c6a7c30c994fd5d816ccb",
    },
];

pub struct Model {
    pub id: &'static str,
    file: &'static str,
    sha256: &'static str,
}

pub const MODELS: [Model; 2] = [
    Model {
        id: "best",
        file: "ggml-large-v3-turbo-q5_0.bin",
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    },
    Model {
        id: "light",
        file: "ggml-small-q5_1.bin",
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
    },
];

fn engine_dir() -> PathBuf {
    crate::portable::data_dir().join("engine")
}

fn pack_dir(pack: &Pack) -> PathBuf {
    engine_dir().join(pack.dir)
}

fn model_path(model: &Model) -> PathBuf {
    engine_dir().join("models").join(model.file)
}

pub fn pack(id: &str) -> Option<&'static Pack> {
    PACKS.iter().find(|p| p.id == id)
}

pub fn model(id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.id == id)
}

/// The NVIDIA driver installs nvcuda.dll; without it the CUDA build can't run.
pub fn has_nvidia() -> bool {
    std::env::var_os("WINDIR")
        .map(|w| Path::new(&w).join(r"System32\nvcuda.dll").exists())
        .unwrap_or(false)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub gpu: bool,
    pub packs: Vec<&'static str>,
    pub models: Vec<&'static str>,
}

/// What is installed. A pack counts only once fully extracted (".ok"
/// marker); a model only once its checksum matched (".part" renamed).
pub fn status() -> Status {
    Status {
        gpu: has_nvidia(),
        packs: PACKS
            .iter()
            .filter(|p| pack_dir(p).join(".ok").exists())
            .map(|p| p.id)
            .collect(),
        models: MODELS
            .iter()
            .filter(|m| model_path(m).exists())
            .map(|m| m.id)
            .collect(),
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(120))
        .user_agent("xnote")
        .build()
}

/// Downloads `url` to `dest`, verifying SHA-256 on the way. `progress` gets
/// (bytes done, bytes total) a few times a second.
fn download(
    url: &str,
    dest: &Path,
    sha256: &str,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let response = agent().get(url).call().map_err(|e| e.to_string())?;
    let total = response
        .header("Content-Length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    let part = dest.with_extension("part");
    let mut file = File::create(&part).map_err(|e| e.to_string())?;
    let mut reader = response.into_reader();
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let (mut done, mut last) = (0u64, Instant::now());
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if last.elapsed() >= Duration::from_millis(200) {
            last = Instant::now();
            progress(done, total);
        }
    }
    progress(done, total);
    drop(file);
    if format!("{:x}", hasher.finalize()) != sha256 {
        let _ = fs::remove_file(&part);
        return Err("download corrupted (checksum mismatch)".into());
    }
    fs::rename(&part, dest).map_err(|e| e.to_string())
}

/// Extracts the server and its DLLs, flat, ignoring everything else. Only
/// the file name of each entry is used, so no path in the zip can escape.
fn extract(zip_path: &Path, dir: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(File::open(zip_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = Path::new(entry.name())
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
        else {
            continue;
        };
        let lower = name.to_lowercase();
        if lower != "whisper-server.exe" && !lower.ends_with(".dll") {
            continue;
        }
        let mut out = File::create(dir.join(&name)).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    if !dir.join("whisper-server.exe").exists() {
        return Err("whisper-server.exe missing from the download".into());
    }
    File::create(dir.join(".ok"))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Installs a pack and a model that aren't there yet. `progress` gets
/// ("engine" | "model", done, total).
pub fn install(
    pack: &Pack,
    model: &Model,
    progress: &mut dyn FnMut(&str, u64, u64),
) -> Result<(), String> {
    let dir = pack_dir(pack);
    if !dir.join(".ok").exists() {
        let zip = engine_dir().join(pack.file);
        let url = format!(
            "https://github.com/ggml-org/whisper.cpp/releases/download/{BUILD}/{}",
            pack.file
        );
        download(&url, &zip, pack.sha256, &mut |d, t| {
            progress("engine", d, t)
        })?;
        let result = extract(&zip, &dir);
        let _ = fs::remove_file(&zip);
        result?;
    }
    let path = model_path(model);
    if !path.exists() {
        let url = format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            model.file
        );
        download(&url, &path, model.sha256, &mut |d, t| {
            progress("model", d, t)
        })?;
    }
    Ok(())
}

// ---------- server ----------

#[repr(C)]
#[derive(Default)]
struct JobLimits {
    per_process_time: i64,
    per_job_time: i64,
    flags: u32,
    min_working_set: usize,
    max_working_set: usize,
    active_processes: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
    io_counters: [u64; 6],
    process_memory: usize,
    job_memory: usize,
    peak_process_memory: usize,
    peak_job_memory: usize,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(attributes: *mut std::ffi::c_void, name: *const u16) -> isize;
    fn SetInformationJobObject(
        job: isize,
        class: i32,
        info: *mut std::ffi::c_void,
        len: u32,
    ) -> i32;
    fn AssignProcessToJobObject(job: isize, process: isize) -> i32;
    fn CloseHandle(handle: isize) -> i32;
}

/// A job object that kills the server if Xnote exits, even by crashing.
fn kill_with_us(child: &Child) -> isize {
    const EXTENDED_LIMITS: i32 = 9;
    const KILL_ON_JOB_CLOSE: u32 = 0x2000;
    unsafe {
        let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
        if job == 0 {
            return 0;
        }
        let mut limits = JobLimits {
            flags: KILL_ON_JOB_CLOSE,
            ..Default::default()
        };
        SetInformationJobObject(
            job,
            EXTENDED_LIMITS,
            &mut limits as *mut JobLimits as *mut std::ffi::c_void,
            std::mem::size_of::<JobLimits>() as u32,
        );
        AssignProcessToJobObject(job, child.as_raw_handle() as isize);
        job
    }
}

/// A running whisper-server, killed when dropped.
pub struct Server {
    child: Child,
    job: isize,
    pub port: u16,
    /// (pack, model, language) it was started with.
    pub key: (String, String, String),
    /// The model really runs on the graphics card (from the server's log).
    pub on_gpu: bool,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        if self.job != 0 {
            unsafe { CloseHandle(self.job) };
        }
    }
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    listener
        .local_addr()
        .map(|a| a.port())
        .map_err(|e| e.to_string())
}

// HTTP readiness is stronger than merely finding an open TCP port. The pinned
// whisper.cpp build exposes /health with status=ok only after model initialization.
fn health_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .try_proxy_from_env(false)
        .timeout_connect(Duration::from_millis(300))
        .timeout_read(Duration::from_millis(500))
        .timeout_write(Duration::from_millis(300))
        .timeout(Duration::from_millis(900))
        .build()
}

fn probe_health(agent: &ureq::Agent, port: u16) -> Result<bool, String> {
    match agent.get(&format!("http://127.0.0.1:{port}/health")).call() {
        Ok(response) => {
            let body: serde_json::Value = response.into_json().map_err(|e| e.to_string())?;
            Ok(body.get("status").and_then(|s| s.as_str()) == Some("ok"))
        }
        Err(ureq::Error::Status(503, _)) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

fn startup_log_tail(path: &Path) -> String {
    // Before capture starts this log contains engine/model initialization, not
    // meeting content. Keep diagnostic output bounded in both memory and the UI.
    use std::io::{Seek, SeekFrom};
    let Ok(mut file) = File::open(path) else {
        return String::new();
    };
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = file.seek(SeekFrom::Start(size.saturating_sub(4096)));
    let mut bytes = Vec::new();
    let _ = file.take(4096).read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes)
        .lines()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

impl Server {
    pub fn ready(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
            && probe_health(&health_agent(), self.port).unwrap_or(false)
    }

    /// Progress is stage-based, never a guessed percentage. Cancellation drops
    /// the partially initialized child and its job object before returning.
    pub fn start(
        pack: &Pack,
        model: &Model,
        language: &str,
        cancelled: &std::sync::atomic::AtomicBool,
        progress: &mut dyn FnMut(&str),
    ) -> Result<Server, String> {
        use std::sync::atomic::Ordering;
        if cancelled.load(Ordering::Acquire) {
            return Err("startup_cancelled".into());
        }
        progress("loading");
        let dir = pack_dir(pack);
        let port = free_port()?;
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let threads = (cores / 2).clamp(4, 12);
        let log_path = engine_dir().join("server.log");
        let log = File::create(&log_path).map_err(|e| e.to_string())?;
        let child = Command::new(dir.join("whisper-server.exe"))
            .arg("-m")
            .arg(model_path(model))
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["-l", language, "-t", &threads.to_string()])
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("engine_start_failed|{e}"))?;
        let job = kill_with_us(&child);
        let mut server = Server {
            child,
            job,
            port,
            key: (pack.id.into(), model.id.into(), language.into()),
            on_gpu: false,
        };
        let agent = health_agent();
        let deadline = Instant::now() + Duration::from_secs(180);
        let mut last_error = String::new();
        let mut connecting = false;
        while Instant::now() < deadline {
            if cancelled.load(Ordering::Acquire) {
                return Err("startup_cancelled".into());
            }
            if let Ok(Some(status)) = server.child.try_wait() {
                return Err(format!(
                    "engine_start_failed|{status}\n{}",
                    startup_log_tail(&log_path)
                ));
            }
            if !connecting
                && fs::read_to_string(&log_path)
                    .map(|s| s.contains("model size") || s.contains("using CUDA"))
                    .unwrap_or(false)
            {
                connecting = true;
                progress("connecting");
            }
            match probe_health(&agent, port) {
                Ok(true) => {
                    if cancelled.load(Ordering::Acquire) {
                        return Err("startup_cancelled".into());
                    }
                    server.on_gpu = fs::read_to_string(&log_path)
                        .map(|l| l.contains("using CUDA"))
                        .unwrap_or(false);
                    return Ok(server);
                }
                Ok(false) => {
                    last_error = "The engine reports that its model is not ready yet.".into()
                }
                Err(error) => last_error = error,
            }
            // Short sleep slices keep cancellation responsive between probes.
            for _ in 0..5 {
                if cancelled.load(Ordering::Acquire) {
                    return Err("startup_cancelled".into());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        Err(format!(
            "engine_start_timeout|{last_error}\n{}",
            startup_log_tail(&log_path)
        ))
    }
}

#[cfg(test)]
mod readiness_tests {
    use super::*;
    fn response(status: &str, body: &str) -> (u16, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let data = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        let task = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0u8; 1024];
            let n = stream.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..n]).starts_with("GET /health "));
            stream.write_all(data.as_bytes()).unwrap();
        });
        (port, task)
    }
    #[test]
    fn health_requires_an_explicit_ready_response() {
        for (status, body, expected) in [
            ("200 OK", r#"{"status":"ok"}"#, true),
            (
                "503 Service Unavailable",
                r#"{"status":"loading model"}"#,
                false,
            ),
            ("200 OK", r#"{"status":"loading model"}"#, false),
        ] {
            let (port, task) = response(status, body);
            assert_eq!(probe_health(&health_agent(), port).unwrap(), expected);
            task.join().unwrap();
        }
    }
    #[test]
    fn malformed_health_is_not_treated_as_ready() {
        let (port, task) = response("200 OK", "not json");
        assert!(probe_health(&health_agent(), port).is_err());
        task.join().unwrap();
    }
    #[test]
    fn an_already_cancelled_start_does_not_spawn_or_write_logs() {
        let cancelled = std::sync::atomic::AtomicBool::new(true);
        let mut stages = Vec::new();
        assert!(
            matches!(Server::start(&PACKS[1], &MODELS[1], "en", &cancelled, &mut |s| stages.push(s.to_string())), Err(e) if e == "startup_cancelled")
        );
        assert!(stages.is_empty());
    }
}

/// 16-bit PCM WAV, mono, 16 kHz.
pub(crate) fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&(RATE as u32).to_le_bytes());
    out.extend_from_slice(&(RATE as u32 * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// Phrases Whisper tends to invent on near-silence.
pub(crate) fn is_phantom(text: &str, seconds: f32) -> bool {
    let t = text.trim().trim_matches('.').trim().to_lowercase();
    if t.is_empty()
        || (t.starts_with('[') && t.ends_with(']'))
        || (t.starts_with('(') && t.ends_with(')'))
    {
        return true;
    }
    const PHANTOMS: [&str; 9] = [
        "thank you",
        "thanks for watching",
        "obrigado",
        "obrigada",
        "gracias",
        "gracias por ver",
        "legendas pela comunidade amara.org",
        "subtitles by the amara.org community",
        "you",
    ];
    seconds < 3.0 && PHANTOMS.iter().any(|p| t == *p)
}

/// A multipart form with text fields and a WAV file; returns the content
/// type (with its boundary) and the body.
pub(crate) fn multipart(fields: &[(&str, &str)], wav: &[u8]) -> (String, Vec<u8>) {
    let boundary = "xnote-7f3a9c1e";
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes(),
        );
    }
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"segment.wav\"\r\n\
             Content-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(wav);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// Sends one segment to the server; None when nothing real was said.
pub fn transcribe(port: u16, samples: &[f32], language: &str) -> Result<Option<String>, String> {
    let fields = [
        ("response_format", "json"),
        ("temperature", "0.0"),
        ("language", language),
    ];
    let (content_type, body) = multipart(&fields, &wav(samples));
    let response: serde_json::Value = ureq::AgentBuilder::new()
        .try_proxy_from_env(false)
        .build()
        .post(&format!("http://127.0.0.1:{port}/inference"))
        .set("Content-Type", &content_type)
        .timeout(Duration::from_secs(300))
        .send_bytes(&body)
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    let text = response["text"]
        .as_str()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let seconds = samples.len() as f32 / RATE as f32;
    Ok((!is_phantom(&text, seconds)).then_some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_valid() {
        let w = wav(&[0.0, 0.5, -0.5]);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(w.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 16_000);
    }

    #[test]
    fn filters_phantom_phrases_only_when_short() {
        assert!(is_phantom("[BLANK_AUDIO]", 5.0));
        assert!(is_phantom(" Obrigado. ", 1.2));
        assert!(!is_phantom("Obrigado.", 6.0));
        assert!(!is_phantom("Vamos fechar o orçamento na sexta.", 2.0));
    }

    #[test]
    fn job_limits_struct_matches_windows_layout() {
        // JOBOBJECT_EXTENDED_LIMIT_INFORMATION is 144 bytes on x64.
        assert_eq!(std::mem::size_of::<JobLimits>(), 144);
    }
}

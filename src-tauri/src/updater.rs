//! Self-update from GitHub Releases, for a portable exe. It is the same file
//! in every Skale app: change it in one and copy it to the others.
//!
//! `start` checks the app's repository now and every six hours. When the
//! latest release is newer than this build, its exe is downloaded in the
//! background and kept beside this one as `<name>.new.exe`, but only if it
//! matches its `<exe>.sig`, a minisign signature made in CI with the Skale X
//! update key (`tauri signer sign`). A tampered release or download is refused.
//!
//! Swapping: Windows lets a running exe be renamed, so `apply` renames this exe
//! to `<name>.old.exe` and puts the new file in its place. It runs when the app
//! exits, or from `restart`, which also starts the new copy with
//! `--updated-from <pid>`. The next start deletes `<name>.old.exe`. The `Data\`
//! folder is left alone.

use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_SOURCE: &str = concat!(
    "https://github.com/Skale-Club/",
    env!("CARGO_PKG_NAME"),
    "/releases/latest/download/latest.json"
);
const PUBLIC_KEY: &str = include_str!("../updater.pub");
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;
const USER_AGENT: &str = "skale-x-updater";
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// Not checked yet.
    Idle,
    Checking,
    UpToDate,
    Downloading,
    /// Downloaded and verified: it is installed when the app closes.
    Ready,
    Failed,
}

/// Where the updater stands; the window shows it in its updates panel.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub source: String,
    /// This build's version.
    pub current: &'static str,
    pub state: State,
    /// The newer release being downloaded or ready.
    pub version: Option<String>,
    /// Its release notes.
    pub notes: String,
    /// Why the last check failed.
    pub error: Option<String>,
    /// When the last check finished, in Unix seconds.
    pub checked_at: Option<u64>,
}

static STATUS: Mutex<Status> = Mutex::new(Status {
    source: String::new(),
    current: env!("CARGO_PKG_VERSION"),
    state: State::Idle,
    version: None,
    notes: String::new(),
    error: None,
    checked_at: None,
});
static LISTENER: OnceLock<Box<dyn Fn(&Status) + Send + Sync>> = OnceLock::new();
static BUSY: AtomicBool = AtomicBool::new(false);

pub fn status() -> Status {
    let mut status = STATUS.lock().unwrap().clone();
    if status.source.is_empty() {
        status.source = source().unwrap_or_else(|_| DEFAULT_SOURCE.into());
    }
    status
}

#[derive(Deserialize)]
struct Source {
    url: String,
}

fn https(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|_| "invalidUpdateSource")?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("invalidUpdateSource".into());
    }
    Ok(())
}

fn source() -> Result<String, String> {
    let path = crate::portable::data_dir().join("update-source.json");
    let url = match std::fs::read_to_string(&path) {
        Ok(text) => {
            serde_json::from_str::<Source>(&text)
                .map_err(|_| "invalidUpdateSource")?
                .url
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => option_env!("XPACE_UPDATE_URL")
            .unwrap_or(DEFAULT_SOURCE)
            .into(),
        Err(e) => return Err(e.to_string()),
    };
    https(&url)?;
    Ok(url)
}

fn set(change: impl FnOnce(&mut Status)) {
    let status = {
        let mut status = STATUS.lock().unwrap();
        change(&mut status);
        status.clone()
    };
    if let Some(listener) = LISTENER.get() {
        listener(&status);
    }
}

/// Starts the background checks: now, then every six hours. `on_change`
/// hears every change of status.
pub fn start(on_change: impl Fn(&Status) + Send + Sync + 'static) {
    let _ = LISTENER.set(Box::new(on_change));
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(pending) = staged(&exe) {
            set(|s| {
                s.state = State::Ready;
                s.version = Some(pending.version);
                s.notes = pending.notes;
            });
        }
    }
    std::thread::spawn(|| loop {
        run();
        std::thread::sleep(EVERY);
    });
}

/// Checks now, unless a check or download is already running.
pub fn check_now() {
    std::thread::spawn(run);
}

fn run() {
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let before = status();
    let staged = before
        .version
        .clone()
        .filter(|_| before.state == State::Ready);
    set(|s| {
        if s.state != State::Ready {
            s.state = State::Checking;
        }
    });
    let outcome = check().and_then(|found| match found {
        Some(update) if Some(&update.version) != staged.as_ref() => {
            stage(&update).map(|_| Some(update))
        }
        _ => Ok(None),
    });
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    set(|s| {
        s.checked_at = Some(now);
        match outcome {
            Ok(Some(update)) => {
                s.state = State::Ready;
                s.version = Some(update.version);
                s.notes = update.notes;
                s.error = None;
            }
            Ok(None) => {
                if s.state != State::Ready {
                    s.state = State::UpToDate;
                }
                s.error = None;
            }
            Err(error) => {
                // An update downloaded earlier is still on disk and still good.
                if before.state == State::Ready {
                    s.state = State::Ready;
                    s.version = before.version;
                    s.notes = before.notes;
                } else {
                    s.state = State::Failed;
                }
                s.error = Some(error);
            }
        }
    });
    BUSY.store(false, Ordering::SeqCst);
}

struct Update {
    version: String,
    notes: String,
    exe_url: String,
    signature: String,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: String,
    url: String,
    signature: String,
}

fn parse_manifest(text: &str, current: &str) -> Result<Option<Update>, String> {
    let manifest: Manifest = serde_json::from_str(text).map_err(|_| "invalidUpdateFeed")?;
    let latest = version(&manifest.version).ok_or("invalidUpdateFeed")?;
    let current = version(current).ok_or("invalidUpdateFeed")?;
    if latest <= current {
        return Ok(None);
    }
    https(&manifest.url)?;
    Signature::decode(&from_base64(&manifest.signature)?).map_err(|_| "invalidUpdateFeed")?;
    Ok(Some(Update {
        version: manifest.version.trim_start_matches('v').into(),
        notes: manifest.notes,
        exe_url: manifest.url,
        signature: manifest.signature,
    }))
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// "v1.2.3" or "1.2.3" -> (1, 2, 3).
fn version(text: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<_> = text.trim().trim_start_matches('v').split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

/// The latest release, if it is newer than this build and carries a signed exe.
fn check() -> Result<Option<Update>, String> {
    let endpoint = source()?;
    set(|s| s.source = endpoint.clone());
    let body = match ureq::get(&endpoint)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .call()
    {
        Ok(response) => response.into_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(404, _)) => return Err("updateFeedMissing".into()),
        Err(e) => return Err(e.to_string()),
    };
    if serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .is_some_and(|v| v.get("tag_name").is_some())
    {
        parse_release(serde_json::from_str(&body).map_err(|_| "invalidUpdateFeed")?)
    } else {
        parse_manifest(&body, env!("CARGO_PKG_VERSION"))
    }
}

// Legacy GitHub releases remain supported as an explicitly configured source.
fn parse_release(release: Release) -> Result<Option<Update>, String> {
    let latest = version(&release.tag_name).ok_or("invalidUpdateFeed")?;
    let current = version(env!("CARGO_PKG_VERSION")).ok_or("invalidUpdateFeed")?;
    if latest <= current {
        return Ok(None);
    }
    let exe = release
        .assets
        .iter()
        .find(|a| a.name.to_lowercase().ends_with(".exe"));
    let sig = exe.and_then(|e| {
        release
            .assets
            .iter()
            .find(|a| a.name == format!("{}.sig", e.name))
    });
    Ok(match (exe, sig) {
        (Some(exe), Some(sig)) => Some(Update {
            version: release.tag_name.trim_start_matches('v').to_string(),
            notes: release.body.unwrap_or_default(),
            exe_url: exe.browser_download_url.clone(),
            signature: String::from_utf8(download(&sig.browser_download_url)?)
                .map_err(|e| e.to_string())?,
        }),
        _ => return Err("unsignedUpdate".into()),
    })
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    https(url)?;
    let response = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(600))
        .call()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_DOWNLOAD + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DOWNLOAD {
        return Err("updateTooLarge".into());
    }
    Ok(bytes)
}

/// Tauri stores keys and signatures as base64 of the minisign text files.
fn from_base64(text: &str) -> Result<String, String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .map_err(|e| e.to_string())?;
    String::from_utf8(raw).map_err(|e| e.to_string())
}

fn verify(data: &[u8], signature: &str) -> Result<(), String> {
    let key = PublicKey::decode(&from_base64(PUBLIC_KEY)?).map_err(|e| e.to_string())?;
    let signature = Signature::decode(&from_base64(signature)?).map_err(|e| e.to_string())?;
    key.verify(data, &signature, true)
        .map_err(|_| "the update's signature does not match".to_string())
}

/// Read-only CLI verification used by CI before publishing a signed package.
pub fn verify_package(exe: &Path, sig: &Path) -> Result<(), String> {
    let size = std::fs::metadata(exe).map_err(|e| e.to_string())?.len();
    if size > MAX_DOWNLOAD {
        return Err("updateTooLarge".into());
    }
    verify(
        &std::fs::read(exe).map_err(|e| e.to_string())?,
        &std::fs::read_to_string(sig).map_err(|e| e.to_string())?,
    )
}

/// Handle the read-only release verification mode before any GUI/system work.
pub fn verify_cli() -> bool {
    let args: Vec<_> = std::env::args().collect();
    if !args.get(1).is_some_and(|arg| arg == "--verify-update") {
        return false;
    }
    let result = match (args.get(2), args.get(3)) {
        (Some(exe), Some(sig)) => verify_package(Path::new(exe), Path::new(sig)),
        _ => Err("Usage: --verify-update <exe> <signature>".into()),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
    true
}

fn pending_path(exe: &Path) -> PathBuf {
    sibling(exe, "new").with_extension("json")
}

fn staged(exe: &Path) -> Result<Manifest, String> {
    let pending: Manifest = serde_json::from_str(
        &std::fs::read_to_string(pending_path(exe)).map_err(|e| e.to_string())?,
    )
    .map_err(|_| "invalidUpdateFeed")?;
    if version(&pending.version).ok_or("invalidUpdateFeed")?
        <= version(env!("CARGO_PKG_VERSION")).unwrap()
    {
        return Err("no update is ready".into());
    }
    let path = sibling(exe, "new");
    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > MAX_DOWNLOAD {
        return Err("updateTooLarge".into());
    }
    verify(
        &std::fs::read(path).map_err(|e| e.to_string())?,
        &pending.signature,
    )?;
    Ok(pending)
}

fn sibling(exe: &Path, tag: &str) -> PathBuf {
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    exe.with_file_name(format!("{stem}.{tag}.exe"))
}

/// Downloads and verifies the update, then keeps it beside the exe.
fn stage(update: &Update) -> Result<(), String> {
    set(|s| {
        s.state = State::Downloading;
        s.version = Some(update.version.clone());
    });
    let data = download(&update.exe_url)?;
    verify(&data, &update.signature)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let new = sibling(&exe, "new");
    let temporary = new.with_extension("download");
    std::fs::write(&temporary, &data).map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, &new).map_err(|e| e.to_string())?;
    let pending = Manifest {
        version: update.version.clone(),
        notes: update.notes.clone(),
        url: update.exe_url.clone(),
        signature: update.signature.clone(),
    };
    std::fs::write(
        pending_path(&exe),
        serde_json::to_vec(&pending).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// Swaps in the downloaded exe, if there is one, and returns the exe's path.
/// Call it when the app exits.
pub fn apply() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    apply_at(&exe)
}

fn apply_at(exe: &Path) -> Option<PathBuf> {
    // Validate again at execution time; a stray/replaced .new.exe is never run.
    staged(exe).ok()?;
    let (new, old) = (sibling(&exe, "new"), sibling(&exe, "old"));
    if !new.exists() {
        return None;
    }
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).ok()?;
    if std::fs::rename(&new, &exe).is_err() {
        let _ = std::fs::rename(&old, &exe); // put the working copy back
        return None;
    }
    let _ = std::fs::remove_file(pending_path(exe));
    Some(exe.to_path_buf())
}

/// Swaps in the downloaded exe and starts it. The caller must exit right after.
pub fn restart() -> Result<(), String> {
    let exe = apply().ok_or("no update is ready")?;
    Command::new(&exe)
        .arg("--updated-from")
        .arg(std::process::id().to_string())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn WaitForSingleObject(handle: isize, milliseconds: u32) -> u32;
    fn CloseHandle(handle: isize) -> i32;
}

/// Call first thing in main: after an update, wait for the previous copy to
/// exit, then remove it.
pub fn finish_previous() {
    let args: Vec<String> = std::env::args().collect();
    let previous = args
        .iter()
        .position(|a| a == "--updated-from")
        .and_then(|i| args.get(i + 1))
        .and_then(|pid| pid.parse::<u32>().ok());
    if let Some(pid) = previous {
        const SYNCHRONIZE: u32 = 0x0010_0000;
        unsafe {
            let handle = OpenProcess(SYNCHRONIZE, 0, pid);
            if handle != 0 {
                WaitForSingleObject(handle, 15_000);
                CloseHandle(handle);
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(sibling(&exe, "old"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // "hello\r\n" signed with the Skale X update key.
    const DATA: &[u8] = b"hello\r\n";
    const SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVSVnR3VnA0YlBlckpVNTJHbGRQd3Naa1ZmcEFXRmJ4VjdFS0htc2tFbE1vM0NXTmJ4TUpGdVhRUllJd1pSODdrcmpERWEyNWRPQjQ3UWhaL2FmSWJwczZnT095OUNLaGcwPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkzMDQ3NTMyCWZpbGU6c2lndGVzdC5iaW4KV0xLVCtaS2VMcXJPLzBLRzExcG4rSDNXS1BxV1REUEdKNUYvRitDRHB4bk5pTWkya1hCTWRQNUNsMlBMVUV6c0I3cmkwSUFPZVNiejJkaTZ0b2hORFE9PQo=";

    #[test]
    fn accepts_signed_data_and_rejects_tampered() {
        assert!(verify(DATA, SIG).is_ok());
        assert!(verify(b"hellO\r\n", SIG).is_err());
    }

    #[test]
    fn compares_versions() {
        assert_eq!(version("v0.2.0"), Some((0, 2, 0)));
        assert!(version("v0.10.0") > version("0.9.9"));
        assert_eq!(version("garbage"), None);
    }

    #[test]
    fn names_siblings_next_to_the_exe() {
        let exe = Path::new(r"C:\Apps\Xvert.exe");
        assert_eq!(sibling(exe, "old"), PathBuf::from(r"C:\Apps\Xvert.old.exe"));
    }

    #[test]
    fn checks_the_apps_own_repository() {
        assert_eq!(
            DEFAULT_SOURCE,
            format!(
                "https://github.com/Skale-Club/{}/releases/latest/download/latest.json",
                env!("CARGO_PKG_NAME")
            )
        );
    }

    #[test]
    fn feed_requires_a_new_stable_version_and_signed_https_package() {
        let manifest = |version: &str, url: &str, signature: &str| {
            serde_json::json!({ "version": version, "url": url, "signature": signature })
                .to_string()
        };
        assert!(parse_manifest(
            &manifest("0.2.0", "https://example.com/Xpace.exe", SIG),
            "0.1.0"
        )
        .unwrap()
        .is_some());
        assert!(parse_manifest(
            &manifest("0.1.0", "https://example.com/Xpace.exe", SIG),
            "0.1.0"
        )
        .unwrap()
        .is_none());
        assert!(parse_manifest(
            &manifest("0.2.0-beta", "https://example.com/Xpace.exe", SIG),
            "0.1.0"
        )
        .is_err());
        assert!(parse_manifest(
            &manifest("0.2.0", "http://example.com/Xpace.exe", SIG),
            "0.1.0"
        )
        .is_err());
        assert!(parse_manifest(
            &manifest("0.2.0", "https://example.com/Xpace.exe", "invalid"),
            "0.1.0"
        )
        .is_err());
        assert!(https("https://name:password@example.com/feed.json").is_err());
        assert!(https("file:///C:/update.exe").is_err());
    }

    #[test]
    fn unsigned_legacy_release_is_an_error() {
        let release = Release {
            tag_name: "999.0.0".into(),
            body: None,
            assets: vec![],
        };
        assert_eq!(parse_release(release).err().unwrap(), "unsignedUpdate");
    }

    #[test]
    fn applies_only_verified_staged_files_and_retains_data() {
        let root = std::env::temp_dir().join(format!("xpace-update-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Data")).unwrap();
        let exe = root.join("Xpace.exe");
        std::fs::write(&exe, b"old test executable").unwrap();
        std::fs::write(root.join("Data/settings.json"), b"keep settings").unwrap();
        let new = sibling(&exe, "new");
        std::fs::write(&new, DATA).unwrap();
        assert!(apply_at(&exe).is_none()); // no manifest, even if the name exists
        let manifest = Manifest {
            version: "999.0.0".into(),
            notes: "new version".into(),
            url: "https://example.com/Xpace.exe".into(),
            signature: SIG.into(),
        };
        std::fs::write(pending_path(&exe), serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(staged(&exe).is_ok());
        std::fs::write(&new, b"tampered").unwrap();
        assert!(apply_at(&exe).is_none());
        assert_eq!(std::fs::read(&exe).unwrap(), b"old test executable");
        std::fs::write(&new, DATA).unwrap();
        assert_eq!(apply_at(&exe), Some(exe.clone()));
        assert_eq!(std::fs::read(&exe).unwrap(), DATA);
        assert_eq!(
            std::fs::read(root.join("Data/settings.json")).unwrap(),
            b"keep settings"
        );
        assert!(!pending_path(&exe).exists());
        // This absolute target is a single disposable fixture created above.
        std::fs::remove_dir_all(&root).unwrap();
    }
}

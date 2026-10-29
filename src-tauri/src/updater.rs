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

const REPO: &str = concat!("Skale-Club/", env!("CARGO_PKG_NAME"));
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
    STATUS.lock().unwrap().clone()
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
    // A download left by an earlier session may be older than this build.
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(sibling(&exe, "new"));
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
    let staged = before.version.clone().filter(|_| before.state == State::Ready);
    set(|s| {
        if s.state != State::Ready {
            s.state = State::Checking;
        }
    });
    let outcome = check().and_then(|found| match found {
        Some(update) if Some(&update.version) != staged.as_ref() => stage(&update).map(|_| Some(update)),
        _ => Ok(None),
    });
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
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
    sig_url: String,
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
    let mut parts = text.trim().trim_start_matches('v').split(['.', '-', '+']);
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// The latest release, if it is newer than this build and carries a signed exe.
fn check() -> Result<Option<Update>, String> {
    let release: Release = match ureq::get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .call()
    {
        Ok(response) => response.into_json().map_err(|e| e.to_string())?,
        // No release published yet.
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let (Some(latest), Some(current)) = (version(&release.tag_name), version(env!("CARGO_PKG_VERSION"))) else {
        return Ok(None);
    };
    if latest <= current {
        return Ok(None);
    }
    let exe = release.assets.iter().find(|a| a.name.to_lowercase().ends_with(".exe"));
    let sig = exe.and_then(|e| release.assets.iter().find(|a| a.name == format!("{}.sig", e.name)));
    Ok(match (exe, sig) {
        (Some(exe), Some(sig)) => Some(Update {
            version: release.tag_name.trim_start_matches('v').to_string(),
            notes: release.body.unwrap_or_default(),
            exe_url: exe.browser_download_url.clone(),
            sig_url: sig.browser_download_url.clone(),
        }),
        _ => None,
    })
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let response = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(600))
        .call()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response.into_reader().take(MAX_DOWNLOAD).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Tauri stores keys and signatures as base64 of the minisign text files.
fn from_base64(text: &str) -> Result<String, String> {
    let raw = base64::engine::general_purpose::STANDARD.decode(text.trim()).map_err(|e| e.to_string())?;
    String::from_utf8(raw).map_err(|e| e.to_string())
}

fn verify(data: &[u8], signature: &str) -> Result<(), String> {
    let key = PublicKey::decode(&from_base64(PUBLIC_KEY)?).map_err(|e| e.to_string())?;
    let signature = Signature::decode(&from_base64(signature)?).map_err(|e| e.to_string())?;
    key.verify(data, &signature, true).map_err(|_| "the update's signature does not match".to_string())
}

fn sibling(exe: &Path, tag: &str) -> PathBuf {
    let stem = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    exe.with_file_name(format!("{stem}.{tag}.exe"))
}

/// Downloads and verifies the update, then keeps it beside the exe.
fn stage(update: &Update) -> Result<(), String> {
    set(|s| {
        s.state = State::Downloading;
        s.version = Some(update.version.clone());
    });
    let data = download(&update.exe_url)?;
    let signature = String::from_utf8(download(&update.sig_url)?).map_err(|e| e.to_string())?;
    verify(&data, &signature)?;
    let new = sibling(&std::env::current_exe().map_err(|e| e.to_string())?, "new");
    std::fs::write(&new, &data).map_err(|e| {
        let _ = std::fs::remove_file(&new);
        e.to_string()
    })
}

/// Swaps in the downloaded exe, if there is one, and returns the exe's path.
/// Call it when the app exits.
pub fn apply() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
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
    Some(exe)
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
        assert_eq!(REPO, format!("Skale-Club/{}", env!("CARGO_PKG_NAME")));
    }
}

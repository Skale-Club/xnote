//! Self-update from GitHub Releases, for a portable exe.
//!
//! Each release carries the exe and `<exe>.sig`, a minisign signature made in
//! CI with the Skale X update key (`tauri signer sign`). A download is only
//! used if its signature matches the public key built into this app, so a
//! tampered release or download is refused.
//!
//! Swapping: Windows lets a running exe be renamed, so the app renames itself
//! to `<name>.old.exe`, puts the new file in its place and starts it with
//! `--updated-from <pid>`. The new copy waits for the old one to exit, then
//! deletes `<name>.old.exe`. The `Data\` folder is left alone.

use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const REPO: &str = "skaleclub/xnote";
const PUBLIC_KEY: &str = include_str!("../updater.pub");
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;
const USER_AGENT: &str = "skale-x-updater";

#[derive(Clone, Serialize)]
pub struct Update {
    pub version: String,
    pub notes: String,
    #[serde(skip)]
    exe_url: String,
    #[serde(skip)]
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
pub fn check() -> Result<Option<Update>, String> {
    let release: Release = ureq::get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
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

/// Downloads, verifies and swaps in the new exe, then starts it. The caller
/// must exit right after.
pub fn install(update: &Update) -> Result<(), String> {
    let data = download(&update.exe_url)?;
    let signature = String::from_utf8(download(&update.sig_url)?).map_err(|e| e.to_string())?;
    verify(&data, &signature)?;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let (new, old) = (sibling(&exe, "new"), sibling(&exe, "old"));
    std::fs::write(&new, &data).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&old);
    if let Err(e) = std::fs::rename(&exe, &old) {
        let _ = std::fs::remove_file(&new);
        return Err(e.to_string());
    }
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe); // put the working copy back
        return Err(e.to_string());
    }
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
        let exe = Path::new(r"C:\Apps\Xnote.exe");
        assert_eq!(sibling(exe, "old"), PathBuf::from(r"C:\Apps\Xnote.old.exe"));
    }
}

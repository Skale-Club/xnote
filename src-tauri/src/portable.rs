//! Portable layout, in the PortableApps.com style: the exe sits at the top of
//! its folder and everything the app writes goes to `Data\` beside it, so the
//! whole folder can be copied or moved as one piece.

use std::fs;
use std::path::PathBuf;

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `Data\` next to the exe, created on first use.
pub fn data_dir() -> PathBuf {
    let dir = exe_dir().join("Data");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Moves what an older version kept next to the exe into `Data\`. Same
/// folder, so it is a rename, instant even for large files.
pub fn migrate(names: &[&str]) {
    let (from, to) = (exe_dir(), data_dir());
    for name in names {
        let (old, new) = (from.join(name), to.join(name));
        if old.exists() && !new.exists() {
            let _ = fs::rename(&old, &new);
        }
    }
}
/// Keeps the version in `App\AppInfo\appinfo.ini` (PortableApps metadata)
/// in step with this exe, since self-updates replace only the exe.
pub fn sync_appinfo() {
    let path = exe_dir().join(r"App\AppInfo\appinfo.ini");
    let Ok(text) = fs::read_to_string(&path) else { return };
    let version = env!("CARGO_PKG_VERSION");
    let updated: Vec<String> = text
        .lines()
        .map(|line| {
            if line.starts_with("PackageVersion=") {
                format!("PackageVersion={version}.0")
            } else if line.starts_with("DisplayVersion=") {
                format!("DisplayVersion={version}")
            } else {
                line.to_string()
            }
        })
        .collect();
    let updated = updated.join("\r\n") + "\r\n";
    if updated != text {
        let _ = fs::write(&path, updated);
    }
}
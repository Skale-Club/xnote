//! A small diagnostic log in `Data/logs/xnote.log` (rotated at 1 MB, three
//! files kept). It records what happened (recording started, a provider
//! error, an update) but never API keys, notes or transcript text.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOCK: Mutex<()> = Mutex::new(());
const MAX_BYTES: u64 = 1_000_000;

fn dir() -> PathBuf {
    crate::portable::data_dir().join("logs")
}

/// "2026-11-21 21:45:03" (UTC) from Unix seconds.
fn stamp(secs: u64) -> String {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}", rest / 3600, (rest / 60) % 60, rest % 60)
}

fn write(level: &str, message: &str) {
    let _guard = LOCK.lock();
    let dir = dir();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("xnote.log");
    if fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = fs::rename(dir.join("xnote.1.log"), dir.join("xnote.2.log"));
        let _ = fs::rename(&path, dir.join("xnote.1.log"));
    }
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let line = message.replace(['\r', '\n'], " ");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {level} {line}", stamp(secs));
    }
}

pub fn info(message: &str) {
    write("INFO", message)
}

pub fn error(message: &str) {
    write("ERROR", message)
}

/// The last `lines` lines, for the diagnostics.
pub fn tail(lines: usize) -> String {
    let text = fs::read_to_string(dir().join("xnote.log")).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_are_utc_dates() {
        assert_eq!(stamp(0), "1970-01-01 00:00:00");
        assert_eq!(stamp(951_782_400), "2000-02-29 00:00:00");
        assert_eq!(stamp(1_795_293_903), "2026-11-21 20:45:03");
    }
}

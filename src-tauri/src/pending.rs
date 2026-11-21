//! Speech segments whose cloud transcription failed (no internet, provider
//! down, quota) wait in `Data/pending/<meeting>/` as raw audio and are sent
//! again later, so a dropped connection does not lose part of a meeting.
//! Each file is deleted as soon as it is transcribed. Only audio that could
//! not be sent is kept, never a recording of the meeting.

use std::fs;
use std::path::{Path, PathBuf};

pub struct Item {
    pub meeting: String,
    /// Milliseconds from the start of the meeting.
    pub ms: u64,
    /// "me" or "them".
    pub who: String,
    path: PathBuf,
}

fn root() -> PathBuf {
    crate::portable::data_dir().join("pending")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.chars().all(|c| c.is_ascii_digit())
}

/// Keeps a segment to send later.
pub fn save(meeting: &str, ms: u64, who: &str, samples: &[f32]) -> Result<(), String> {
    if !valid_id(meeting) || !["me", "them"].contains(&who) {
        return Err("bad pending segment".into());
    }
    let dir = root().join(meeting);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    fs::write(dir.join(format!("{ms}-{who}.f32")), bytes).map_err(|e| e.to_string())
}

fn parse(meeting: &str, path: &Path) -> Option<Item> {
    let stem = path.file_name()?.to_str()?.strip_suffix(".f32")?;
    let (ms, who) = stem.split_once('-')?;
    if !["me", "them"].contains(&who) {
        return None;
    }
    Some(Item { meeting: meeting.into(), ms: ms.parse().ok()?, who: who.into(), path: path.to_path_buf() })
}

/// Everything waiting, oldest meeting and earliest segment first.
pub fn list() -> Vec<Item> {
    let mut items = Vec::new();
    let Ok(meetings) = fs::read_dir(root()) else { return items };
    for meeting in meetings.flatten() {
        let name = meeting.file_name().to_string_lossy().to_string();
        if !valid_id(&name) {
            continue;
        }
        let Ok(files) = fs::read_dir(meeting.path()) else { continue };
        items.extend(files.flatten().filter_map(|f| parse(&name, &f.path())));
    }
    items.sort_by(|a, b| (&a.meeting, a.ms).cmp(&(&b.meeting, b.ms)));
    items
}

pub fn count() -> usize {
    list().len()
}

pub fn samples(item: &Item) -> Result<Vec<f32>, String> {
    let bytes = fs::read(&item.path).map_err(|e| e.to_string())?;
    Ok(bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
}

/// Transcribed (or nothing was said): the audio is deleted.
pub fn done(item: &Item) {
    let _ = fs::remove_file(&item.path);
    if let Some(dir) = item.path.parent() {
        let _ = fs::remove_dir(dir); // only when empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_back_only_well_formed_names() {
        let dir = std::env::temp_dir();
        assert!(parse("1", &dir.join("5200-me.f32")).is_some_and(|i| i.ms == 5200 && i.who == "me"));
        assert!(parse("1", &dir.join("5200-boss.f32")).is_none());
        assert!(parse("1", &dir.join("x-me.f32")).is_none());
        assert!(parse("1", &dir.join("5200-me.wav")).is_none());
        assert!(save("../evil", 1, "me", &[0.0]).is_err());
        assert!(save("123", 1, "nobody", &[0.0]).is_err());
    }
}

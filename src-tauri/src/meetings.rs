//! Meetings are JSON files in `Data/meetings` beside the exe: the user's notes,
//! the transcript and the enhanced notes. Audio is never stored.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Line {
    /// Milliseconds from the start of the meeting.
    pub ms: u64,
    /// "me" (microphone) or "them" (computer audio).
    pub who: String,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Meeting {
    pub id: String,
    pub title: String,
    /// Unix time in milliseconds.
    pub created: u64,
    pub duration_ms: u64,
    pub notes: String,
    pub enhanced: String,
    pub transcript: Vec<Line>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    title: String,
    created: u64,
    duration_ms: u64,
    enhanced: bool,
    /// The file is there but can't be read; it is listed so it doesn't just vanish.
    broken: bool,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// File access is serialized so the transcriber appending lines and the
/// page saving notes never overwrite each other.
#[derive(Default)]
pub struct Store {
    lock: Mutex<()>,
}

impl Store {
    fn dir() -> PathBuf {
        crate::portable::data_dir().join("meetings")
    }

    /// Ids are digits only, so they can't point outside the folder.
    fn path(id: &str) -> Result<PathBuf, String> {
        if id.is_empty() || id.len() > 32 || !id.chars().all(|c| c.is_ascii_digit()) {
            return Err("bad meeting id".into());
        }
        Ok(Self::dir().join(format!("{id}.json")))
    }

    /// The file name decides which meeting this is, whatever id is written
    /// inside, so an edited or restored file can't redirect a save to another
    /// meeting. Speakers other than "me" and "them" become "them".
    fn read(id: &str) -> Result<Meeting, String> {
        let text = fs::read_to_string(Self::path(id)?).map_err(|e| e.to_string())?;
        Self::parse(id, &text)
    }

    fn parse(id: &str, text: &str) -> Result<Meeting, String> {
        let mut meeting: Meeting = serde_json::from_str(text).map_err(|e| e.to_string())?;
        meeting.id = id.to_string();
        for line in &mut meeting.transcript {
            if line.who != "me" {
                line.who = "them".into();
            }
        }
        Ok(meeting)
    }

    fn write(meeting: &Meeting) -> Result<(), String> {
        let path = Self::path(&meeting.id)?;
        fs::create_dir_all(Self::dir()).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("tmp");
        let text = serde_json::to_string_pretty(meeting).map_err(|e| e.to_string())?;
        fs::write(&tmp, text).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }

    pub fn create(&self) -> Result<Meeting, String> {
        let _guard = self.lock.lock().unwrap();
        let mut id = now_ms();
        while Self::path(&id.to_string())?.exists() {
            id += 1;
        }
        let meeting = Meeting { id: id.to_string(), created: now_ms(), ..Default::default() };
        Self::write(&meeting)?;
        Ok(meeting)
    }

    pub fn list(&self) -> Vec<Summary> {
        let _guard = self.lock.lock().unwrap();
        let Ok(entries) = fs::read_dir(Self::dir()) else { return Vec::new() };
        let mut list: Vec<Summary> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let id = name.strip_suffix(".json")?;
                Self::path(id).ok()?;
                Some(match Self::read(id) {
                    Ok(m) => Summary {
                        id: m.id,
                        title: m.title,
                        created: m.created,
                        duration_ms: m.duration_ms,
                        enhanced: !m.enhanced.is_empty(),
                        broken: false,
                    },
                    Err(_) => Summary {
                        id: id.to_string(),
                        title: String::new(),
                        created: id.parse().unwrap_or(0),
                        duration_ms: 0,
                        enhanced: false,
                        broken: true,
                    },
                })
            })
            .collect();
        list.sort_by(|a, b| b.created.cmp(&a.created));
        list
    }

    pub fn get(&self, id: &str) -> Result<Meeting, String> {
        let _guard = self.lock.lock().unwrap();
        Self::read(id)
    }

    /// Read-modify-write under the lock.
    pub fn update(&self, id: &str, change: impl FnOnce(&mut Meeting)) -> Result<Meeting, String> {
        let _guard = self.lock.lock().unwrap();
        let mut meeting = Self::read(id)?;
        change(&mut meeting);
        Self::write(&meeting)?;
        Ok(meeting)
    }

    /// Moves the file to `meetings\.trash`, so a mistake can be undone by hand.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let _guard = self.lock.lock().unwrap();
        let path = Self::path(id)?;
        let trash = Self::dir().join(".trash");
        fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
        fs::rename(&path, trash.join(format!("{id}.json"))).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::Store;

    #[test]
    fn rejects_ids_that_could_escape_the_folder() {
        assert!(Store::path(r"..\..\secret").is_err());
        assert!(Store::path("1730000000000").is_ok());
        assert!(Store::path("").is_err());
    }

    #[test]
    fn the_file_name_decides_the_id_and_speakers_are_checked() {
        let text = r#"{"id":"222","notes":"x","transcript":[{"ms":0,"who":"me","text":"a"},{"ms":1,"who":"x\" onclick=\"y","text":"b"}]}"#;
        let m = Store::parse("111", text).unwrap();
        assert_eq!(m.id, "111");
        assert_eq!(m.transcript[0].who, "me");
        assert_eq!(m.transcript[1].who, "them");
        assert!(Store::parse("111", "{not json").is_err());
    }
}

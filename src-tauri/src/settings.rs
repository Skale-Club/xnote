//! Settings kept in Data/xnote.json beside the executable. API keys are never
//! here: they live in the Windows Credential Manager (see providers.rs).

use crate::portable;
use crate::providers::{self, Custom, Provider};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub language: String,
    /// "auto", "pt", "en" or "es"; also the language of the enhanced notes.
    pub transcription_language: String,
    /// "local" (whisper.cpp on this computer) or "cloud"; empty until chosen.
    pub transcription: String,
    /// Chosen whisper.cpp build ("cuda" / "cpu") and model ("best" / "light").
    pub pack: String,
    pub model: String,
    /// Cloud transcription provider, and its model (empty: the provider's default).
    pub provider: String,
    pub transcription_model: String,
    /// Who writes the summaries. None: never chosen (older settings);
    /// Some(""): summaries are off and Xnote only transcribes.
    pub summary_provider: Option<String>,
    /// Empty: the provider's suggested model.
    pub summary_model: String,
    /// Write the summary when a recording stops.
    pub auto_summary: bool,
    /// OpenAI-compatible addresses the user added.
    pub custom_providers: Vec<Custom>,
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
            transcription_model: String::new(),
            summary_provider: None,
            summary_model: String::new(),
            auto_summary: true,
            custom_providers: Vec::new(),
        }
    }
}

impl Settings {
    /// Settings from before the cloud option have an engine but no mode.
    pub fn mode(&self) -> &str {
        if self.transcription.is_empty() && !self.pack.is_empty() {
            "local"
        } else {
            &self.transcription
        }
    }

    pub fn find(&self, id: &str) -> Option<Provider> {
        providers::find(id, &self.custom_providers)
    }

    /// The cloud transcription provider and model, when one is set up.
    pub fn cloud_transcriber(&self) -> Option<(Provider, String)> {
        let p = self.find(&self.provider).filter(|p| p.can_transcribe && providers::usable(p))?;
        let model =
            if self.transcription_model.is_empty() { p.transcribe_model.clone() } else { self.transcription_model.clone() };
        (!model.is_empty()).then_some((p, model))
    }

    /// The summary provider id; empty when summaries are off.
    pub fn summary_id(&self) -> &str {
        self.summary_provider.as_deref().unwrap_or("")
    }

    /// The model the summaries use.
    pub fn summary_model_of(&self, provider: &Provider) -> String {
        if self.summary_model.is_empty() {
            provider.summary_model.clone()
        } else {
            self.summary_model.clone()
        }
    }

    /// The summary provider and model, when summaries are on and usable.
    pub fn summarizer(&self) -> Option<(Provider, String)> {
        let p = self.find(self.summary_id()).filter(|p| p.can_summarize && providers::usable(p))?;
        let model = self.summary_model_of(&p);
        (!model.is_empty()).then_some((p, model))
    }

    /// Before providers were configurable, a saved OpenAI key meant
    /// "summarize with OpenAI". Keeps that for people who update.
    fn migrate(&mut self, openai_has_key: bool) -> bool {
        if self.summary_provider.is_some() {
            return false;
        }
        self.summary_provider = Some(if openai_has_key { "openai".into() } else { String::new() });
        true
    }
}

pub fn save(settings: &Settings) {
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(portable::data_dir().join("xnote.json"), text);
    }
}

pub fn load() -> Settings {
    let mut settings = match fs::read_to_string(portable::data_dir().join("xnote.json")) {
        // Some editors save UTF-8 with a BOM, which serde_json rejects.
        Ok(text) => serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_default(),
        Err(_) => Settings::default(),
    };
    let openai_has_key = || settings.find("openai").is_some_and(|p| providers::has_key(&p));
    let had_key = settings.summary_provider.is_none() && openai_has_key();
    if settings.migrate(had_key) || !portable::data_dir().join("xnote.json").exists() {
        save(&settings);
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_settings_keep_summarizing_with_openai() {
        let mut old: Settings = serde_json::from_str(r#"{"language":"pt","pack":"cpu","model":"best"}"#).unwrap();
        assert_eq!(old.mode(), "local");
        assert!(old.auto_summary);
        assert!(old.migrate(true));
        assert_eq!(old.summary_id(), "openai");
        assert!(!old.migrate(false), "migrates once");

        let mut fresh = Settings::default();
        fresh.migrate(false);
        assert_eq!(fresh.summary_id(), "", "summaries are off until chosen");
        assert!(fresh.summarizer().is_none());
    }

    #[test]
    fn models_fall_back_to_the_provider_suggestion() {
        let mut s = Settings { summary_provider: Some("groq".into()), ..Default::default() };
        let groq = s.find("groq").unwrap();
        assert_eq!(s.summary_model_of(&groq), groq.summary_model);
        s.summary_model = "my-model".into();
        assert_eq!(s.summary_model_of(&groq), "my-model");
    }
}

//! Transcription in the cloud (or on a compatible server the user runs), for
//! computers where running Whisper locally is not an option. Every provider
//! takes the same OpenAI-style request (a WAV file in a multipart form); only
//! the address, the model and the language field differ. Each speech
//! segment's audio goes to the provider; nothing is stored here.

use crate::audio::RATE;
use crate::engine::{is_phantom, multipart, wav};
use crate::providers::{self, Provider};
use std::time::Duration;

/// Form fields for one request. Whisper servers take a single `language`;
/// OpenAI's gpt-transcribe takes a `languages` list instead.
fn fields(provider: &Provider, model: &str, language: &str) -> Vec<(&'static str, String)> {
    let mut fields = vec![("model", model.to_string())];
    let openai = provider.id == "openai";
    if !openai {
        fields.push(("response_format", "json".into()));
        fields.push(("temperature", "0".into()));
    }
    if language != "auto" {
        fields.push((if openai { "languages[]" } else { "language" }, language.to_string()));
    }
    fields
}

/// Sends one segment; None when nothing real was said.
pub fn transcribe(provider: &Provider, model: &str, samples: &[f32], language: &str) -> Result<Option<String>, String> {
    let key = providers::request_key(provider).map_err(|_| "no transcription key".to_string())?;
    let fields = fields(provider, model, language);
    let fields: Vec<(&str, &str)> = fields.iter().map(|(n, v)| (*n, v.as_str())).collect();
    let (content_type, body) = multipart(&fields, &wav(samples));
    let url = format!("{}/audio/transcriptions", provider.base_url);
    let agent = providers::agent(Duration::from_secs(120));

    let response = providers::send_with_retry(provider, || {
        let request = agent.post(&url).set("Content-Type", &content_type);
        providers::authorize(provider, request, key.as_deref()).send_bytes(&body)
    })?;
    let response: serde_json::Value = response.into_json().map_err(|e| e.to_string())?;
    let text = response["text"].as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
    let seconds = samples.len() as f32 / RATE as f32;
    Ok((!is_phantom(&text, seconds)).then_some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_field_matches_each_provider() {
        let groq = providers::find("groq", &[]).unwrap();
        let fields_groq = fields(&groq, "whisper-large-v3-turbo", "pt");
        assert!(fields_groq.contains(&("language", "pt".into())));
        assert!(fields_groq.contains(&("model", "whisper-large-v3-turbo".into())));
        let openai = providers::find("openai", &[]).unwrap();
        let fields_openai = fields(&openai, "gpt-transcribe", "pt");
        assert!(fields_openai.contains(&("languages[]", "pt".into())));
        assert!(!fields_openai.iter().any(|(n, _)| *n == "response_format"));
        // Automatic: no language hint at all.
        assert!(!fields(&groq, "m", "auto").iter().any(|(n, _)| n.starts_with("language")));
    }
}

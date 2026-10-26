//! Transcription in the cloud, for computers where downloading and running
//! Whisper locally is not an option. Groq and OpenAI both take the same
//! OpenAI-style request (a WAV file in a multipart form), so one function
//! serves both; only the address, the model and the language field differ.
//! Each speech segment's audio goes to the provider; nothing is stored here.
//! The user's API keys live in the Windows Credential Manager.

use crate::audio::RATE;
use crate::engine::{is_phantom, multipart, wav};
use std::time::Duration;

pub struct Provider {
    pub id: &'static str,
    base: &'static str,
    model: &'static str,
}

pub const PROVIDERS: [Provider; 2] = [
    // Whisper large-v3 turbo, the same model the local engine uses.
    Provider { id: "groq", base: "https://api.groq.com/openai/v1", model: "whisper-large-v3-turbo" },
    Provider { id: "openai", base: "https://api.openai.com/v1", model: "gpt-transcribe" },
];

pub fn provider(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

fn credential(provider: &Provider) -> Result<keyring::Entry, String> {
    keyring::Entry::new("Xnote", &format!("{}-api-key", provider.id)).map_err(|e| e.to_string())
}

fn key_of(provider: &Provider) -> Option<String> {
    credential(provider).ok()?.get_password().ok()
}

pub fn has_key(provider: &Provider) -> bool {
    key_of(provider).is_some()
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(120))
        .user_agent("xnote")
        .build()
}

/// The provider's own error message, when it sent one.
fn api_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            let body: serde_json::Value = response.into_json().unwrap_or_default();
            let message = body["error"]["message"].as_str().unwrap_or("request failed");
            format!("{code}: {message}")
        }
        other => other.to_string(),
    }
}

/// Checks the key with a free call (listing models) before storing it.
pub fn set_key(provider: &Provider, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("empty key".into());
    }
    agent()
        .get(&format!("{}/models", provider.base))
        .set("Authorization", &format!("Bearer {key}"))
        .call()
        .map_err(api_error)?;
    credential(provider)?.set_password(key).map_err(|e| e.to_string())
}

pub fn clear_key(provider: &Provider) -> Result<(), String> {
    credential(provider)?.delete_credential().map_err(|e| e.to_string())
}

/// Form fields for one request. Groq takes Whisper's single `language`;
/// OpenAI's gpt-transcribe takes a `languages` list instead.
fn fields(provider: &Provider, language: &str) -> Vec<(&'static str, String)> {
    let mut fields = vec![("model", provider.model.to_string())];
    if provider.id == "groq" {
        fields.push(("response_format", "json".into()));
        fields.push(("temperature", "0".into()));
    }
    if language != "auto" {
        let name = if provider.id == "openai" { "languages[]" } else { "language" };
        fields.push((name, language.to_string()));
    }
    fields
}

/// Sends one segment; None when nothing real was said. Rate limits (429)
/// and brief outages are retried a few times, waiting as the provider asks.
pub fn transcribe(provider: &Provider, samples: &[f32], language: &str) -> Result<Option<String>, String> {
    let key = key_of(provider).ok_or("no transcription key")?;
    let fields = fields(provider, language);
    let fields: Vec<(&str, &str)> = fields.iter().map(|(n, v)| (*n, v.as_str())).collect();
    let (content_type, body) = multipart(&fields, &wav(samples));
    let url = format!("{}/audio/transcriptions", provider.base);

    let mut attempt = 0;
    let response: serde_json::Value = loop {
        attempt += 1;
        let result = agent()
            .post(&url)
            .set("Authorization", &format!("Bearer {key}"))
            .set("Content-Type", &content_type)
            .send_bytes(&body);
        match result {
            Ok(response) => break response.into_json().map_err(|e| e.to_string())?,
            Err(ureq::Error::Status(code, response)) if (code == 429 || code >= 500) && attempt < 4 => {
                let wait = response.header("retry-after").and_then(|v| v.parse::<f64>().ok()).unwrap_or(2.0 * attempt as f64);
                std::thread::sleep(Duration::from_secs_f64(wait.clamp(0.5, 30.0)));
            }
            Err(ureq::Error::Transport(_)) if attempt < 3 => std::thread::sleep(Duration::from_secs(2)),
            Err(e) => return Err(api_error(e)),
        }
    };
    let text = response["text"].as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
    let seconds = samples.len() as f32 / RATE as f32;
    Ok((!is_phantom(&text, seconds)).then_some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_field_matches_each_provider() {
        let groq = fields(provider("groq").unwrap(), "pt");
        assert!(groq.contains(&("language", "pt".into())));
        assert!(groq.contains(&("model", "whisper-large-v3-turbo".into())));
        let openai = fields(provider("openai").unwrap(), "pt");
        assert!(openai.contains(&("languages[]", "pt".into())));
        assert!(!openai.iter().any(|(n, _)| *n == "response_format"));
        // Automatic: no language hint at all.
        assert!(!fields(provider("groq").unwrap(), "auto").iter().any(|(n, _)| n.starts_with("language")));
    }
}

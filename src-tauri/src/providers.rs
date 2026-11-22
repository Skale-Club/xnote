//! The AI providers Xnote can use, for cloud transcription and for meeting
//! summaries. Almost every large provider speaks the OpenAI API, so one
//! adapter serves them all (Groq, OpenAI, OpenRouter, Gemini, Mistral,
//! DeepSeek, Ollama, LM Studio and any compatible address the user adds);
//! Anthropic has its own small adapter.
//!
//! API keys live in the Windows Credential Manager under "Xnote" /
//! "<provider id>-api-key", never in a file. Providers that run on this
//! computer (Ollama, LM Studio) need no key.

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Api {
    /// `/chat/completions`, `/audio/transcriptions` and `/models`.
    OpenAi,
    /// Anthropic's Messages API.
    Anthropic,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub api: Api,
    pub base_url: String,
    /// Runs on this computer: nothing leaves the PC.
    pub local: bool,
    pub needs_key: bool,
    pub can_transcribe: bool,
    pub can_summarize: bool,
    /// Where the user creates a key (or downloads the app, for local ones).
    pub key_page: String,
    /// Suggested models; the user can pick any model the provider lists.
    pub transcribe_model: String,
    pub summary_model: String,
    /// Added by the user in the Providers screen.
    pub custom: bool,
}

/// A provider the user added: any OpenAI-compatible address.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Custom {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub needs_key: bool,
    pub can_transcribe: bool,
}

struct BuiltIn {
    id: &'static str,
    name: &'static str,
    api: Api,
    base: &'static str,
    local: bool,
    transcribe: &'static str,
    summary: &'static str,
    key_page: &'static str,
}

const BUILT_IN: [BuiltIn; 9] = [
    BuiltIn {
        id: "groq",
        name: "Groq",
        api: Api::OpenAi,
        base: "https://api.groq.com/openai/v1",
        local: false,
        // Whisper large-v3 turbo, the same model the local engine uses.
        transcribe: "whisper-large-v3-turbo",
        summary: "llama-3.3-70b-versatile",
        key_page: "https://console.groq.com/keys",
    },
    BuiltIn {
        id: "openai",
        name: "OpenAI",
        api: Api::OpenAi,
        base: "https://api.openai.com/v1",
        local: false,
        transcribe: "gpt-transcribe",
        summary: "gpt-6.1-sol",
        key_page: "https://platform.openai.com/api-keys",
    },
    BuiltIn {
        id: "openrouter",
        name: "OpenRouter",
        api: Api::OpenAi,
        base: "https://openrouter.ai/api/v1",
        local: false,
        transcribe: "",
        // OpenRouter's router picks a model for each request.
        summary: "openrouter/auto",
        key_page: "https://openrouter.ai/settings/keys",
    },
    BuiltIn {
        id: "anthropic",
        name: "Anthropic (Claude)",
        api: Api::Anthropic,
        base: "https://api.anthropic.com/v1",
        local: false,
        transcribe: "",
        summary: "claude-sonnet-5-5",
        key_page: "https://console.anthropic.com/settings/keys",
    },
    BuiltIn {
        id: "gemini",
        name: "Google Gemini",
        api: Api::OpenAi,
        base: "https://generativelanguage.googleapis.com/v1beta/openai",
        local: false,
        transcribe: "",
        summary: "gemini-2.5-flash",
        key_page: "https://aistudio.google.com/apikey",
    },
    BuiltIn {
        id: "mistral",
        name: "Mistral",
        api: Api::OpenAi,
        base: "https://api.mistral.ai/v1",
        local: false,
        transcribe: "",
        summary: "mistral-large-latest",
        key_page: "https://console.mistral.ai/api-keys",
    },
    BuiltIn {
        id: "deepseek",
        name: "DeepSeek",
        api: Api::OpenAi,
        base: "https://api.deepseek.com/v1",
        local: false,
        transcribe: "",
        summary: "deepseek-chat",
        key_page: "https://platform.deepseek.com/api_keys",
    },
    BuiltIn {
        id: "ollama",
        name: "Ollama",
        api: Api::OpenAi,
        base: "http://localhost:11434/v1",
        local: true,
        transcribe: "",
        // Whatever the user has pulled; picked from the installed list.
        summary: "",
        key_page: "https://ollama.com/download",
    },
    BuiltIn {
        id: "lmstudio",
        name: "LM Studio",
        api: Api::OpenAi,
        base: "http://localhost:1234/v1",
        local: true,
        transcribe: "",
        summary: "",
        key_page: "https://lmstudio.ai",
    },
];

fn from_built_in(b: &BuiltIn) -> Provider {
    Provider {
        id: b.id.into(),
        name: b.name.into(),
        api: b.api,
        base_url: b.base.into(),
        local: b.local,
        needs_key: !b.local,
        can_transcribe: !b.transcribe.is_empty(),
        can_summarize: true,
        key_page: b.key_page.into(),
        transcribe_model: b.transcribe.into(),
        summary_model: b.summary.into(),
        custom: false,
    }
}

fn from_custom(c: &Custom) -> Provider {
    Provider {
        id: c.id.clone(),
        name: c.name.clone(),
        api: Api::OpenAi,
        base_url: c.base_url.clone(),
        local: is_local_url(&c.base_url),
        needs_key: c.needs_key,
        can_transcribe: c.can_transcribe,
        can_summarize: true,
        key_page: String::new(),
        transcribe_model: String::new(),
        summary_model: String::new(),
        custom: true,
    }
}

/// Built-in providers first, then the user's own.
pub fn all(custom: &[Custom]) -> Vec<Provider> {
    BUILT_IN.iter().map(from_built_in).chain(custom.iter().map(from_custom)).collect()
}

pub fn find(id: &str, custom: &[Custom]) -> Option<Provider> {
    all(custom).into_iter().find(|p| p.id == id)
}

// ---------- addresses ----------

fn is_local_host(host: &str) -> bool {
    host == "localhost" || host == "::1" || host == "[::1]" || host.starts_with("127.")
}

fn is_local_url(address: &str) -> bool {
    url::Url::parse(address).ok().and_then(|u| u.host_str().map(is_local_host)).unwrap_or(false)
}

/// Checks a user-entered base address. Plain http is accepted only for this
/// computer, so a key never crosses the network unencrypted.
pub fn clean_base_url(address: &str) -> Result<String, String> {
    let parsed = url::Url::parse(address.trim()).map_err(|_| "invalid address".to_string())?;
    let host = parsed.host_str().ok_or("invalid address")?;
    match parsed.scheme() {
        "https" => {}
        "http" if is_local_host(host) => {}
        "http" => return Err("https required".into()),
        _ => return Err("invalid address".into()),
    }
    if !parsed.username().is_empty() || parsed.password().is_some() || parsed.query().is_some() {
        return Err("invalid address".into());
    }
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

/// A stable id for a new custom provider, unique among `taken`.
pub fn custom_id(name: &str, taken: &[String]) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base = format!("custom-{}", if slug.is_empty() { "provider".into() } else { slug.chars().take(24).collect::<String>() });
    let builtin = |id: &str| BUILT_IN.iter().any(|b| b.id == id);
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) || builtin(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

// ---------- keys ----------

fn credential(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("Xnote", &format!("{id}-api-key")).map_err(|e| e.to_string())
}

pub fn key_of(provider: &Provider) -> Option<String> {
    credential(&provider.id).ok()?.get_password().ok()
}

/// Ready to use: a key is saved, or none is needed.
pub fn usable(provider: &Provider) -> bool {
    !provider.needs_key || key_of(provider).is_some()
}

pub fn has_key(provider: &Provider) -> bool {
    key_of(provider).is_some()
}

/// Checks the key with a free call (listing models) before storing it.
pub fn set_key(provider: &Provider, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("empty key".into());
    }
    models_with(provider, Some(key))?;
    credential(&provider.id)?.set_password(key).map_err(|e| e.to_string())
}

pub fn clear_key(provider: &Provider) -> Result<(), String> {
    match credential(&provider.id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// ---------- HTTP ----------

pub fn agent(read_timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(read_timeout)
        .user_agent(concat!("xnote/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// Adds the provider's authentication to a request.
pub fn authorize(provider: &Provider, request: ureq::Request, key: Option<&str>) -> ureq::Request {
    let Some(key) = key else { return request };
    match provider.api {
        Api::OpenAi => request.set("Authorization", &format!("Bearer {key}")),
        Api::Anthropic => request.set("x-api-key", key).set("anthropic-version", "2023-06-01"),
    }
}

/// The key to send: the saved one, or none for providers that need none.
pub fn request_key(provider: &Provider) -> Result<Option<String>, String> {
    match key_of(provider) {
        Some(key) => Ok(Some(key)),
        None if !provider.needs_key => Ok(None),
        None => Err("no API key".into()),
    }
}

/// The provider's own error message, when it sent one.
pub fn api_error(provider: &Provider, error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            let body: serde_json::Value = response.into_json().unwrap_or_default();
            let message = body["error"]["message"]
                .as_str()
                .or(body["error"].as_str())
                .or(body["message"].as_str())
                .unwrap_or("request failed");
            format!("{code}: {message}")
        }
        ureq::Error::Transport(_) if provider.local => "not running".into(),
        other => other.to_string(),
    }
}

/// Sends a request, retrying rate limits (429), brief outages (5xx) and
/// dropped connections a few times, waiting as the provider asks.
pub fn send_with_retry(
    provider: &Provider,
    send: impl Fn() -> Result<ureq::Response, ureq::Error>,
) -> Result<ureq::Response, String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match send() {
            Ok(response) => return Ok(response),
            Err(ureq::Error::Status(code, response)) if (code == 429 || code >= 500) && attempt < 4 => {
                let wait = response.header("retry-after").and_then(|v| v.parse::<f64>().ok()).unwrap_or(2.0 * attempt as f64);
                std::thread::sleep(Duration::from_secs_f64(wait.clamp(0.5, 30.0)));
            }
            Err(ureq::Error::Transport(_)) if attempt < 3 && !provider.local => std::thread::sleep(Duration::from_secs(2)),
            Err(e) => return Err(api_error(provider, e)),
        }
    }
}

/// Model ids from a `/models` response, without Gemini's "models/" prefix.
fn model_ids(body: &serde_json::Value) -> Vec<String> {
    let mut ids: Vec<String> = body["data"]
        .as_array()
        .or(body["models"].as_array())
        .map(|list| {
            list.iter()
                .filter_map(|m| m["id"].as_str().or(m["name"].as_str()))
                .map(|id| id.strip_prefix("models/").unwrap_or(id).to_string())
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids.dedup();
    ids
}

fn models_with(provider: &Provider, key: Option<&str>) -> Result<Vec<String>, String> {
    let request = agent(Duration::from_secs(20)).get(&format!("{}/models", provider.base_url));
    let response = authorize(provider, request, key).call().map_err(|e| api_error(provider, e))?;
    let body: serde_json::Value = response.into_json().map_err(|e| e.to_string())?;
    Ok(model_ids(&body))
}

/// The models the provider offers this account (or, for Ollama and LM
/// Studio, the ones installed). Also tells whether a local app is running.
pub fn models(provider: &Provider) -> Result<Vec<String>, String> {
    let key = request_key(provider)?;
    models_with(provider, key.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_ins_cover_transcription_and_summaries() {
        let list = all(&[]);
        let groq = list.iter().find(|p| p.id == "groq").unwrap();
        assert!(groq.can_transcribe && groq.can_summarize && groq.needs_key);
        let ollama = list.iter().find(|p| p.id == "ollama").unwrap();
        assert!(ollama.local && !ollama.needs_key && !ollama.can_transcribe);
        assert_eq!(find("anthropic", &[]).unwrap().api, Api::Anthropic);
        // Every built-in key page is https.
        assert!(list.iter().all(|p| p.key_page.starts_with("https://")));
    }

    #[test]
    fn custom_providers_follow_the_built_ins() {
        let custom = Custom {
            id: "custom-lab".into(),
            name: "Lab".into(),
            base_url: "http://127.0.0.1:8000/v1".into(),
            needs_key: false,
            can_transcribe: true,
        };
        let list = all(std::slice::from_ref(&custom));
        let last = list.last().unwrap();
        assert!(last.custom && last.local && last.can_transcribe);
        assert_eq!(find("custom-lab", &[custom]).unwrap().api, Api::OpenAi);
    }

    #[test]
    fn base_addresses_need_https_off_this_computer() {
        assert_eq!(clean_base_url("https://api.example.com/v1/").unwrap(), "https://api.example.com/v1");
        assert_eq!(clean_base_url(" http://localhost:11434/v1 ").unwrap(), "http://localhost:11434/v1");
        assert!(clean_base_url("http://127.0.0.1:8080").is_ok());
        assert_eq!(clean_base_url("http://192.168.0.10/v1").unwrap_err(), "https required");
        assert!(clean_base_url("ftp://example.com").is_err());
        assert!(clean_base_url("https://user:pw@example.com").is_err());
        assert!(clean_base_url("not a url").is_err());
    }

    #[test]
    fn custom_ids_are_unique_and_never_shadow_built_ins() {
        assert_eq!(custom_id("My Server!", &[]), "custom-my-server");
        assert_eq!(custom_id("My Server", &["custom-my-server".into()]), "custom-my-server-2");
        assert_eq!(custom_id("???", &[]), "custom-provider");
    }

    #[test]
    fn reads_model_lists_from_each_shape() {
        let openai = serde_json::json!({ "data": [{ "id": "b" }, { "id": "a" }, { "id": "a" }] });
        assert_eq!(model_ids(&openai), ["a", "b"]);
        let gemini = serde_json::json!({ "data": [{ "id": "models/gemini-2.5-flash" }] });
        assert_eq!(model_ids(&gemini), ["gemini-2.5-flash"]);
        assert!(model_ids(&serde_json::json!({})).is_empty());
    }
}

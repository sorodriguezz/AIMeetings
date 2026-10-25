//! One chat call to an OpenAI-compatible server: the bundled `llama-server`, Ollama (which speaks
//! the same `/v1/chat/completions`) or any server the user points at.

use std::time::Duration;

use serde::Deserialize;
use tokio_util::sync::CancellationToken;

/// Where a model answers.
#[derive(Debug, Clone)]
pub struct Endpoint {
    /// Up to, not including, `/v1`: `http://127.0.0.1:8080`.
    pub base: String,
    pub api_key: Option<String>,
}

impl Endpoint {
    pub fn new(url: &str, api_key: Option<String>) -> Endpoint {
        let trimmed = url.trim().trim_end_matches('/');
        let base = trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string();
        Endpoint { base, api_key: api_key.filter(|k| !k.trim().is_empty()) }
    }
}

pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub system: &'a str,
    pub user: &'a str,
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Default)]
pub struct ChatOutcome {
    pub text: String,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct Response {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

/// Sends one request and waits for the whole answer. `cancel` stops waiting (and drops the
/// connection, which llama-server and Ollama take as the end of the request).
pub async fn chat(endpoint: &Endpoint, request: &ChatRequest<'_>, cancel: &CancellationToken) -> Result<ChatOutcome, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        // A two-hour meeting's minutes on a laptop CPU take minutes, not seconds.
        .timeout(Duration::from_secs(30 * 60))
        .build()
        .map_err(|e| format!("Couldn't create the local model's HTTP client: {e}"))?;
    let body = serde_json::json!({
        "model": request.model,
        "messages": [
            { "role": "system", "content": request.system },
            { "role": "user", "content": request.user },
        ],
        "max_tokens": request.max_tokens,
        "temperature": request.temperature,
        "stream": false,
    });
    let mut call = client.post(format!("{}/v1/chat/completions", endpoint.base)).json(&body);
    if let Some(key) = &endpoint.api_key {
        call = call.bearer_auth(key);
    }
    let response = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err("cancelled".into()),
        response = call.send() => response.map_err(|e| format!("The local model didn't answer at {}: {e}", endpoint.base))?,
    };
    let status = response.status();
    let text = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err("cancelled".into()),
        text = response.text() => text.map_err(|e| format!("The local model's answer was cut off: {e}"))?,
    };
    if !status.is_success() {
        return Err(format!("The local model answered {status}: {}", error_sentence(&text)));
    }
    parse(&text)
}

fn parse(body: &str) -> Result<ChatOutcome, String> {
    let parsed: Response = serde_json::from_str(body).map_err(|e| format!("The local model's answer isn't what was expected: {e}"))?;
    let text = parsed.choices.into_iter().next().and_then(|c| c.message.content).unwrap_or_default();
    Ok(ChatOutcome {
        text: strip_thinking(&text).trim().to_string(),
        prompt_tokens: parsed.usage.as_ref().and_then(|u| u.prompt_tokens),
        completion_tokens: parsed.usage.as_ref().and_then(|u| u.completion_tokens),
    })
}

/// A reasoning model's `<think>…</think>` preamble, which some templates leave in the content.
fn strip_thinking(text: &str) -> &str {
    match text.find("</think>") {
        Some(end) if text.trim_start().starts_with("<think>") => &text[end + "</think>".len()..],
        _ => text,
    }
}

/// The message inside an OpenAI-style error body, or the body itself, cut short.
fn error_sentence(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(|m| m.as_str()).map(str::to_string))
        .unwrap_or_else(|| body.chars().take(300).collect())
}

/// The models a server lists (`/v1/models`), for the picker when the server is Ollama or the user's.
pub async fn list_models(endpoint: &Endpoint) -> Result<Vec<String>, String> {
    let client = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().map_err(|e| e.to_string())?;
    let mut call = client.get(format!("{}/v1/models", endpoint.base));
    if let Some(key) = &endpoint.api_key {
        call = call.bearer_auth(key);
    }
    let response = call.send().await.map_err(|e| format!("Nothing answered at {}: {e}", endpoint.base))?;
    if !response.status().is_success() {
        return Err(format!("{} answered {}", endpoint.base, response.status()));
    }
    let value: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(value
        .get("data")
        .and_then(|d| d.as_array())
        .map(|items| items.iter().filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(str::to_string)).collect())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_drop_a_trailing_v1_and_slash() {
        assert_eq!(Endpoint::new("http://127.0.0.1:11434/v1/", None).base, "http://127.0.0.1:11434");
        assert_eq!(Endpoint::new("https://api.example.com", Some(" ".into())).api_key, None);
    }

    #[test]
    fn answers_parse_and_lose_a_thinking_preamble() {
        let body = r#"{"choices":[{"message":{"content":"<think>hmm</think>\n## Resumen\n- a"}}],"usage":{"prompt_tokens":10,"completion_tokens":4}}"#;
        let outcome = parse(body).unwrap();
        assert_eq!(outcome.text, "## Resumen\n- a");
        assert_eq!(outcome.prompt_tokens, Some(10));
        assert_eq!(error_sentence(r#"{"error":{"message":"model not found"}}"#), "model not found");
    }
}

// gemini.rs — Gemini generateContent REST provider for voice transcription.
// Transport/provider failures are typed so the orchestration layer can safely
// distinguish transient overload from permanent auth/configuration problems.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::Client;
use serde_json::{json, Value};
use std::fmt;
use tracing::{debug, error, warn};

use crate::tracking::TokenUsage;

const MIN_TIMEOUT_SECS: u64 = 3;
const MAX_TIMEOUT_SECS: u64 = 180;
const ADAPTIVE_BASE_TIMEOUT_SECS: u64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeminiErrorKind {
    Configuration,
    Authentication,
    BadRequest,
    RateLimited,
    Unavailable,
    Timeout,
    Network,
    InvalidResponse,
}

#[derive(Debug)]
pub struct GeminiError {
    pub kind: GeminiErrorKind,
    message: String,
}

impl GeminiError {
    fn new(kind: GeminiErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn configuration(message: impl Into<String>) -> Self {
        Self::new(GeminiErrorKind::Configuration, message)
    }

    pub fn is_transient(&self) -> bool {
        matches!(
            self.kind,
            GeminiErrorKind::RateLimited
                | GeminiErrorKind::Unavailable
                | GeminiErrorKind::Timeout
                | GeminiErrorKind::Network
        )
    }
}

impl fmt::Display for GeminiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for GeminiError {}

pub struct GeminiProvider {
    pub api_key: String,
    pub model: String,
    timeout_secs: u64,
    custom_prompt: Option<String>,
}

impl GeminiProvider {
    pub fn new(
        api_key: String,
        model: String,
        timeout_secs: u64,
        custom_prompt: Option<String>,
    ) -> Self {
        Self {
            api_key,
            model,
            timeout_secs: timeout_secs.clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS),
            custom_prompt,
        }
    }

    pub async fn transcribe(
        &self,
        samples: &[i16],
        language: &str,
    ) -> Result<(String, TokenUsage), GeminiError> {
        if samples.is_empty() {
            return Err(GeminiError::configuration("No audio samples to transcribe"));
        }
        if self.api_key.trim().is_empty() {
            return Err(GeminiError::configuration(
                "Gemini API key is not configured",
            ));
        }

        let duration_secs = samples.len() as f64 / 16_000.0;
        let effective_timeout_secs = adaptive_timeout_secs(self.timeout_secs, duration_secs);
        debug!(
            samples = samples.len(),
            duration_secs = format!("{:.1}", duration_secs),
            configured_timeout_secs = self.timeout_secs,
            effective_timeout_secs,
            model = %self.model,
            "Sending audio to Gemini API"
        );

        let wav_bytes = encode_wav(samples);
        let wav_b64 = BASE64.encode(&wav_bytes);
        let model_name = crate::providers::model_catalog::normalize_model_id(&self.model);
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            model_name
        );
        let body = build_request_body(
            &wav_b64,
            language,
            self.custom_prompt.as_deref(),
            model_name,
        );

        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(effective_timeout_secs))
            .build()
            .map_err(|error| {
                GeminiError::new(
                    GeminiErrorKind::Configuration,
                    format!("Failed to build HTTP client: {error}"),
                )
            })?;

        // One request per model. The higher-level provider orchestration changes
        // model on transient overload instead of retrying the same 503 endpoint
        // four times and wasting the user's time.
        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(classify_reqwest_error)?;

        let status = response.status();
        let response_text = response.text().await.map_err(classify_reqwest_error)?;
        debug!(status = %status, body_len = response_text.len(), "API response received");

        if !status.is_success() {
            error!(status = %status, body = %truncate_str(&response_text, 500), model = %model_name, "Gemini API error");
            let code = status.as_u16();
            return Err(match code {
                401 | 403 => GeminiError::new(
                    GeminiErrorKind::Authentication,
                    format!("Gemini API key non valida o permessi insufficienti ({status})"),
                ),
                400..=499 if code != 429 => GeminiError::new(
                    GeminiErrorKind::BadRequest,
                    format!("Gemini ha rifiutato la richiesta ({status})"),
                ),
                429 => GeminiError::new(
                    GeminiErrorKind::RateLimited,
                    "Gemini rate limit raggiunto (429)",
                ),
                500..=599 => GeminiError::new(
                    GeminiErrorKind::Unavailable,
                    format!("Gemini temporaneamente non disponibile ({status})"),
                ),
                _ => GeminiError::new(
                    GeminiErrorKind::InvalidResponse,
                    format!("Gemini API returned {status}"),
                ),
            });
        }

        let parsed: Value = serde_json::from_str(&response_text).map_err(|error| {
            GeminiError::new(
                GeminiErrorKind::InvalidResponse,
                format!("Failed to parse Gemini API JSON response: {error}"),
            )
        })?;
        let usage = extract_usage(&parsed);
        let transcription = extract_text(&parsed)?;

        debug!(
            text_len = transcription.chars().count(),
            text_preview = %truncate_str(&transcription, 80),
            prompt_tokens = usage.prompt_tokens,
            audio_input_tokens = usage.audio_input_tokens,
            text_input_tokens = usage.text_input_tokens,
            output_tokens = usage.candidates_tokens,
            thought_tokens = usage.thoughts_tokens,
            "Transcription received"
        );

        Ok((transcription, usage))
    }
}

fn classify_reqwest_error(error: reqwest::Error) -> GeminiError {
    if error.is_timeout() {
        GeminiError::new(
            GeminiErrorKind::Timeout,
            format!("Timeout durante la richiesta Gemini: {error}"),
        )
    } else if error.is_connect() {
        GeminiError::new(
            GeminiErrorKind::Network,
            format!("Connessione a Gemini non disponibile: {error}"),
        )
    } else {
        GeminiError::new(
            GeminiErrorKind::Network,
            format!("Errore di rete durante la richiesta Gemini: {error}"),
        )
    }
}

fn adaptive_timeout_secs(configured_timeout_secs: u64, duration_secs: f64) -> u64 {
    let duration_budget = ADAPTIVE_BASE_TIMEOUT_SECS
        .saturating_add((duration_secs.max(0.0) * 0.25).ceil() as u64);
    configured_timeout_secs
        .max(duration_budget)
        .clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS)
}

fn build_request_body(
    wav_b64: &str,
    language: &str,
    custom_prompt: Option<&str>,
    model: &str,
) -> Value {
    let prompt = custom_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| crate::config::transcription_prompt(language));

    let mut generation_config = json!({
        "maxOutputTokens": 4096
    });

    // Starting with Gemini 3.6/3.5, temperature/top_p/top_k are deprecated.
    // For transcription we intentionally request the lowest supported Gemini 3
    // thinking level to reduce latency and billable thought tokens.
    if let Some(level) = crate::providers::model_catalog::find(model)
        .and_then(|spec| spec.thinking_level)
    {
        generation_config["thinkingConfig"] = json!({"thinkingLevel": level});
    }

    json!({
        "contents": [{
            "parts": [
                { "text": prompt },
                {
                    "inlineData": {
                        "mimeType": "audio/wav",
                        "data": wav_b64
                    }
                }
            ]
        }],
        "generationConfig": generation_config
    })
}

fn extract_usage(response: &Value) -> TokenUsage {
    let Some(meta) = response.get("usageMetadata") else {
        return TokenUsage::default();
    };

    let prompt_tokens = meta
        .get("promptTokenCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let candidates_tokens = meta
        .get("candidatesTokenCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let thoughts_tokens = meta
        .get("thoughtsTokenCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let mut audio_input_tokens = 0;
    let mut text_input_tokens = 0;
    if let Some(details) = meta.get("promptTokensDetails").and_then(Value::as_array) {
        for detail in details {
            let modality = detail
                .get("modality")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_uppercase();
            let count = detail
                .get("tokenCount")
                .or_else(|| detail.get("tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            match modality.as_str() {
                "AUDIO" => audio_input_tokens += count,
                "TEXT" => text_input_tokens += count,
                _ => {}
            }
        }
    }

    TokenUsage {
        prompt_tokens,
        candidates_tokens,
        thoughts_tokens,
        audio_input_tokens,
        text_input_tokens,
    }
}

fn extract_text(response: &Value) -> Result<String, GeminiError> {
    if let Some(error) = response.get("error") {
        let msg = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Unknown Gemini API error");
        return Err(GeminiError::new(
            GeminiErrorKind::InvalidResponse,
            format!("Gemini API error: {msg}"),
        ));
    }

    if let Some(candidates) = response.get("candidates").and_then(Value::as_array) {
        if let Some(first) = candidates.first() {
            if let Some(parts) = first
                .get("content")
                .and_then(|content| content.get("parts"))
                .and_then(Value::as_array)
            {
                let mut text = String::new();
                for part in parts {
                    if let Some(value) = part.get("text").and_then(Value::as_str) {
                        text.push_str(value);
                    }
                }
                if !text.is_empty() {
                    return Ok(text.trim().to_string());
                }
            }

            if let Some(reason) = first.get("finishReason").and_then(Value::as_str) {
                if reason != "STOP" {
                    warn!(reason, "Gemini response had non-STOP finish reason");
                }
            }
        }
    }

    warn!(
        response = %truncate_str(&response.to_string(), 300),
        "Could not extract text from Gemini response"
    );
    Ok(String::new())
}

pub fn encode_wav(samples: &[i16]) -> Vec<u8> {
    let num_channels: u16 = 1;
    let sample_rate: u32 = 16_000;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * (num_channels as u32) * (bits_per_sample as u32 / 8);
    let block_align = num_channels * (bits_per_sample / 8);
    let data_size = (samples.len() * 2) as u32;
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(44 + data_size as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    for &sample in samples {
        buf.extend_from_slice(&sample.to_le_bytes());
    }

    buf
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let prefix: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}…", prefix)
    } else {
        prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_wav_header() {
        let samples: Vec<i16> = vec![0; 1600];
        let wav = encode_wav(&samples);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    fn test_adaptive_timeout_scales_with_audio() {
        assert_eq!(adaptive_timeout_secs(10, 20.0), 25);
        assert_eq!(adaptive_timeout_secs(10, 60.0), 35);
        assert_eq!(adaptive_timeout_secs(10, 245.0), 82);
        assert_eq!(adaptive_timeout_secs(120, 20.0), 120);
    }

    #[test]
    fn latest_model_request_has_no_deprecated_sampling_params() {
        let body = build_request_body("dGVzdA==", "it", None, "gemini-3.6-flash");
        let config = &body["generationConfig"];
        assert!(config.get("temperature").is_none());
        assert!(config.get("topP").is_none());
        assert!(config.get("topK").is_none());
        assert_eq!(config["thinkingConfig"]["thinkingLevel"], "minimal");
    }

    #[test]
    fn usage_parses_audio_text_and_thought_tokens() {
        let response = json!({
            "usageMetadata": {
                "promptTokenCount": 330,
                "candidatesTokenCount": 40,
                "thoughtsTokenCount": 7,
                "totalTokenCount": 377,
                "promptTokensDetails": [
                    {"modality":"AUDIO","tokenCount":300},
                    {"modality":"TEXT","tokenCount":30}
                ]
            }
        });
        let usage = extract_usage(&response);
        assert_eq!(usage.audio_input_tokens, 300);
        assert_eq!(usage.text_input_tokens, 30);
        assert_eq!(usage.thoughts_tokens, 7);
    }

    #[test]
    fn error_kinds_have_correct_transience() {
        assert!(GeminiError::new(GeminiErrorKind::Unavailable, "503").is_transient());
        assert!(GeminiError::new(GeminiErrorKind::RateLimited, "429").is_transient());
        assert!(!GeminiError::new(GeminiErrorKind::Authentication, "403").is_transient());
        assert!(!GeminiError::configuration("bad model").is_transient());
    }

    #[test]
    fn test_truncate_str_utf8() {
        assert_eq!(truncate_str("A me così è già", 8), "A me cos…");
        assert_eq!(truncate_str("🙂🙂🙂", 2), "🙂🙂…");
    }
}

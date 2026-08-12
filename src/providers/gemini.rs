// gemini.rs — Gemini generateContent REST provider for voice transcription.
// Keeps transport failures separate from transcription text so errors are never
// injected into the user's focused application.

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde_json::{json, Value};
use tracing::{debug, error, warn};

use crate::tracking::TokenUsage;

const MIN_TIMEOUT_SECS: u64 = 3;
const MAX_TIMEOUT_SECS: u64 = 180;
const ADAPTIVE_BASE_TIMEOUT_SECS: u64 = 20;

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
}

fn http_client(timeout_secs: u64) -> Result<ClientWithMiddleware> {
    let reqwest_client = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .context("Failed to build HTTP client")?;

    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);

    Ok(ClientBuilder::new(reqwest_client)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build())
}

impl GeminiProvider {
    pub async fn transcribe(&self, samples: &[i16], language: &str) -> Result<(String, TokenUsage)> {
        if samples.is_empty() {
            bail!("No audio samples to transcribe");
        }
        if self.api_key.trim().is_empty() {
            bail!("Gemini API key is not configured");
        }

        let duration_secs = samples.len() as f64 / 16_000.0;
        let effective_timeout_secs = adaptive_timeout_secs(self.timeout_secs, duration_secs);
        debug!(
            samples = samples.len(),
            duration_secs = format!("{:.1}", duration_secs),
            configured_timeout_secs = self.timeout_secs,
            effective_timeout_secs,
            "Sending audio to Gemini API"
        );

        let wav_bytes = encode_wav(samples);
        let wav_b64 = BASE64.encode(&wav_bytes);

        debug!(
            wav_size = wav_bytes.len(),
            b64_size = wav_b64.len(),
            "Audio encoded as WAV"
        );

        let model_name = self.model.strip_prefix("models/").unwrap_or(&self.model);
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            model_name
        );
        let body = build_request_body(
            &wav_b64,
            language,
            self.custom_prompt.as_deref(),
        );

        debug!(model = %self.model, "Sending request to Gemini API");

        let client = http_client(effective_timeout_secs)?;
        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send()
            .await
            .context("HTTP request to Gemini API failed")?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .context("Failed to read API response body")?;

        debug!(status = %status, body_len = response_text.len(), "API response received");

        if !status.is_success() {
            error!(status = %status, body = %truncate_str(&response_text, 500), "Gemini API error");
            match status.as_u16() {
                401 | 403 => bail!("Gemini API key non valida o permessi insufficienti ({status})"),
                429 => bail!("Gemini rate limit raggiunto (429). Riprova tra qualche secondo"),
                500..=599 => bail!("Gemini temporaneamente non disponibile ({status})"),
                _ => bail!("Gemini API returned {status}"),
            }
        }

        let parsed: Value =
            serde_json::from_str(&response_text).context("Failed to parse Gemini API JSON response")?;
        let usage = extract_usage(&parsed);
        let transcription = extract_text(&parsed)?;

        debug!(
            text_len = transcription.chars().count(),
            text_preview = %truncate_str(&transcription, 80),
            prompt_tokens = usage.prompt_tokens,
            output_tokens = usage.candidates_tokens,
            "Transcription received"
        );

        Ok((transcription, usage))
    }
}

fn adaptive_timeout_secs(configured_timeout_secs: u64, duration_secs: f64) -> u64 {
    // Long recordings need materially more than the historical 10-second
    // request budget. Keep the profile setting as a lower bound, then add a
    // duration-aware network/model budget: 20s + 0.25s for each audio second.
    // Examples: 20s audio -> 25s, 60s -> 35s, 245s -> ~82s.
    let duration_budget = ADAPTIVE_BASE_TIMEOUT_SECS
        .saturating_add((duration_secs.max(0.0) * 0.25).ceil() as u64);
    configured_timeout_secs
        .max(duration_budget)
        .clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS)
}

fn build_request_body(wav_b64: &str, language: &str, custom_prompt: Option<&str>) -> Value {
    let prompt = custom_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| crate::config::transcription_prompt(language));

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
        "generationConfig": {
            "temperature": 0.0,
            "maxOutputTokens": 4096
        }
    })
}

fn extract_usage(response: &Value) -> TokenUsage {
    if let Some(meta) = response.get("usageMetadata") {
        TokenUsage {
            prompt_tokens: meta
                .get("promptTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            candidates_tokens: meta
                .get("candidatesTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            total_tokens: meta
                .get("totalTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        }
    } else {
        TokenUsage::default()
    }
}

fn extract_text(response: &Value) -> Result<String> {
    if let Some(error) = response.get("error") {
        let msg = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown Gemini API error");
        bail!("Gemini API error: {msg}");
    }

    if let Some(candidates) = response.get("candidates").and_then(|c| c.as_array()) {
        if let Some(first) = candidates.first() {
            if let Some(content) = first.get("content") {
                if let Some(parts) = content.get("parts").and_then(|p| p.as_array()) {
                    let mut text = String::new();
                    for part in parts {
                        if let Some(value) = part.get("text").and_then(|value| value.as_str()) {
                            text.push_str(value);
                        }
                    }
                    if !text.is_empty() {
                        return Ok(text.trim().to_string());
                    }
                }
            }

            if let Some(reason) = first.get("finishReason").and_then(|r| r.as_str()) {
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
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 3200);
    }

    #[test]
    fn test_encode_wav_total_size() {
        let samples: Vec<i16> = vec![100, -100, 0, i16::MAX, i16::MIN];
        let wav = encode_wav(&samples);
        assert_eq!(wav.len(), 44 + samples.len() * 2);
    }

    #[test]
    fn test_adaptive_timeout_scales_with_audio() {
        assert_eq!(adaptive_timeout_secs(10, 20.0), 25);
        assert_eq!(adaptive_timeout_secs(10, 60.0), 35);
        assert_eq!(adaptive_timeout_secs(10, 245.0), 82);
        assert_eq!(adaptive_timeout_secs(120, 20.0), 120);
        assert_eq!(adaptive_timeout_secs(10, 10_000.0), MAX_TIMEOUT_SECS);
    }

    #[test]
    fn test_build_request_body_default_prompt() {
        let body = build_request_body("dGVzdA==", "auto", None);
        assert_eq!(
            body["contents"][0]["parts"][1]["inlineData"]["mimeType"],
            "audio/wav"
        );
        assert_eq!(
            body["contents"][0]["parts"][1]["inlineData"]["data"],
            "dGVzdA=="
        );
        assert_eq!(body["generationConfig"]["temperature"], 0.0);
    }

    #[test]
    fn test_build_request_body_custom_prompt() {
        let body = build_request_body("dGVzdA==", "it", Some("Scrivi solo il testo"));
        assert_eq!(body["contents"][0]["parts"][0]["text"], "Scrivi solo il testo");
    }

    #[test]
    fn test_extract_text_success() {
        let response = json!({
            "candidates": [{
                "content": { "parts": [{"text": "ciao mondo"}] },
                "finishReason": "STOP"
            }]
        });
        assert_eq!(extract_text(&response).unwrap(), "ciao mondo");
    }

    #[test]
    fn test_extract_text_empty() {
        let response = json!({"candidates": [{"content": {"parts": []}}]});
        assert!(extract_text(&response).unwrap().is_empty());
    }

    #[test]
    fn test_extract_text_api_error_is_error() {
        let response = json!({"error": {"message": "API key invalid", "code": 403}});
        assert!(extract_text(&response).is_err());
    }

    #[test]
    fn test_truncate_str_ascii() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 5), "hello…");
    }

    #[test]
    fn test_truncate_str_utf8() {
        assert_eq!(truncate_str("A me così è già", 8), "A me cos…");
        assert_eq!(truncate_str("èèè", 2), "èè…");
        assert_eq!(truncate_str("🙂🙂🙂", 2), "🙂🙂…");
    }

    #[test]
    fn test_encode_wav_empty() {
        let samples: Vec<i16> = vec![];
        let wav = encode_wav(&samples);
        assert_eq!(wav.len(), 44);
        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 0);
    }

    #[test]
    fn test_extract_text_multipart() {
        let response = json!({
            "candidates": [{
                "content": {"parts": [{"text": "ciao "},{"text": "mondo"}]},
                "finishReason": "STOP"
            }]
        });
        assert_eq!(extract_text(&response).unwrap(), "ciao mondo");
    }

    #[test]
    fn test_extract_text_no_candidates() {
        assert!(extract_text(&json!({})).unwrap().is_empty());
    }

    #[test]
    fn test_extract_text_safety_block() {
        let response = json!({
            "candidates": [{"content": {"parts": []}, "finishReason": "SAFETY"}]
        });
        assert!(extract_text(&response).unwrap().is_empty());
    }
}

// network.rs — HTTP client for Gemini generateContent REST API.
// Records audio → encodes as WAV base64 → sends to Gemini → returns transcription text.
// No WebSocket, no streaming — simple and reliable.

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use reqwest::Client;
use serde_json::{Value, json};
use tracing::{debug, error, info, warn};

use crate::config::Config;

/// HTTP client singleton (reuses connections).
fn http_client() -> Result<Client> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("Failed to build HTTP client")
}

/// Transcribe audio samples using Gemini REST API.
///
/// - `config`: App config with API key and model.
/// - `samples`: All recorded PCM i16 16kHz mono samples.
///
/// Returns the transcription text.
pub async fn transcribe(config: &Config, samples: &[i16]) -> Result<String> {
    if samples.is_empty() {
        bail!("No audio samples to transcribe");
    }

    let duration_secs = samples.len() as f64 / 16_000.0;
    info!(
        samples = samples.len(),
        duration_secs = format!("{:.1}", duration_secs),
        "Transcribing audio via Gemini REST API..."
    );

    // Step 1: Encode PCM samples as WAV in memory
    let wav_bytes = encode_wav(samples);
    let wav_b64 = BASE64.encode(&wav_bytes);

    debug!(
        wav_size = wav_bytes.len(),
        b64_size = wav_b64.len(),
        "Audio encoded as WAV"
    );

    // Step 2: Build the API request
    let url = config.api_url();
    let body = build_request_body(&wav_b64);

    debug!(url = %url, "Sending request to Gemini API");

    // Step 3: Send HTTP POST
    let client = http_client()?;
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
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
        bail!(
            "Gemini API returned HTTP {}: {}",
            status,
            truncate_str(&response_text, 200)
        );
    }

    // Step 4: Parse the response
    let parsed: Value = serde_json::from_str(&response_text)
        .context("Failed to parse Gemini API JSON response")?;

    let transcription = extract_text(&parsed)?;

    info!(
        text_len = transcription.len(),
        text_preview = %truncate_str(&transcription, 80),
        "Transcription received"
    );

    Ok(transcription)
}

/// Build the JSON body for Gemini generateContent with inline audio.
fn build_request_body(wav_b64: &str) -> Value {
    json!({
        "contents": [{
            "parts": [
                {
                    "text": "Trascrivi esattamente ciò che viene detto in questo audio, parola per parola. Non aggiungere commenti, non rispondere a domande, non inventare punteggiatura. Restituisci SOLO il testo dettato. Se l'audio è silenzioso o incomprensibile, rispondi con una stringa vuota."
                },
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

/// Extract text from Gemini generateContent response.
fn extract_text(response: &Value) -> Result<String> {
    // Standard response format:
    // { "candidates": [{ "content": { "parts": [{ "text": "..." }] } }] }
    if let Some(candidates) = response.get("candidates").and_then(|c| c.as_array()) {
        if let Some(first) = candidates.first() {
            if let Some(content) = first.get("content") {
                if let Some(parts) = content.get("parts").and_then(|p| p.as_array()) {
                    let mut text = String::new();
                    for part in parts {
                        if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                            text.push_str(t);
                        }
                    }
                    if !text.is_empty() {
                        return Ok(text.trim().to_string());
                    }
                }
            }

            // Check for safety/block reasons
            if let Some(reason) = first.get("finishReason").and_then(|r| r.as_str()) {
                if reason != "STOP" {
                    warn!(reason, "Gemini response had non-STOP finish reason");
                }
            }
        }
    }

    // Check for error in response
    if let Some(error) = response.get("error") {
        let msg = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown API error");
        bail!("Gemini API error: {}", msg);
    }

    warn!(
        response = %truncate_str(&response.to_string(), 300),
        "Could not extract text from Gemini response"
    );
    Ok(String::new())
}

/// Encode PCM i16 mono 16kHz samples as a WAV file in memory.
pub fn encode_wav(samples: &[i16]) -> Vec<u8> {
    let num_channels: u16 = 1;
    let sample_rate: u32 = 16_000;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * (num_channels as u32) * (bits_per_sample as u32 / 8);
    let block_align = num_channels * (bits_per_sample / 8);
    let data_size = (samples.len() * 2) as u32;
    let file_size = 36 + data_size; // RIFF header is 44 bytes, file_size = total - 8

    let mut buf = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // sub-chunk size (PCM = 16)
    buf.extend_from_slice(&1u16.to_le_bytes()); // audio format (PCM = 1)
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data sub-chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    // PCM samples (little-endian i16)
    for &sample in samples {
        buf.extend_from_slice(&sample.to_le_bytes());
    }

    buf
}

/// Truncate a string for display.
fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_wav_header() {
        let samples: Vec<i16> = vec![0; 1600]; // 100ms at 16kHz
        let wav = encode_wav(&samples);

        // Check RIFF header
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");

        // Data size should be samples * 2 bytes
        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 3200); // 1600 samples * 2 bytes
    }

    #[test]
    fn test_encode_wav_total_size() {
        let samples: Vec<i16> = vec![100, -100, 0, i16::MAX, i16::MIN];
        let wav = encode_wav(&samples);
        assert_eq!(wav.len(), 44 + samples.len() * 2); // 44 header + data
    }

    #[test]
    fn test_build_request_body() {
        let body = build_request_body("dGVzdA==");
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
    fn test_extract_text_success() {
        let response = json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "ciao mondo"}]
                },
                "finishReason": "STOP"
            }]
        });
        let text = extract_text(&response).unwrap();
        assert_eq!(text, "ciao mondo");
    }

    #[test]
    fn test_extract_text_empty() {
        let response = json!({
            "candidates": [{
                "content": {
                    "parts": []
                }
            }]
        });
        let text = extract_text(&response).unwrap();
        assert!(text.is_empty());
    }

    #[test]
    fn test_extract_text_api_error() {
        let response = json!({
            "error": {
                "message": "API key invalid",
                "code": 403
            }
        });
        let result = extract_text(&response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("API key invalid"));
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 5), "hello…");
    }
}

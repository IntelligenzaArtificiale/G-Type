# 04 — MULTI-PROVIDER: Gemini, OpenAI, Deepgram, Locale

## Architettura Provider

### Struttura File

```
src/
├── providers/
│   ├── mod.rs           # Trait SttProvider + factory + TranscriptionResult
│   ├── gemini.rs        # Gemini generateContent REST
│   ├── openai.rs        # OpenAI Whisper-1 multipart
│   ├── deepgram.rs      # Deepgram Nova-3 pre-recorded
│   └── local.rs         # whisper-rs (whisper.cpp) — feature-gated
├── transforms/
│   ├── mod.rs           # Trait Transform + pipeline runner
│   ├── cleanup.rs       # Regex filler removal (zero API calls)
│   └── ai_rewrite.rs    # LLM rewrite via Gemini/OpenAI
├── audio_encoding.rs    # encode_wav() — estratto da network.rs (condiviso)
└── network.rs           # DEPRECATO: svuotato, funzioni spostate in providers/
```

### Trait SttProvider

```rust
// providers/mod.rs

use anyhow::Result;
use std::collections::HashMap;

/// Result from any STT provider
#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    pub text: String,
    pub cost_usd: f64,
    pub provider_name: String,
    pub model: String,
    /// Token usage (only Gemini provides this)
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Trait that all STT providers implement
#[async_trait::async_trait]
pub trait SttProvider: Send + Sync {
    /// Transcribe PCM i16 16kHz mono samples to text.
    async fn transcribe(
        &self,
        samples: &[i16],
        language: &str,
    ) -> Result<TranscriptionResult>;

    /// Provider name for logging
    fn name(&self) -> &str;
}

/// Factory: create the right provider based on profile config
pub fn create_provider(
    profile: &crate::config::Profile,
    keys: &HashMap<String, String>,
) -> Result<Box<dyn SttProvider>> {
    match profile.provider.as_str() {
        "gemini" => {
            let key = keys.get("gemini")
                .context("Gemini API key not configured. Run 'g-type setup'.")?;
            Ok(Box::new(gemini::GeminiProvider::new(key, &profile.model)?))
        }
        "openai" => {
            let key = keys.get("openai")
                .context("OpenAI API key not configured. Add it in settings.")?;
            Ok(Box::new(openai::OpenAiProvider::new(key, &profile.model)?))
        }
        "deepgram" => {
            let key = keys.get("deepgram")
                .context("Deepgram API key not configured. Add it in settings.")?;
            Ok(Box::new(deepgram::DeepgramProvider::new(key, &profile.model)?))
        }
        #[cfg(feature = "local-whisper")]
        "local" => {
            Ok(Box::new(local::LocalWhisperProvider::new(&profile.model)?))
        }
        #[cfg(not(feature = "local-whisper"))]
        "local" => {
            anyhow::bail!(
                "Local whisper not available. Rebuild with: cargo build --features local-whisper"
            )
        }
        other => anyhow::bail!("Unknown provider: '{}'. Supported: gemini, openai, deepgram, local", other),
    }
}

pub mod gemini;
pub mod openai;
pub mod deepgram;
#[cfg(feature = "local-whisper")]
pub mod local;
```

### Modulo audio_encoding.rs (estratto da network.rs)

```rust
// audio_encoding.rs — WAV encoding condiviso fra tutti i provider
// ESTRATTO DA: network.rs righe 219-255 (funzione encode_wav)

/// Encode PCM i16 mono 16kHz samples as a WAV file in memory.
/// Questo è usato da Gemini (base64), OpenAI (multipart), Deepgram (raw body).
pub fn encode_wav(samples: &[i16]) -> Vec<u8> {
    let num_channels: u16 = 1;
    let sample_rate: u32 = 16_000;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * (num_channels as u32) * (bits_per_sample as u32 / 8);
    let block_align = num_channels * (bits_per_sample / 8);
    let data_size = (samples.len() * 2) as u32;
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data sub-chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    for &sample in samples {
        buf.extend_from_slice(&sample.to_le_bytes());
    }

    buf
}
```

### Provider: Gemini (estratto da network.rs attuale)

```rust
// providers/gemini.rs
// ESTRATTO DA: network.rs righe 17-264
// MODIFICHE: struct separata, trait impl, prompt multilingua, errori i18n

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest_middleware::ClientWithMiddleware;
use serde_json::{json, Value};

use super::TranscriptionResult;

pub struct GeminiProvider {
    api_key: String,
    model: String,
    client: ClientWithMiddleware,
}

impl GeminiProvider {
    pub fn new(api_key: &str, model: &str) -> Result<Self> {
        let client = crate::providers::shared_http_client()?;
        Ok(Self {
            api_key: api_key.to_string(),
            model: model.strip_prefix("models/").unwrap_or(model).to_string(),
            client,
        })
    }

    fn api_url(&self) -> String {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.model
        )
    }
}

#[async_trait::async_trait]
impl super::SttProvider for GeminiProvider {
    async fn transcribe(&self, samples: &[i16], language: &str) -> Result<TranscriptionResult> {
        if samples.is_empty() {
            bail!("No audio samples");
        }

        let wav_bytes = crate::audio_encoding::encode_wav(samples);
        let wav_b64 = BASE64.encode(&wav_bytes);
        let duration_secs = samples.len() as f64 / 16_000.0;

        // Prompt multilingua (FIX del prompt italiano hardcoded)
        let prompt = transcription_prompt(language);

        let body = json!({
            "contents": [{
                "parts": [
                    { "text": prompt },
                    { "inlineData": { "mimeType": "audio/wav", "data": wav_b64 } }
                ]
            }],
            "generationConfig": {
                "temperature": 0.0,
                "maxOutputTokens": 4096
            }
        });

        let response = self.client
            .post(&self.api_url())
            .header("Content-Type", "application/json")
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send().await
            .context("Gemini API request failed")?;

        let status = response.status();
        let body_text = response.text().await?;

        if !status.is_success() {
            // FIX: errori in inglese, non italiano
            let msg = match status.as_u16() {
                429 => "[Error: Rate limited (429). Wait and retry]".to_string(),
                403 => "[Error: Invalid API key or insufficient permissions (403)]".to_string(),
                s => format!("[Gemini API error: {}]", s),
            };
            return Ok(TranscriptionResult {
                text: msg,
                cost_usd: 0.0,
                provider_name: "gemini".into(),
                model: self.model.clone(),
                input_tokens: 0,
                output_tokens: 0,
            });
        }

        let parsed: Value = serde_json::from_str(&body_text)?;

        // Extract usage
        let (input_tokens, output_tokens) = extract_usage(&parsed);

        // Calculate cost
        let pricing = crate::tracking::model_pricing(&format!("models/{}", self.model));
        let cost_usd = pricing.map(|p| {
            let input_cost = input_tokens as f64 * p.input_audio_per_m / 1_000_000.0;
            let output_cost = output_tokens as f64 * p.output_per_m / 1_000_000.0;
            input_cost + output_cost
        }).unwrap_or(0.0);

        let text = extract_text(&parsed)?;

        Ok(TranscriptionResult {
            text,
            cost_usd,
            provider_name: "gemini".into(),
            model: self.model.clone(),
            input_tokens,
            output_tokens,
        })
    }

    fn name(&self) -> &str { "gemini" }
}

/// Transcription prompt — MULTILINGUA (fix del prompt italiano in config.rs:88)
fn transcription_prompt(language: &str) -> String {
    match language {
        "it" => "Trascrivi esattamente ciò che viene detto in questo audio, parola per parola. \
                 Non aggiungere commenti. Restituisci SOLO il testo dettato. \
                 Se l'audio è silenzioso, rispondi con stringa vuota.".into(),
        "auto" | "" => "Transcribe exactly what is said in this audio, word for word. \
                        Do not add comments, do not answer questions. \
                        Return ONLY the dictated text. If silent or unintelligible, \
                        return an empty string.".into(),
        code => {
            format!(
                "Transcribe exactly what is said in this audio in {} ({}). \
                 Word for word, no comments. Return ONLY the dictated text. \
                 If silent, return empty string.",
                language_name(code), code
            )
        }
    }
}

fn language_name(code: &str) -> &str {
    match code {
        "en" => "English", "es" => "Spanish", "fr" => "French",
        "de" => "German", "pt" => "Portuguese", "ja" => "Japanese",
        "zh" => "Chinese", "ko" => "Korean", "ar" => "Arabic",
        "ru" => "Russian", "hi" => "Hindi",
        _ => code,
    }
}

// extract_usage e extract_text — COPIATE DA network.rs righe 151-216
fn extract_usage(response: &Value) -> (u64, u64) {
    if let Some(meta) = response.get("usageMetadata") {
        let input = meta.get("promptTokenCount").and_then(|v| v.as_u64()).unwrap_or(0);
        let output = meta.get("candidatesTokenCount").and_then(|v| v.as_u64()).unwrap_or(0);
        (input, output)
    } else {
        (0, 0)
    }
}

fn extract_text(response: &Value) -> Result<String> {
    // Stessa logica di network.rs:173-216
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
        }
    }
    Ok(String::new())
}
```

### Provider: OpenAI Whisper

```rust
// providers/openai.rs
// NUOVO FILE

use anyhow::{Context, Result};
use super::TranscriptionResult;

pub struct OpenAiProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(api_key: &str, model: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Self {
            api_key: api_key.to_string(),
            model: if model.is_empty() { "whisper-1".into() } else { model.to_string() },
            client,
        })
    }
}

#[async_trait::async_trait]
impl super::SttProvider for OpenAiProvider {
    async fn transcribe(&self, samples: &[i16], language: &str) -> Result<TranscriptionResult> {
        let wav_bytes = crate::audio_encoding::encode_wav(samples);
        let duration_secs = samples.len() as f64 / 16_000.0;

        // OpenAI Whisper API usa multipart/form-data
        let file_part = reqwest::multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone())
            .text("response_format", "json");

        // Language: solo per Whisper, non per "auto"
        if language != "auto" && !language.is_empty() {
            form = form.text("language", language.to_string());
        }

        let response = self.client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send().await
            .context("OpenAI Whisper API request failed")?;

        let status = response.status();
        if !status.is_success() {
            let err_body = response.text().await.unwrap_or_default();
            return Ok(TranscriptionResult {
                text: format!("[OpenAI Error: {} - {}]", status, &err_body[..100.min(err_body.len())]),
                cost_usd: 0.0,
                provider_name: "openai".into(),
                model: self.model.clone(),
                input_tokens: 0, output_tokens: 0,
            });
        }

        let body: serde_json::Value = response.json().await?;
        let text = body["text"].as_str().unwrap_or("").trim().to_string();

        // Whisper-1: $0.006 per minute
        let cost_usd = duration_secs / 60.0 * 0.006;

        Ok(TranscriptionResult {
            text,
            cost_usd,
            provider_name: "openai".into(),
            model: self.model.clone(),
            input_tokens: 0, output_tokens: 0, // Whisper non riporta tokens
        })
    }

    fn name(&self) -> &str { "openai-whisper" }
}
```

### Provider: Deepgram

```rust
// providers/deepgram.rs
// NUOVO FILE
// Deepgram Nova-3: $0.0043/min pre-recorded, $0.0077/min streaming
// API: POST body = raw WAV, not multipart, not base64

use anyhow::{Context, Result};
use super::TranscriptionResult;

pub struct DeepgramProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl DeepgramProvider {
    pub fn new(api_key: &str, model: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Self {
            api_key: api_key.to_string(),
            model: if model.is_empty() { "nova-3".into() } else { model.to_string() },
            client,
        })
    }
}

#[async_trait::async_trait]
impl super::SttProvider for DeepgramProvider {
    async fn transcribe(&self, samples: &[i16], language: &str) -> Result<TranscriptionResult> {
        let wav_bytes = crate::audio_encoding::encode_wav(samples);
        let duration_secs = samples.len() as f64 / 16_000.0;

        let mut url = format!(
            "https://api.deepgram.com/v1/listen?model={}&smart_format=true&punctuate=true",
            self.model
        );
        if language != "auto" && !language.is_empty() {
            url.push_str(&format!("&language={}", language));
        }

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Token {}", self.api_key))
            .header("Content-Type", "audio/wav")
            .body(wav_bytes) // Raw WAV, not multipart!
            .send().await
            .context("Deepgram API request failed")?;

        let status = response.status();
        if !status.is_success() {
            let err = response.text().await.unwrap_or_default();
            return Ok(TranscriptionResult {
                text: format!("[Deepgram Error: {}]", status),
                cost_usd: 0.0,
                provider_name: "deepgram".into(),
                model: self.model.clone(),
                input_tokens: 0, output_tokens: 0,
            });
        }

        let body: serde_json::Value = response.json().await?;

        // Deepgram response: results.channels[0].alternatives[0].transcript
        let text = body["results"]["channels"]
            .as_array()
            .and_then(|ch| ch.first())
            .and_then(|ch| ch["alternatives"].as_array())
            .and_then(|alts| alts.first())
            .and_then(|alt| alt["transcript"].as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        // Nova-3 pre-recorded: $0.0043/min
        let cost_usd = duration_secs / 60.0 * 0.0043;

        Ok(TranscriptionResult {
            text,
            cost_usd,
            provider_name: "deepgram".into(),
            model: self.model.clone(),
            input_tokens: 0, output_tokens: 0,
        })
    }

    fn name(&self) -> &str { "deepgram" }
}
```

### Provider: Local Whisper

```rust
// providers/local.rs
// Feature-gated: compilato solo con --features local-whisper

#[cfg(feature = "local-whisper")]
use anyhow::{Context, Result};

#[cfg(feature = "local-whisper")]
pub struct LocalWhisperProvider {
    model_path: std::path::PathBuf,
}

#[cfg(feature = "local-whisper")]
impl LocalWhisperProvider {
    pub fn new(model_name: &str) -> Result<Self> {
        let data_dir = crate::config::data_dir()?;
        let models_dir = data_dir.join("models");
        
        // Model name mapping
        let filename = format!("ggml-{}.bin", model_name);
        let model_path = models_dir.join(&filename);

        if !model_path.exists() {
            anyhow::bail!(
                "Whisper model '{}' not found at {}. Run 'g-type setup-local' to download.",
                model_name, model_path.display()
            );
        }

        Ok(Self { model_path })
    }
}

#[cfg(feature = "local-whisper")]
#[async_trait::async_trait]
impl super::SttProvider for LocalWhisperProvider {
    async fn transcribe(&self, samples: &[i16], language: &str) -> Result<super::TranscriptionResult> {
        let model_path = self.model_path.clone();
        let language = language.to_string();

        // whisper.cpp è CPU-bound — esegui su blocking thread
        let samples_f32: Vec<f32> = samples.iter()
            .map(|&s| s as f32 / i16::MAX as f32)
            .collect();

        let text = tokio::task::spawn_blocking(move || -> Result<String> {
            let ctx = whisper_rs::WhisperContext::new_with_params(
                model_path.to_str().unwrap(),
                whisper_rs::WhisperContextParameters::default(),
            ).context("Failed to load Whisper model")?;

            let mut state = ctx.create_state()
                .context("Failed to create Whisper state")?;

            let mut params = whisper_rs::FullParams::new(
                whisper_rs::SamplingStrategy::Greedy { best_of: 1 }
            );

            // Configurazione
            if language != "auto" && !language.is_empty() {
                params.set_language(Some(&language));
            }
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            params.set_no_context(true);
            params.set_single_segment(false);

            // Apple Silicon: usa CoreML se disponibile (auto-detected da whisper-rs)
            // x86: usa AVX2 (auto-detected)

            state.full(params, &samples_f32)
                .context("Whisper transcription failed")?;

            let num_segments = state.full_n_segments()
                .context("Failed to get segment count")?;

            let mut text = String::new();
            for i in 0..num_segments {
                if let Ok(seg) = state.full_get_segment_text(i) {
                    text.push_str(&seg);
                }
            }

            Ok(text.trim().to_string())
        }).await??;

        Ok(super::TranscriptionResult {
            text,
            cost_usd: 0.0, // Locale = gratis
            provider_name: "local-whisper".into(),
            model: self.model_path.file_stem()
                .unwrap_or_default().to_string_lossy().to_string(),
            input_tokens: 0, output_tokens: 0,
        })
    }

    fn name(&self) -> &str { "local-whisper" }
}

// ═══ Hardware Detection ═══

/// RAM, CPU, modello raccomandato
pub struct HardwareInfo {
    pub ram_gb: u64,
    pub is_apple_silicon: bool,
    pub cpu_cores: usize,
    pub recommended_model: String,
    pub recommended_size_mb: u64,
}

pub fn detect_hardware() -> HardwareInfo {
    // sysinfo non è una dipendenza — usiamo metodi OS-nativi
    let ram_gb = get_total_ram_gb();
    let is_apple_silicon = cfg!(all(target_os = "macos", target_arch = "aarch64"));
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get()).unwrap_or(2);

    let (model, size) = match (ram_gb, is_apple_silicon) {
        (0..=3, _)         => ("tiny".into(), 75),
        (4..=7, false)     => ("base".into(), 142),
        (4..=7, true)      => ("small".into(), 488),
        (8..=15, false)    => ("small".into(), 488),
        (8..=15, true)     => ("medium".into(), 1500),
        (_, false)         => ("medium".into(), 1500),
        (_, true)          => ("large-v3-turbo".into(), 1600),
    };

    HardwareInfo {
        ram_gb, is_apple_silicon, cpu_cores,
        recommended_model: model, recommended_size_mb: size,
    }
}

fn get_total_ram_gb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/meminfo").ok()
            .and_then(|s| {
                s.lines().find(|l| l.starts_with("MemTotal:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|n| n.parse::<u64>().ok())
                    .map(|kb| kb / 1_048_576)
            })
            .unwrap_or(4)
    }
    #[cfg(target_os = "macos")]
    {
        // sysctl hw.memsize
        std::process::Command::new("sysctl").arg("-n").arg("hw.memsize")
            .output().ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|bytes| bytes / 1_073_741_824)
            .unwrap_or(8)
    }
    #[cfg(target_os = "windows")]
    {
        // Simplified: assume 8GB if can't detect
        8
    }
}

// ═══ Model Download ═══

const WHISPER_MODELS: &[(&str, &str, u64)] = &[
    ("tiny",            "ggml-tiny.bin",            75),
    ("tiny.en",         "ggml-tiny.en.bin",         75),
    ("base",            "ggml-base.bin",            142),
    ("base.en",         "ggml-base.en.bin",         142),
    ("small",           "ggml-small.bin",           488),
    ("small.en",        "ggml-small.en.bin",        488),
    ("medium",          "ggml-medium.bin",          1500),
    ("medium.en",       "ggml-medium.en.bin",       1500),
    ("large-v3-turbo",  "ggml-large-v3-turbo.bin",  1600),
];

pub fn model_download_url(model_name: &str) -> Option<String> {
    WHISPER_MODELS.iter()
        .find(|(name, _, _)| *name == model_name)
        .map(|(_, filename, _)| {
            format!(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
                filename
            )
        })
}

pub fn model_filename(model_name: &str) -> Option<&'static str> {
    WHISPER_MODELS.iter()
        .find(|(name, _, _)| *name == model_name)
        .map(|(_, filename, _)| *filename)
}

pub fn model_size_mb(model_name: &str) -> Option<u64> {
    WHISPER_MODELS.iter()
        .find(|(name, _, _)| *name == model_name)
        .map(|(_, _, size)| *size)
}
```

### Transform Pipeline

```rust
// transforms/mod.rs

use anyhow::Result;
use std::collections::HashMap;

pub struct TransformContext {
    pub language: String,
    pub profile_name: String,
    pub api_keys: HashMap<String, String>,
    pub audio_duration_secs: f64,
}

/// Execute transforms in sequence. If one fails, log warning and continue with last good text.
pub async fn run_pipeline(
    configs: &[crate::config::TransformConfig],
    raw_text: &str,
    ctx: &TransformContext,
) -> Result<String> {
    let mut text = raw_text.to_string();

    for config in configs {
        match apply_transform(config, &text, ctx).await {
            Ok(transformed) => {
                tracing::debug!(
                    transform = ?config, 
                    before_len = text.len(), 
                    after_len = transformed.len(),
                    "Transform applied"
                );
                text = transformed;
            }
            Err(e) => {
                tracing::warn!(%e, transform = ?config, "Transform failed, keeping previous text");
                // NON interrompiamo — continuiamo con il testo precedente
            }
        }
    }

    Ok(text)
}

async fn apply_transform(
    config: &crate::config::TransformConfig,
    text: &str,
    ctx: &TransformContext,
) -> Result<String> {
    match config {
        crate::config::TransformConfig::Cleanup => {
            cleanup::apply(text)
        }
        crate::config::TransformConfig::AiRewrite { prompt, context, model } => {
            let api_key = ctx.api_keys.get("gemini")
                .or_else(|| ctx.api_keys.get("openai"))
                .context("No API key available for AI rewrite")?;
            ai_rewrite::apply(text, prompt, context, model, api_key).await
        }
        crate::config::TransformConfig::Template { template } => {
            Ok(template.replace("{{body}}", text))
        }
    }
}

pub mod cleanup;
pub mod ai_rewrite;
```

```rust
// transforms/cleanup.rs — Filler word removal, zero API calls

pub fn apply(text: &str) -> anyhow::Result<String> {
    let mut result = text.to_string();

    // Filler words per lingua
    let fillers = [
        // English
        "um", "uh", "erm", "hmm", "like", "you know", "basically",
        "actually", "literally", "right", "so yeah",
        // Italian
        "ehm", "cioè", "tipo", "praticamente", "allora", "insomma",
        "diciamo", "ecco",
        // Spanish
        "este", "o sea", "bueno",
        // French  
        "euh", "ben", "genre", "voilà",
    ];

    for filler in &fillers {
        // Word boundary match, case insensitive
        // Rimuovi anche virgola/punto che segue il filler
        let pattern = format!(r"(?i)\b{}\b[,.]?\s*", regex::escape(filler));
        if let Ok(re) = regex::Regex::new(&pattern) {
            result = re.replace_all(&result, " ").to_string();
        }
    }

    // Normalizza spazi multipli
    if let Ok(re) = regex::Regex::new(r"\s{2,}") {
        result = re.replace_all(&result, " ").to_string();
    }

    // Trim
    result = result.trim().to_string();

    // Prima lettera maiuscola
    if let Some(first) = result.chars().next() {
        if first.is_lowercase() {
            result = first.to_uppercase().to_string() + &result[first.len_utf8()..];
        }
    }

    Ok(result)
}
```

```rust
// transforms/ai_rewrite.rs — Riscrittura AI via LLM

use anyhow::{Context, Result};
use serde_json::json;

pub async fn apply(
    text: &str,
    prompt: &str,
    context: &str,
    model: &str,
    api_key: &str,
) -> Result<String> {
    let system = if context.is_empty() {
        prompt.to_string()
    } else {
        format!("{}\n\nContext about me: {}", prompt, context)
    };

    let model_name = model.strip_prefix("models/").unwrap_or(model);

    let body = json!({
        "contents": [{
            "parts": [{
                "text": format!(
                    "{}\n\n---\nOriginal dictation:\n{}\n---\nRewritten output (nothing else):",
                    system, text
                )
            }]
        }],
        "generationConfig": {
            "temperature": 0.3,
            "maxOutputTokens": 4096
        }
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model_name
    );

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("x-goog-api-key", api_key)
        .json(&body)
        .send().await
        .context("AI rewrite request failed")?;

    if !response.status().is_success() {
        anyhow::bail!("AI rewrite failed: HTTP {}", response.status());
    }

    let parsed: serde_json::Value = response.json().await?;

    // Riusa la stessa logica di parsing di gemini.rs
    if let Some(candidates) = parsed["candidates"].as_array() {
        if let Some(first) = candidates.first() {
            if let Some(parts) = first["content"]["parts"].as_array() {
                let text: String = parts.iter()
                    .filter_map(|p| p["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    return Ok(text.trim().to_string());
                }
            }
        }
    }

    // Se il parsing fallisce, ritorna il testo originale
    Ok(text.to_string())
}
```

### Cargo.toml — Nuove dipendenze

```toml
# Aggiungere a [dependencies]
async-trait = "0.1"
regex = "1"

# Aggiungere se si usa il multipart di reqwest
# reqwest già presente, aggiungere feature "multipart"
reqwest = { version = "0.12", features = ["json", "rustls-tls", "blocking", "multipart"] }

# Feature-gated per locale
[features]
default = []
local-whisper = ["whisper-rs"]

[dependencies.whisper-rs]
version = "0.13"
optional = true
```

### Verifica API Keys nel Wizard

```rust
// config.rs — Funzioni di verifica per ogni provider

/// Verifica Gemini key (già esistente, riusa verify_api_key_sync)
fn verify_gemini_key(key: &str) -> Result<()> {
    // ... codice attuale da config.rs:333-358 ...
}

/// Verifica OpenAI key
fn verify_openai_key(key: &str) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10)).build()?;
    let resp = client.get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", key))
        .send()?;
    if resp.status().is_success() { Ok(()) }
    else { anyhow::bail!("OpenAI key invalid (HTTP {})", resp.status()) }
}

/// Verifica Deepgram key
fn verify_deepgram_key(key: &str) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10)).build()?;
    let resp = client.get("https://api.deepgram.com/v1/projects")
        .header("Authorization", format!("Token {}", key))
        .send()?;
    if resp.status().is_success() { Ok(()) }
    else { anyhow::bail!("Deepgram key invalid (HTTP {})", resp.status()) }
}
```

---

## Checklist

- [ ] Creare `src/audio_encoding.rs` (extract encode_wav da network.rs)
- [ ] Creare `src/providers/mod.rs` (trait, factory, shared http client)
- [ ] Creare `src/providers/gemini.rs` (extract da network.rs + fix prompt + fix errori)
- [ ] Creare `src/providers/openai.rs` (multipart, pricing $0.006/min)
- [ ] Creare `src/providers/deepgram.rs` (raw WAV body, smart_format)
- [ ] Creare `src/providers/local.rs` (whisper-rs, feature-gated, hardware detect)
- [ ] Creare `src/transforms/mod.rs` (pipeline runner)
- [ ] Creare `src/transforms/cleanup.rs` (regex filler removal)
- [ ] Creare `src/transforms/ai_rewrite.rs` (Gemini rewrite)
- [ ] Aggiungere `async-trait`, `regex` a Cargo.toml
- [ ] Aggiungere feature `multipart` a reqwest
- [ ] Aggiungere feature `local-whisper` con whisper-rs opzionale
- [ ] Svuotare `network.rs` (redirect a providers::gemini)
- [ ] Aggiungere verify_openai_key, verify_deepgram_key nel wizard
- [ ] Test: ogni provider con mock response
- [ ] Test: pipeline con cleanup + ai_rewrite in sequenza
- [ ] Test: factory con tutti i provider names

use std::collections::HashMap;

use crate::config::Profile;
use crate::tracking::TokenUsage;

pub mod gemini;
#[path = "../model_catalog.rs"]
mod raw_model_catalog;

pub mod model_catalog {
    pub use super::raw_model_catalog::{
        fallback_models, find, is_selectable, normalize_model_id, recommended_model,
        selectable_models, LIVE_AUDIO_MODELS, PRICING_REVIEWED_AT,
    };
}

#[allow(dead_code)]
pub mod openai {
    // Placeholder
}

#[allow(dead_code)]
pub mod deepgram {
    // Placeholder
}

pub enum Provider {
    Gemini(gemini::GeminiProvider),
}

impl Provider {
    pub async fn transcribe(
        &self,
        audio: &[i16],
        language: &str,
    ) -> Result<(String, TokenUsage), gemini::GeminiError> {
        match self {
            Provider::Gemini(provider) => provider.transcribe(audio, language).await,
        }
    }
}

#[derive(Debug)]
pub struct TranscriptionOutcome {
    pub text: String,
    pub usage: TokenUsage,
    pub model_used: String,
    pub fallback_from: Option<String>,
}

pub fn create_provider_for_model(
    profile: &Profile,
    keys: &HashMap<String, String>,
    model: &str,
) -> Result<Provider, gemini::GeminiError> {
    if profile.provider != "gemini" {
        return Err(gemini::GeminiError::configuration(format!(
            "Provider '{}' non supportato",
            profile.provider
        )));
    }
    if !model_catalog::is_selectable(model) {
        return Err(gemini::GeminiError::configuration(format!(
            "Modello '{}' non disponibile per la trascrizione one-shot",
            model_catalog::normalize_model_id(model)
        )));
    }

    let api_key = keys.get("gemini").cloned().unwrap_or_default();
    Ok(Provider::Gemini(gemini::GeminiProvider::new(
        api_key,
        model.to_string(),
        profile.timeout_secs,
        profile.custom_prompt.clone(),
    )))
}

pub async fn transcribe_exact(
    profile: &Profile,
    keys: &HashMap<String, String>,
    model: &str,
    audio: &[i16],
    language: &str,
) -> Result<TranscriptionOutcome, gemini::GeminiError> {
    let normalized = model_catalog::normalize_model_id(model);
    let provider = create_provider_for_model(profile, keys, normalized)?;
    let (text, usage) = provider.transcribe(audio, language).await?;
    Ok(TranscriptionOutcome {
        text,
        usage,
        model_used: format!("models/{normalized}"),
        fallback_from: None,
    })
}

pub async fn transcribe_exact_prompt(
    profile: &Profile,
    keys: &HashMap<String, String>,
    model: &str,
    audio: &[i16],
    language: &str,
    prompt: &str,
) -> Result<TranscriptionOutcome, gemini::GeminiError> {
    let mut request_profile = profile.clone();
    request_profile.custom_prompt = Some(prompt.to_string());
    transcribe_exact(&request_profile, keys, model, audio, language).await
}

pub async fn transcribe_with_fallback(
    profile: &Profile,
    keys: &HashMap<String, String>,
    audio: &[i16],
    language: &str,
) -> Result<TranscriptionOutcome, gemini::GeminiError> {
    let configured = model_catalog::normalize_model_id(&profile.model).to_string();
    let primary = if model_catalog::is_selectable(&configured) {
        configured.clone()
    } else {
        let recommended = model_catalog::normalize_model_id(model_catalog::recommended_model());
        tracing::warn!(configured_model = %configured, replacement = %recommended,
            "Configured Gemini model is retired/unsupported; using current recommended model");
        recommended.to_string()
    };

    let mut candidates = vec![primary.clone()];
    candidates.extend(
        model_catalog::fallback_models(&primary)
            .into_iter()
            .map(str::to_string),
    );

    let mut last_error: Option<gemini::GeminiError> = None;
    for (index, model) in candidates.iter().enumerate() {
        if index > 0 {
            tracing::warn!(primary = %primary, fallback = %model,
                "Transient Gemini failure: trying fallback transcription model");
        }
        let provider = create_provider_for_model(profile, keys, model)?;
        match provider.transcribe(audio, language).await {
            Ok((text, usage)) => {
                return Ok(TranscriptionOutcome {
                    text,
                    usage,
                    model_used: format!("models/{model}"),
                    fallback_from: (index > 0).then(|| format!("models/{primary}")),
                });
            }
            Err(error) if error.is_transient() => {
                tracing::warn!(model = %model, kind = ?error.kind, %error, "Transient Gemini error");
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        gemini::GeminiError::configuration("Nessun modello di trascrizione disponibile")
    }))
}

pub async fn transcribe_with_fallback_prompt(
    profile: &Profile,
    keys: &HashMap<String, String>,
    audio: &[i16],
    language: &str,
    prompt: &str,
) -> Result<TranscriptionOutcome, gemini::GeminiError> {
    let mut request_profile = profile.clone();
    request_profile.custom_prompt = Some(prompt.to_string());
    transcribe_with_fallback(&request_profile, keys, audio, language).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_profile_is_not_directly_selectable() {
        let profile = Profile {
            model: "models/gemini-2.0-flash".into(),
            ..Profile::default()
        };
        let keys = HashMap::new();
        assert!(create_provider_for_model(&profile, &keys, &profile.model).is_err());
    }

    #[test]
    fn prompt_override_is_isolated_to_request_clone() {
        let profile = Profile::default();
        let mut request = profile.clone();
        request.custom_prompt = Some("custom".into());
        assert!(profile.custom_prompt.is_none());
        assert_eq!(request.custom_prompt.as_deref(), Some("custom"));
    }
}

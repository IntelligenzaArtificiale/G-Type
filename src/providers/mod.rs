use std::collections::HashMap;

use crate::config::Profile;
use crate::tracking::TokenUsage;

pub mod gemini;
#[path = "../model_catalog.rs"]
pub mod model_catalog;

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

/// Instantiate a Gemini provider from a profile but with an explicit model.
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

/// Compatibility helper for callers that explicitly want the profile model.
pub fn create_provider(
    profile: &Profile,
    keys: &HashMap<String, String>,
) -> Result<Provider, gemini::GeminiError> {
    create_provider_for_model(profile, keys, &profile.model)
}

/// Transcribe using exactly one selected model. Used by dashboard recovery so
/// the user's model choice is explicit and predictable.
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

/// Normal dictation path: use the configured model first. Only transient
/// failures (429, 5xx, timeout/network) may move to up to two inexpensive,
/// stable Flash-Lite fallbacks. Auth, bad request and configuration errors stop
/// immediately. This avoids repeatedly hammering an overloaded model while
/// keeping unexpected fallback costs bounded.
pub async fn transcribe_with_fallback(
    profile: &Profile,
    keys: &HashMap<String, String>,
    audio: &[i16],
    language: &str,
) -> Result<TranscriptionOutcome, gemini::GeminiError> {
    let primary = model_catalog::normalize_model_id(&profile.model).to_string();
    let mut candidates = vec![primary.clone()];
    candidates.extend(
        model_catalog::fallback_models(&primary)
            .into_iter()
            .map(str::to_string),
    );

    let mut last_error: Option<gemini::GeminiError> = None;
    for (index, model) in candidates.iter().enumerate() {
        if index > 0 {
            tracing::warn!(
                primary = %primary,
                fallback = %model,
                "Transient Gemini failure: trying fallback transcription model"
            );
        }

        let provider = create_provider_for_model(profile, keys, model)?;
        match provider.transcribe(audio, language).await {
            Ok((text, usage)) => {
                if index > 0 {
                    tracing::info!(
                        primary = %primary,
                        model_used = %model,
                        "Transcription recovered through fallback model"
                    );
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_profile_model_must_be_selectable() {
        let profile = Profile {
            model: "models/gemini-2.0-flash".into(),
            ..Profile::default()
        };
        let keys = HashMap::new();
        assert!(create_provider(&profile, &keys).is_err());
    }
}

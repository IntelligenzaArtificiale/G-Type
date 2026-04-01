use anyhow::Result;
use std::collections::HashMap;

use crate::config::Profile;
use crate::tracking::TokenUsage;

pub mod gemini;

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
    pub async fn transcribe(&self, audio: &[i16], language: &str) -> Result<(String, TokenUsage)> {
        match self {
            Provider::Gemini(p) => p.transcribe(audio, language).await,
        }
    }
}

/// Factory function per instanziare il provider corretto dal profilo.
pub fn create_provider(profile: &Profile, keys: &HashMap<String, String>) -> Result<Provider> {
    match profile.provider.as_str() {
        "gemini" => {
            let api_key = keys.get("gemini").cloned().unwrap_or_default();
            Ok(Provider::Gemini(gemini::GeminiProvider::new(api_key, profile.model.clone())))
        }
        _ => anyhow::bail!("Provider '{}' non ancora supportato o sconosciuto", profile.provider),
    }
}

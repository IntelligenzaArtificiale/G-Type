// model_catalog.rs — Single source of truth for Gemini audio→text models.
// Pricing last reviewed against official Google Gemini Developer API docs:
// 2026-08-13. Standard paid-tier prices, USD per 1M tokens.

use serde::Serialize;

pub const PRICING_REVIEWED_AT: &str = "2026-08-13";
pub const RECOMMENDED_MODEL: &str = "models/gemini-3.5-flash-lite";

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub status: &'static str,
    pub api_kind: &'static str,
    pub selectable: bool,
    pub audio_input: bool,
    pub text_output: bool,
    pub input_text_per_m: f64,
    pub input_audio_per_m: f64,
    pub output_per_m: f64,
    pub high_input_text_per_m: Option<f64>,
    pub high_input_audio_per_m: Option<f64>,
    pub high_output_per_m: Option<f64>,
    pub high_tier_after_tokens: Option<u64>,
    pub thinking_level: Option<&'static str>,
}

impl ModelSpec {
    pub fn normalized_id(&self) -> String {
        format!("models/{}", self.id)
    }

    pub fn pricing_for_prompt_tokens(&self, prompt_tokens: u64) -> (f64, f64, f64) {
        let high = self
            .high_tier_after_tokens
            .is_some_and(|threshold| prompt_tokens > threshold);
        if high {
            (
                self.high_input_text_per_m.unwrap_or(self.input_text_per_m),
                self.high_input_audio_per_m
                    .unwrap_or(self.input_audio_per_m),
                self.high_output_per_m.unwrap_or(self.output_per_m),
            )
        } else {
            (
                self.input_text_per_m,
                self.input_audio_per_m,
                self.output_per_m,
            )
        }
    }
}

// Current one-shot generateContent-compatible audio→text models. Live API models
// are listed separately below because G-Type's push-to-talk path is not a Live
// API session and must not silently route to a protocol-incompatible endpoint.
pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "gemini-3.6-flash",
        label: "Gemini 3.6 Flash",
        status: "stable",
        api_kind: "generate_content",
        selectable: true,
        audio_input: true,
        text_output: true,
        input_text_per_m: 1.50,
        input_audio_per_m: 1.50,
        output_per_m: 7.50,
        high_input_text_per_m: None,
        high_input_audio_per_m: None,
        high_output_per_m: None,
        high_tier_after_tokens: None,
        thinking_level: Some("minimal"),
    },
    ModelSpec {
        id: "gemini-3.5-flash",
        label: "Gemini 3.5 Flash",
        status: "stable",
        api_kind: "generate_content",
        selectable: true,
        audio_input: true,
        text_output: true,
        input_text_per_m: 1.50,
        input_audio_per_m: 1.50,
        output_per_m: 9.00,
        high_input_text_per_m: None,
        high_input_audio_per_m: None,
        high_output_per_m: None,
        high_tier_after_tokens: None,
        thinking_level: Some("minimal"),
    },
    ModelSpec {
        id: "gemini-3.5-flash-lite",
        label: "Gemini 3.5 Flash-Lite",
        status: "stable",
        api_kind: "generate_content",
        selectable: true,
        audio_input: true,
        text_output: true,
        input_text_per_m: 0.30,
        input_audio_per_m: 0.30,
        output_per_m: 2.50,
        high_input_text_per_m: None,
        high_input_audio_per_m: None,
        high_output_per_m: None,
        high_tier_after_tokens: None,
        thinking_level: Some("minimal"),
    },
    ModelSpec {
        id: "gemini-3.1-flash-lite",
        label: "Gemini 3.1 Flash-Lite",
        status: "stable",
        api_kind: "generate_content",
        selectable: true,
        audio_input: true,
        text_output: true,
        input_text_per_m: 0.25,
        input_audio_per_m: 0.50,
        output_per_m: 1.50,
        high_input_text_per_m: None,
        high_input_audio_per_m: None,
        high_output_per_m: None,
        high_tier_after_tokens: None,
        thinking_level: Some("minimal"),
    },
    ModelSpec {
        id: "gemini-3.1-pro-preview",
        label: "Gemini 3.1 Pro Preview",
        status: "preview",
        api_kind: "generate_content",
        selectable: true,
        audio_input: true,
        text_output: true,
        input_text_per_m: 2.00,
        input_audio_per_m: 2.00,
        output_per_m: 12.00,
        high_input_text_per_m: Some(4.00),
        high_input_audio_per_m: Some(4.00),
        high_output_per_m: Some(18.00),
        high_tier_after_tokens: Some(200_000),
        thinking_level: Some("low"),
    },
    ModelSpec {
        id: "gemini-3-flash-preview",
        label: "Gemini 3 Flash Preview",
        status: "preview",
        api_kind: "generate_content",
        selectable: true,
        audio_input: true,
        text_output: true,
        input_text_per_m: 0.50,
        input_audio_per_m: 1.00,
        output_per_m: 3.00,
        high_input_text_per_m: None,
        high_input_audio_per_m: None,
        high_output_per_m: None,
        high_tier_after_tokens: None,
        thinking_level: Some("minimal"),
    },
    ModelSpec {
        id: "gemini-2.5-pro",
        label: "Gemini 2.5 Pro",
        status: "stable",
        api_kind: "generate_content",
        selectable: true,
        audio_input: true,
        text_output: true,
        input_text_per_m: 1.25,
        input_audio_per_m: 1.25,
        output_per_m: 10.00,
        high_input_text_per_m: Some(2.50),
        high_input_audio_per_m: Some(2.50),
        high_output_per_m: Some(15.00),
        high_tier_after_tokens: Some(200_000),
        thinking_level: None,
    },
    ModelSpec {
        id: "gemini-2.5-flash",
        label: "Gemini 2.5 Flash",
        status: "stable",
        api_kind: "generate_content",
        selectable: true,
        audio_input: true,
        text_output: true,
        input_text_per_m: 0.30,
        input_audio_per_m: 1.00,
        output_per_m: 2.50,
        high_input_text_per_m: None,
        high_input_audio_per_m: None,
        high_output_per_m: None,
        high_tier_after_tokens: None,
        thinking_level: None,
    },
    ModelSpec {
        id: "gemini-2.5-flash-lite",
        label: "Gemini 2.5 Flash-Lite",
        status: "stable",
        api_kind: "generate_content",
        selectable: true,
        audio_input: true,
        text_output: true,
        input_text_per_m: 0.10,
        input_audio_per_m: 0.30,
        output_per_m: 0.40,
        high_input_text_per_m: None,
        high_input_audio_per_m: None,
        high_output_per_m: None,
        high_tier_after_tokens: None,
        thinking_level: None,
    },
    // Kept for historical accounting only. Google shut Gemini 2.0 down on
    // 2026-06-01, so these are never offered for new requests.
    ModelSpec {
        id: "gemini-2.0-flash",
        label: "Gemini 2.0 Flash (retired)",
        status: "retired",
        api_kind: "generate_content",
        selectable: false,
        audio_input: true,
        text_output: true,
        input_text_per_m: 0.10,
        input_audio_per_m: 0.70,
        output_per_m: 0.40,
        high_input_text_per_m: None,
        high_input_audio_per_m: None,
        high_output_per_m: None,
        high_tier_after_tokens: None,
        thinking_level: None,
    },
    ModelSpec {
        id: "gemini-2.5-flash-lite-preview-09-2025",
        label: "Gemini 2.5 Flash-Lite Preview 09-2025 (retired)",
        status: "retired",
        api_kind: "generate_content",
        selectable: false,
        audio_input: true,
        text_output: true,
        input_text_per_m: 0.10,
        input_audio_per_m: 0.30,
        output_per_m: 0.40,
        high_input_text_per_m: None,
        high_input_audio_per_m: None,
        high_output_per_m: None,
        high_tier_after_tokens: None,
        thinking_level: None,
    },
    ModelSpec {
        id: "gemini-3-pro-preview",
        label: "Gemini 3 Pro Preview (retired alias)",
        status: "retired",
        api_kind: "generate_content",
        selectable: false,
        audio_input: true,
        text_output: true,
        input_text_per_m: 2.00,
        input_audio_per_m: 2.00,
        output_per_m: 12.00,
        high_input_text_per_m: Some(4.00),
        high_input_audio_per_m: Some(4.00),
        high_output_per_m: Some(18.00),
        high_tier_after_tokens: Some(200_000),
        thinking_level: Some("low"),
    },
    ModelSpec {
        id: "gemini-3.1-pro-preview-customtools",
        label: "Gemini 3.1 Pro Preview Custom Tools",
        status: "preview-specialized",
        api_kind: "generate_content",
        selectable: false,
        audio_input: true,
        text_output: true,
        input_text_per_m: 2.00,
        input_audio_per_m: 2.00,
        output_per_m: 12.00,
        high_input_text_per_m: Some(4.00),
        high_input_audio_per_m: Some(4.00),
        high_output_per_m: Some(18.00),
        high_tier_after_tokens: Some(200_000),
        thinking_level: Some("low"),
    },
];

#[derive(Debug, Clone, Copy, Serialize)]
pub struct LiveAudioModel {
    pub id: &'static str,
    pub label: &'static str,
    pub status: &'static str,
    pub note: &'static str,
}

pub const LIVE_AUDIO_MODELS: &[LiveAudioModel] = &[
    LiveAudioModel {
        id: "gemini-3.1-flash-live-preview",
        label: "Gemini 3.1 Flash Live Preview",
        status: "preview",
        note: "Live API audio-to-audio; not a drop-in generateContent transcription model",
    },
    LiveAudioModel {
        id: "gemini-2.5-flash-native-audio-preview-12-2025",
        label: "Gemini 2.5 Flash Live Preview",
        status: "preview",
        note: "Live API native audio; not a drop-in generateContent transcription model",
    },
];

pub fn normalize_model_id(model: &str) -> &str {
    model.strip_prefix("models/").unwrap_or(model)
}

pub fn find(model: &str) -> Option<&'static ModelSpec> {
    let id = normalize_model_id(model);
    MODELS.iter().find(|spec| spec.id == id)
}

pub fn selectable_models() -> impl Iterator<Item = &'static ModelSpec> {
    MODELS.iter().filter(|spec| spec.selectable)
}

pub fn is_selectable(model: &str) -> bool {
    find(model).is_some_and(|spec| spec.selectable)
}

pub fn recommended_model() -> &'static str {
    RECOMMENDED_MODEL
}

/// Stable, inexpensive models used only after a transient failure. We never
/// auto-fallback to Pro or Preview models so resilience cannot unexpectedly
/// create a large cost increase.
pub fn fallback_models(primary: &str) -> Vec<&'static str> {
    let primary = normalize_model_id(primary);
    [
        "gemini-3.5-flash-lite",
        "gemini-3.1-flash-lite",
        "gemini-2.5-flash-lite",
    ]
    .into_iter()
    .filter(|candidate| *candidate != primary)
    .take(2)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn selectable_models_are_unique_and_priced() {
        let mut ids = HashSet::new();
        for model in selectable_models() {
            assert!(ids.insert(model.id));
            assert!(model.audio_input && model.text_output);
            assert!(model.input_audio_per_m > 0.0);
            assert!(model.output_per_m > 0.0);
        }
    }

    #[test]
    fn dead_models_are_not_selectable() {
        assert!(!is_selectable("gemini-2.0-flash"));
        assert!(!is_selectable("gemini-2.5-flash-lite-preview-09-2025"));
    }

    #[test]
    fn fallback_never_repeats_primary_or_uses_pro() {
        let fallback = fallback_models("models/gemini-3.5-flash-lite");
        assert!(!fallback.contains(&"gemini-3.5-flash-lite"));
        assert!(fallback.iter().all(|id| !id.contains("pro")));
    }
}

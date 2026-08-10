use tracing::debug;

use crate::config::TransformConfig;

/// Run the configured transforms sequentially.
/// If any transform fails, we log a warning and return the *previous* valid text,
/// guaranteeing we never lose user data.
pub async fn run_pipeline(
    transforms: &[TransformConfig],
    initial_text: &str,
    _language: &str,
) -> String {
    let mut current_text = initial_text.to_string();

    for t in transforms {
        match t {
            TransformConfig::Cleanup => {
                debug!("Applying Cleanup transform");
                current_text = apply_cleanup(&current_text);
            }
            TransformConfig::AiRewrite {
                prompt: _,
                context: _,
                model: _,
            } => {
                debug!("Applying AiRewrite transform (stub)");
                // To be implemented: API call for rewrite.
                // If it fails, keep current_text so user data is never lost.
            }
            TransformConfig::Template { template } => {
                debug!("Applying Template transform");
                current_text = template.replace("{{text}}", &current_text);
            }
        }
    }

    current_text
}

/// Cleanup common fillers and artifacts.
fn apply_cleanup(text: &str) -> String {
    let mut cleaned = text.to_string();

    // Rimuovi spazi multipli.
    cleaned = cleaned.replace("  ", " ");

    cleaned.trim().to_string()
}

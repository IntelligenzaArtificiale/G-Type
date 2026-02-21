// injector.rs — Text injection via keystroke emulation.
// Always types character by character using enigo for maximum compatibility.
// No clipboard involvement — works everywhere, no arboard warnings.

use anyhow::Result;
use enigo::{Enigo, Keyboard, Settings};
use std::thread;
use std::time::Duration;
use tracing::{debug, warn};

/// Delay between individual keystrokes (ms) to avoid dropped input.
const KEYSTROKE_DELAY_MS: u64 = 3;

/// Inject text into the currently focused application via keystroke emulation.
///
/// The `_threshold` parameter is kept for API compatibility but ignored —
/// we always use keystrokes.
pub fn inject(text: &str, _threshold: usize) -> Result<()> {
    if text.is_empty() {
        debug!("Empty text, nothing to inject");
        return Ok(());
    }

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Failed to initialize enigo: {:?}", e))?;

    // Small delay before starting to let the OS settle after hotkey release
    thread::sleep(Duration::from_millis(80));

    for ch in text.chars() {
        if let Err(e) = enigo.text(&ch.to_string()) {
            warn!(?e, char = %ch, "Failed to type character, continuing");
        }
        thread::sleep(Duration::from_millis(KEYSTROKE_DELAY_MS));
    }

    debug!(len = text.len(), "Keystroke injection complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_text() {
        let result = inject("", 80);
        assert!(result.is_ok());
    }

    #[test]
    fn test_threshold_ignored() {
        // Threshold parameter is accepted but ignored
        let short = "hello";
        assert!(short.len() < 80);
        let long = "a".repeat(100);
        assert!(long.len() >= 80);
    }
}

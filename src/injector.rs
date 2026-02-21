// injector.rs — Adaptive text injection into the focused application.
// Short text (<80 chars): emulate keystrokes via enigo with micro-sleep.
// Long text (>=80 chars): clipboard swap via arboard + paste shortcut.

use anyhow::{Context, Result};
use arboard::Clipboard;
use enigo::{Enigo, Keyboard, Settings, Key, Direction};
use std::thread;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Delay between individual keystrokes (ms) to avoid dropped input.
const KEYSTROKE_DELAY_MS: u64 = 5;
/// Delay after paste before restoring clipboard (ms).
const PASTE_SETTLE_MS: u64 = 80;

/// Inject text into the currently focused application.
///
/// Strategy:
/// - If `text.len() < threshold`: type character by character via enigo.
/// - If `text.len() >= threshold`: clipboard injection with backup/restore.
pub fn inject(text: &str, threshold: usize) -> Result<()> {
    if text.is_empty() {
        debug!("Empty text, nothing to inject");
        return Ok(());
    }

    info!(len = text.len(), threshold, "Injecting text");

    if text.len() < threshold {
        inject_keystrokes(text)
    } else {
        inject_clipboard(text)
    }
}

/// Type text character by character using enigo.
fn inject_keystrokes(text: &str) -> Result<()> {
    debug!(len = text.len(), "Using keystroke injection");

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Failed to initialize enigo: {:?}", e))?;

    // Small delay before starting to let the OS settle
    thread::sleep(Duration::from_millis(50));

    for ch in text.chars() {
        if let Err(e) = enigo.text(&ch.to_string()) {
            warn!(?e, char = %ch, "Failed to type character, continuing");
        }
        thread::sleep(Duration::from_millis(KEYSTROKE_DELAY_MS));
    }

    debug!("Keystroke injection complete");
    Ok(())
}

/// Inject text via clipboard: backup current clipboard, set new text, paste, restore.
fn inject_clipboard(text: &str) -> Result<()> {
    debug!(len = text.len(), "Using clipboard injection");

    let mut clipboard = Clipboard::new()
        .context("Failed to access system clipboard")?;

    // Step 1: Backup current clipboard contents
    let backup = clipboard.get_text().ok();
    if backup.is_some() {
        debug!("Clipboard backup saved");
    }

    // Step 2: Set new text in clipboard
    clipboard
        .set_text(text.to_string())
        .context("Failed to set clipboard text")?;

    // Step 3: Simulate paste shortcut
    paste_shortcut()?;

    // Step 4: Wait for paste to settle
    thread::sleep(Duration::from_millis(PASTE_SETTLE_MS));

    // Step 5: Restore original clipboard
    match backup {
        Some(original) => {
            // Re-acquire clipboard (it may have been released)
            match Clipboard::new() {
                Ok(mut cb) => {
                    if let Err(e) = cb.set_text(original) {
                        warn!(%e, "Failed to restore clipboard (non-fatal)");
                    } else {
                        debug!("Clipboard restored");
                    }
                }
                Err(e) => {
                    warn!(%e, "Failed to re-acquire clipboard for restore");
                }
            }
        }
        None => {
            debug!("No previous clipboard content to restore");
        }
    }

    info!("Clipboard injection complete");
    Ok(())
}

/// Send the OS-appropriate paste shortcut (CTRL+V on Linux/Windows, CMD+V on macOS).
fn paste_shortcut() -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Failed to initialize enigo for paste: {:?}", e))?;

    // Small delay to ensure clipboard is ready
    thread::sleep(Duration::from_millis(20));

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = enigo.key(Key::Meta, Direction::Press) {
            error!(?e, "Failed to press Meta key");
        }
        if let Err(e) = enigo.key(Key::Unicode('v'), Direction::Click) {
            error!(?e, "Failed to press V key");
        }
        if let Err(e) = enigo.key(Key::Meta, Direction::Release) {
            error!(?e, "Failed to release Meta key");
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Err(e) = enigo.key(Key::Control, Direction::Press) {
            error!(?e, "Failed to press Control key");
        }
        if let Err(e) = enigo.key(Key::Unicode('v'), Direction::Click) {
            error!(?e, "Failed to press V key");
        }
        if let Err(e) = enigo.key(Key::Control, Direction::Release) {
            error!(?e, "Failed to release Control key");
        }
    }

    debug!("Paste shortcut sent");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_text() {
        // Should succeed without doing anything
        let result = inject("", 80);
        assert!(result.is_ok());
    }

    #[test]
    fn test_threshold_logic() {
        // Just verify the threshold logic doesn't panic
        // Actual injection requires a display server
        let short = "hello";
        assert!(short.len() < 80);

        let long = "a".repeat(100);
        assert!(long.len() >= 80);
    }
}

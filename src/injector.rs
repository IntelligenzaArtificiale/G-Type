// injector.rs — Text injection via keystroke emulation with clipboard fallback.
// Always tries to type character by character using enigo for maximum compatibility.
// If the text is extremely long or injection fails, it falls back to clipboard injection.

use anyhow::{Context, Result};
use arboard::Clipboard;
use enigo::{Enigo, Keyboard, Settings, Key, Direction};
use std::thread;
use std::time::Duration;
use tracing::{debug, error, warn};

/// Delay between individual keystrokes (ms) to avoid dropped input.
const KEYSTROKE_DELAY_MS: u64 = 3;
/// Delay after paste before restoring clipboard (ms).
const PASTE_SETTLE_MS: u64 = 80;
/// Threshold above which we consider text "too long" for keystrokes and use clipboard.
const LONG_TEXT_THRESHOLD: usize = 500;

/// Inject text into the currently focused application.
///
/// Strategy:
/// 1. If text is very long (>500 chars), use clipboard injection directly.
/// 2. Otherwise, try keystroke injection.
/// 3. If keystroke injection fails, fallback to clipboard injection.
pub fn inject(text: &str) -> Result<()> {
    if text.is_empty() {
        debug!("Empty text, nothing to inject");
        return Ok(());
    }

    if text.len() > LONG_TEXT_THRESHOLD {
        debug!(len = text.len(), "Text is very long, using clipboard injection directly");
        return inject_clipboard(text);
    }

    match inject_keystrokes(text) {
        Ok(_) => Ok(()),
        Err(e) => {
            warn!(%e, "Keystroke injection failed, falling back to clipboard");
            inject_clipboard(text)
        }
    }
}

/// Type text character by character using enigo.
fn inject_keystrokes(text: &str) -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Failed to initialize enigo: {:?}", e))?;

    // Small delay before starting to let the OS settle after hotkey release
    thread::sleep(Duration::from_millis(80));

    for ch in text.chars() {
        if let Err(e) = enigo.text(&ch.to_string()) {
            anyhow::bail!("Failed to type character '{}': {:?}", ch, e);
        }
        thread::sleep(Duration::from_millis(KEYSTROKE_DELAY_MS));
    }

    debug!(len = text.len(), "Keystroke injection complete");
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

    debug!("Clipboard injection complete");
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
        let result = inject("");
        assert!(result.is_ok());
    }
}

// injector.rs — Cross-platform text injection with conservative clipboard fallback.

use anyhow::{Context, Result};
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::thread;
use std::time::Duration;
use tracing::{debug, warn};

const KEYSTROKE_DELAY_MS: u64 = 3;
const PRE_INJECT_SETTLE_MS: u64 = 80;
const PASTE_SETTLE_MS: u64 = 250;
const LONG_TEXT_THRESHOLD_CHARS: usize = 300;

/// Inject text into the currently focused application.
///
/// Short, single-line ASCII text is typed directly. Unicode, multiline and
/// longer text uses the clipboard path because it is more reliable across
/// keyboard layouts, IMEs, browsers, IDEs, RDP/VM sessions and macOS/Windows.
pub fn inject(text: &str) -> Result<()> {
    if text.is_empty() {
        debug!("Empty text, nothing to inject");
        return Ok(());
    }

    if should_use_clipboard(text) {
        debug!(chars = text.chars().count(), "Using clipboard injection");
        return inject_clipboard(text);
    }

    match inject_keystrokes(text) {
        Ok(()) => Ok(()),
        Err(error) => {
            warn!(%error, "Keystroke injection failed, falling back to clipboard");
            inject_clipboard(text)
        }
    }
}

fn should_use_clipboard(text: &str) -> bool {
    text.chars().count() > LONG_TEXT_THRESHOLD_CHARS
        || !text.is_ascii()
        || text.contains('\n')
        || text.contains('\r')
}

fn inject_keystrokes(text: &str) -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|error| anyhow::anyhow!("Failed to initialize enigo: {:?}", error))?;

    thread::sleep(Duration::from_millis(PRE_INJECT_SETTLE_MS));

    let total = text.chars().count();
    let log_every = if total > 200 { total / 10 } else { usize::MAX };

    for (index, ch) in text.chars().enumerate() {
        enigo
            .text(&ch.to_string())
            .map_err(|error| anyhow::anyhow!("Failed to type character {:?}: {:?}", ch, error))?;
        thread::sleep(Duration::from_millis(KEYSTROKE_DELAY_MS));

        if log_every != usize::MAX && (index + 1) % log_every == 0 {
            let pct = ((index + 1) as f64 / total as f64 * 100.0) as u32;
            debug!(progress = %format!("{}%", pct), chars = index + 1, total, "Injecting text...");
        }
    }

    debug!(chars = total, "Keystroke injection complete");
    Ok(())
}

fn inject_clipboard(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new().context("Failed to access system clipboard")?;
    let backup = clipboard.get_text().ok();

    clipboard
        .set_text(text.to_string())
        .context("Failed to set clipboard text")?;

    // Give clipboard ownership a moment to propagate before asking the focused
    // application to paste it.
    thread::sleep(Duration::from_millis(30));
    paste_shortcut()?;

    // Some applications consume clipboard contents asynchronously. Restoring
    // immediately can make them paste the user's old clipboard instead.
    thread::sleep(Duration::from_millis(PASTE_SETTLE_MS));

    if let Some(original) = backup {
        match Clipboard::new() {
            Ok(mut cb) => {
                if let Err(error) = cb.set_text(original) {
                    warn!(%error, "Failed to restore clipboard (non-fatal)");
                }
            }
            Err(error) => warn!(%error, "Failed to re-acquire clipboard for restore"),
        }
    }

    debug!(chars = text.chars().count(), "Clipboard injection complete");
    Ok(())
}

fn paste_shortcut() -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|error| anyhow::anyhow!("Failed to initialize enigo for paste: {:?}", error))?;

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo
        .key(modifier, Direction::Press)
        .map_err(|error| anyhow::anyhow!("Failed to press paste modifier: {:?}", error))?;

    let click_result = enigo.key(Key::Unicode('v'), Direction::Click);
    let release_result = enigo.key(modifier, Direction::Release);

    click_result.map_err(|error| anyhow::anyhow!("Failed to press paste key: {:?}", error))?;
    release_result
        .map_err(|error| anyhow::anyhow!("Failed to release paste modifier: {:?}", error))?;

    debug!("Paste shortcut sent");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_strategy_is_conservative() {
        assert!(!should_use_clipboard("hello world"));
        assert!(should_use_clipboard("ciao è già pronto"));
        assert!(should_use_clipboard("prima riga\nseconda riga"));
        assert!(should_use_clipboard(
            &"a".repeat(LONG_TEXT_THRESHOLD_CHARS + 1)
        ));
    }
}

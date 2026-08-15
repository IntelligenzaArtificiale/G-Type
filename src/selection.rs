use anyhow::{Context, Result};
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::thread;
use std::time::Duration;

const COPY_SETTLE_MS: u64 = 110;

/// Capture the currently selected text without keeping clipboard side effects.
/// A sentinel lets us distinguish "nothing selected" from text that happens to
/// equal the user's existing clipboard contents.
pub fn capture_selected_text() -> Result<Option<String>> {
    let mut clipboard = Clipboard::new().context("Impossibile accedere agli appunti")?;
    let backup = clipboard.get_text().ok();
    let sentinel = format!(
        "__GTYPE_SELECTION_SENTINEL_{}_{}__",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    clipboard
        .set_text(sentinel.clone())
        .context("Impossibile preparare gli appunti per Voice Edit")?;
    drop(clipboard);

    send_copy_shortcut()?;
    thread::sleep(Duration::from_millis(COPY_SETTLE_MS));

    let mut clipboard = Clipboard::new().context("Impossibile rileggere gli appunti")?;
    let copied = clipboard.get_text().unwrap_or_default();
    drop(clipboard);

    restore_clipboard(backup);

    let copied = copied.trim().to_string();
    if copied.is_empty() || copied == sentinel {
        Ok(None)
    } else {
        Ok(Some(copied))
    }
}

fn restore_clipboard(backup: Option<String>) {
    if let Some(value) = backup {
        if let Ok(mut clipboard) = Clipboard::new() {
            let _ = clipboard.set_text(value);
        }
    }
}

fn send_copy_shortcut() -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|error| anyhow::anyhow!("Impossibile inizializzare la tastiera: {error:?}"))?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo
        .key(modifier, Direction::Press)
        .map_err(|error| anyhow::anyhow!("Impossibile premere il modificatore copia: {error:?}"))?;
    let click = enigo.key(Key::Unicode('c'), Direction::Click);
    let release = enigo.key(modifier, Direction::Release);
    click.map_err(|error| anyhow::anyhow!("Impossibile inviare copia: {error:?}"))?;
    release.map_err(|error| anyhow::anyhow!("Impossibile rilasciare il modificatore copia: {error:?}"))?;
    Ok(())
}

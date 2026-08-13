// autostart.rs — User-level start-at-login integration.
// Uses only standard per-user startup locations; no administrator privileges.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

fn current_executable() -> Result<PathBuf> {
    std::env::current_exe().context("Cannot determine G-Type executable path")
}

#[cfg(target_os = "linux")]
fn entry_path() -> Result<PathBuf> {
    let base = directories::BaseDirs::new().context("Cannot determine home directory")?;
    Ok(base.config_dir().join("autostart").join("g-type.desktop"))
}

#[cfg(target_os = "macos")]
fn entry_path() -> Result<PathBuf> {
    let base = directories::BaseDirs::new().context("Cannot determine home directory")?;
    Ok(base
        .home_dir()
        .join("Library")
        .join("LaunchAgents")
        .join("net.intelligenzaartificiale.g-type.plist"))
}

#[cfg(target_os = "windows")]
fn entry_path() -> Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").context("APPDATA is not available")?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup")
        .join("g-type.cmd"))
}

pub fn is_enabled() -> Result<bool> {
    Ok(entry_path()?.exists())
}

pub fn set_enabled(enabled: bool) -> Result<()> {
    let path = entry_path()?;
    if !enabled {
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Cannot remove autostart entry {}", path.display()))?;
        }
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create autostart directory {}", parent.display()))?;
    }

    let exe = current_executable()?;
    fs::write(&path, entry_contents(&exe))
        .with_context(|| format!("Cannot write autostart entry {}", path.display()))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn entry_contents(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=G-Type\nComment=Global voice dictation\nExec=\"{}\"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exe.display()
    )
}

#[cfg(target_os = "macos")]
fn entry_contents(exe: &Path) -> String {
    let program = xml_escape(&exe.to_string_lossy());
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>net.intelligenzaartificiale.g-type</string><key>ProgramArguments</key><array><string>{program}</string></array><key>RunAtLoad</key><true/></dict></plist>\n"
    )
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "windows")]
fn entry_contents(exe: &Path) -> String {
    format!("@echo off\r\nstart \"\" \"{}\"\r\n", exe.display())
}

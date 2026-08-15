use serde::{Deserialize, Serialize};
use std::process::Command;

const MAX_TITLE_CHARS: usize = 180;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppContext {
    pub id: String,
    pub app_name: String,
    pub app_identifier: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
}

impl AppContext {
    pub fn display_name(&self) -> String {
        match self.surface.as_deref() {
            Some(surface) if !surface.eq_ignore_ascii_case(&self.app_name) => {
                format!("{} · {}", self.app_name, surface)
            }
            _ => self.app_name.clone(),
        }
    }
}

/// Capture the foreground application once, at operation start. This is always
/// best-effort: context failure must never block dictation or editing.
pub fn capture() -> Option<AppContext> {
    #[cfg(target_os = "windows")]
    let raw = capture_windows();
    #[cfg(target_os = "macos")]
    let raw = capture_macos();
    #[cfg(target_os = "linux")]
    let raw = capture_linux();
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let raw: Option<(String, String, Option<String>)> = None;

    raw.and_then(|(app_name, app_identifier, title)| normalize(app_name, app_identifier, title))
}

fn normalize(
    app_name: String,
    app_identifier: String,
    title: Option<String>,
) -> Option<AppContext> {
    let app_name = sanitize(&app_name, 80);
    let app_identifier = sanitize(&app_identifier, 160);
    if app_name.is_empty() && app_identifier.is_empty() {
        return None;
    }

    let app_name = if app_name.is_empty() {
        app_identifier.clone()
    } else {
        app_name
    };
    let app_identifier = if app_identifier.is_empty() {
        slug(&app_name)
    } else {
        app_identifier
    };
    let window_title = title
        .map(|value| sanitize(&value, MAX_TITLE_CHARS))
        .filter(|value| !value.is_empty());

    let browser = browser_family(&app_name, &app_identifier);
    let surface = browser
        .and_then(|browser| infer_browser_surface(window_title.as_deref(), browser))
        .map(str::to_string);

    let id = match (browser, surface.as_deref()) {
        (Some(browser), Some(surface)) => format!("web:{browser}:{}", slug(surface)),
        _ => format!("app:{}", slug(&app_identifier)),
    };

    Some(AppContext {
        id,
        app_name,
        app_identifier,
        window_title,
        surface,
    })
}

fn sanitize(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .map(|ch| {
            if matches!(ch, '\n' | '\r' | '\t') {
                ' '
            } else {
                ch
            }
        })
        .filter(|ch| !ch.is_control())
        .take(max_chars)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

fn browser_family(app_name: &str, identifier: &str) -> Option<&'static str> {
    let value = format!("{app_name} {identifier}").to_ascii_lowercase();
    if value.contains("chrome") || value.contains("chromium") {
        Some("chrome")
    } else if value.contains("edge") || value.contains("msedge") {
        Some("edge")
    } else if value.contains("firefox") {
        Some("firefox")
    } else if value.contains("brave") {
        Some("brave")
    } else if value.contains("safari") {
        Some("safari")
    } else if value.contains("vivaldi") {
        Some("vivaldi")
    } else {
        None
    }
}

fn infer_browser_surface(title: Option<&str>, _browser: &str) -> Option<&'static str> {
    let title = title?.to_ascii_lowercase();
    [
        ("gmail", "Gmail"),
        ("google docs", "Google Docs"),
        ("google sheets", "Google Sheets"),
        ("google slides", "Google Slides"),
        ("outlook", "Outlook"),
        ("whatsapp", "WhatsApp"),
        ("slack", "Slack"),
        ("telegram", "Telegram"),
        ("discord", "Discord"),
        ("notion", "Notion"),
        ("chatgpt", "ChatGPT"),
        ("github", "GitHub"),
        ("youtube", "YouTube"),
        ("google meet", "Google Meet"),
        ("microsoft teams", "Microsoft Teams"),
    ]
    .iter()
    .find(|(needle, _)| title.contains(needle))
    .map(|(_, label)| *label)
}

#[cfg(target_os = "windows")]
fn capture_windows() -> Option<(String, String, Option<String>)> {
    let script = r#"
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class GTypeWin32 {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
}
'@;
$h = [GTypeWin32]::GetForegroundWindow();
if ($h -eq [IntPtr]::Zero) { exit 1 }
[uint32]$procId = 0;
[GTypeWin32]::GetWindowThreadProcessId($h, [ref]$procId) | Out-Null;
$p = Get-Process -Id $procId -ErrorAction Stop;
$sb = New-Object System.Text.StringBuilder 1024;
[GTypeWin32]::GetWindowText($h, $sb, $sb.Capacity) | Out-Null;
$identifier = $p.ProcessName;
try { if ($p.Path) { $identifier = $p.Path } } catch {}
Write-Output ($p.ProcessName + "`t" + $identifier + "`t" + $sb.ToString());
"#;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_tabbed(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "macos")]
fn capture_macos() -> Option<(String, String, Option<String>)> {
    // JXA uses AppKit's NSWorkspace.frontmostApplication for app identity and
    // does not require Accessibility permission merely to know the active app.
    // Window title stays optional to preserve the best-effort contract.
    let script = r#"ObjC.import('AppKit');
var app = $.NSWorkspace.sharedWorkspace.frontmostApplication;
if (!app) { $.exit(1); }
var name = ObjC.unwrap(app.localizedName) || '';
var bundle = ObjC.unwrap(app.bundleIdentifier) || name;
console.log(name + '\t' + bundle + '\t');"#;
    let output = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_tabbed(&String::from_utf8_lossy(&output.stdout))
        .or_else(|| parse_tabbed(&String::from_utf8_lossy(&output.stderr)))
}

#[cfg(target_os = "linux")]
fn capture_linux() -> Option<(String, String, Option<String>)> {
    // EWMH _NET_ACTIVE_WINDOW covers X11/XWayland. Native Wayland compositors
    // may intentionally expose no portable equivalent; then context is None.
    let root = Command::new("xprop")
        .args(["-root", "_NET_ACTIVE_WINDOW"])
        .output()
        .ok()?;
    if !root.status.success() {
        return None;
    }
    let root_text = String::from_utf8_lossy(&root.stdout);
    let window_id = root_text
        .split_whitespace()
        .find(|part| part.starts_with("0x"))?
        .trim_end_matches(',');
    if window_id == "0x0" {
        return None;
    }

    let props = Command::new("xprop")
        .args([
            "-id",
            window_id,
            "_NET_WM_NAME",
            "WM_NAME",
            "WM_CLASS",
            "_NET_WM_PID",
        ])
        .output()
        .ok()?;
    if !props.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&props.stdout);
    let title =
        property_string(&text, "_NET_WM_NAME").or_else(|| property_string(&text, "WM_NAME"));
    let class = property_string(&text, "WM_CLASS").unwrap_or_default();
    let pid = property_u32(&text, "_NET_WM_PID");
    let process_name = pid
        .and_then(|pid| std::fs::read_to_string(format!("/proc/{pid}/comm")).ok())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    let app_name = process_name
        .clone()
        .or_else(|| last_quoted_value(&class))
        .unwrap_or_else(|| "Linux application".to_string());
    let identifier = last_quoted_value(&class)
        .or(process_name)
        .unwrap_or_else(|| app_name.clone());
    Some((app_name, identifier, title))
}

#[cfg(target_os = "linux")]
fn property_string(text: &str, name: &str) -> Option<String> {
    text.lines()
        .find(|line| line.starts_with(name))
        .and_then(|line| {
            line.split_once('=')
                .map(|(_, value)| value.trim().to_string())
        })
        .and_then(|value| {
            let unquoted = value.trim_matches('"').trim().to_string();
            (!unquoted.is_empty() && unquoted != "not found.").then_some(unquoted)
        })
}

#[cfg(target_os = "linux")]
fn property_u32(text: &str, name: &str) -> Option<u32> {
    text.lines()
        .find(|line| line.starts_with(name))
        .and_then(|line| line.split_once('='))
        .and_then(|(_, value)| value.trim().parse().ok())
}

#[cfg(target_os = "linux")]
fn last_quoted_value(value: &str) -> Option<String> {
    value
        .split(',')
        .next_back()
        .map(|part| part.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn parse_tabbed(value: &str) -> Option<(String, String, Option<String>)> {
    let mut parts = value.trim().splitn(3, '\t');
    let app = parts.next()?.trim().to_string();
    let identifier = parts.next().unwrap_or(&app).trim().to_string();
    let title = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    (!app.is_empty()).then_some((app, identifier, title))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_untrusted_titles() {
        assert_eq!(sanitize("  hello\nworld\t  ", 50), "hello world");
    }

    #[test]
    fn browser_surface_separates_gmail_from_chrome() {
        let context = normalize(
            "Google Chrome".into(),
            "chrome".into(),
            Some("Inbox - user@example.com - Gmail".into()),
        )
        .unwrap();
        assert_eq!(context.id, "web:chrome:gmail");
        assert_eq!(context.surface.as_deref(), Some("Gmail"));
    }

    #[test]
    fn native_apps_get_stable_application_id() {
        let context = normalize("Code".into(), "code".into(), Some("main.rs".into())).unwrap();
        assert_eq!(context.id, "app:code");
    }
}

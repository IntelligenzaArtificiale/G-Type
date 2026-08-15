// overlay.rs — Floating pill overlay shown during recording/processing.
// Visibility is controlled by showing/hiding the native window — no CSS
// opacity tricks, no compositor/transparency requirement. Works on any
// X11 or Wayland setup without a compositor.

use crate::ui_bridge::UiCommand;
use anyhow::Result;
use std::sync::mpsc::Sender;
use winit::window::Window;
use wry::{WebView, WebViewBuilder};

// Pill HTML — solid dark background, no transparency.
// Window itself is hidden when idle; shown on recording/processing.
const PILL_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  html, body {
    width: 100%; height: 100%;
    background: #0e0e0e;
    overflow: hidden;
  }
  body {
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    -webkit-user-select: none;
    user-select: none;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 10px 20px;
    background: #141414;
    border-radius: 999px;
    border: 1.5px solid #333;
    box-shadow: 0 8px 32px rgba(0,0,0,0.8);
    transition: border-color 0.18s ease;
  }
  .pill.recording  { border-color: #ef4444; }
  .pill.processing { border-color: #3b82f6; }

  .dot {
    width: 9px; height: 9px;
    border-radius: 50%;
    background: #ef4444;
    flex-shrink: 0;
  }
  .dot.recording  { animation: pulse-red  1.3s ease-in-out infinite; }
  .dot.processing { background: #3b82f6; animation: pulse-blue 1.0s ease-in-out infinite; }

  .label {
    color: #f5f5f5;
    font-size: 13px;
    font-weight: 500;
    letter-spacing: 0.01em;
    white-space: nowrap;
  }

  @keyframes pulse-red  {
    0%,100% { box-shadow: 0 0 0 0 rgba(239,68,68,0.7); }
    55%      { box-shadow: 0 0 0 6px rgba(239,68,68,0); }
  }
  @keyframes pulse-blue {
    0%,100% { box-shadow: 0 0 0 0 rgba(59,130,246,0.7); }
    55%      { box-shadow: 0 0 0 6px rgba(59,130,246,0); }
  }
</style>
</head>
<body>
<div class="pill recording" id="pill">
  <div class="dot recording" id="dot"></div>
  <span class="label" id="label">Recording...</span>
</div>
<script>
  var pill  = document.getElementById('pill');
  var dot   = document.getElementById('dot');
  var label = document.getElementById('label');

  window.set_state = function(state) {
    pill.className = 'pill ' + state;
    dot.className  = 'dot '  + state;
    label.textContent = state === 'recording' ? 'Recording...' : 'Transcribing...';
  };
</script>
</body>
</html>"#;

pub struct OverlayManager {
    /// Owned window — MUST NOT be dropped. wry only borrows the raw handle.
    window: Window,
    #[allow(dead_code)]
    webview: WebView,
    #[allow(dead_code)]
    ui_tx: Sender<UiCommand>,
}

impl OverlayManager {
    pub fn new(window: Window, ui_tx: Sender<UiCommand>) -> Result<Self> {
        // Position at top-center of the current monitor.
        if let Some(monitor) = window.current_monitor() {
            let screen = monitor.size();
            let scale = monitor.scale_factor();
            let pill_w = 320.0_f64;
            let x = (screen.width as f64 / scale - pill_w) / 2.0;
            window.set_outer_position(winit::dpi::LogicalPosition::new(x, 24.0));
        }

        // Start hidden — shown only when recording/processing begins.
        window.set_visible(false);
        // Click-through always: the pill is informational only.
        let _ = window.set_cursor_hittest(false);

        // No transparency required — solid background avoids compositor issues.
        let webview = WebViewBuilder::new().with_html(PILL_HTML).build(&window)?;

        Ok(Self {
            window,
            webview,
            ui_tx,
        })
    }

    pub fn set_recording(&self) -> Result<()> {
        let _ = self
            .webview
            .evaluate_script("window.set_state('recording')");
        self.window.set_visible(true);
        Ok(())
    }

    pub fn set_processing(&self) -> Result<()> {
        let _ = self
            .webview
            .evaluate_script("window.set_state('processing')");
        self.window.set_visible(true);
        Ok(())
    }

    pub fn set_idle(&self) -> Result<()> {
        self.window.set_visible(false);
        Ok(())
    }
}

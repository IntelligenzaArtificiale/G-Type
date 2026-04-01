// overlay.rs — Floating pill overlay shown during recording/processing.
// Always-visible window (avoids GTK show/hide race on Linux); visual state
// controlled entirely via CSS transitions + JS. Click-through in idle state
// via set_cursor_hittest(false) so the invisible pill never blocks input.

use anyhow::Result;
use wry::{WebViewBuilder, WebView};
use std::sync::mpsc::Sender;
use winit::window::Window;
use crate::ui_bridge::UiCommand;

// Pill HTML is embedded in the binary — no network dependency, no CDN.
// CSS controls visibility: idle = opacity 0 + scale 0.85 (invisible, click-through).
// Recording/processing = opacity 1 + scale 1 (visible, interactive).
const PILL_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  html, body { width: 100%; height: 100%; background: transparent; overflow: hidden; }
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
    padding: 9px 18px;
    background: rgba(12, 12, 12, 0.90);
    backdrop-filter: blur(20px) saturate(160%);
    -webkit-backdrop-filter: blur(20px) saturate(160%);
    border-radius: 999px;
    border: 1px solid rgba(255, 255, 255, 0.09);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.6), 0 0 0 1px rgba(255,255,255,0.04);
    /* Hidden by default — appears via JS state change */
    opacity: 0;
    transform: scale(0.82) translateY(-6px);
    pointer-events: none;
    transition: opacity 0.22s cubic-bezier(0.4, 0, 0.2, 1),
                transform 0.22s cubic-bezier(0.4, 0, 0.2, 1),
                border-color 0.22s ease;
  }
  .pill.visible {
    opacity: 1;
    transform: scale(1) translateY(0);
    pointer-events: auto;
  }
  .pill.recording { border-color: rgba(239, 68, 68, 0.30); }
  .pill.processing { border-color: rgba(59, 130, 246, 0.30); }

  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
    background: #ef4444;
    transition: background 0.22s ease;
  }
  .dot.recording {
    animation: pulse-red 1.3s cubic-bezier(0.4, 0, 0.6, 1) infinite;
  }
  .dot.processing {
    background: #3b82f6;
    animation: pulse-blue 1.0s ease-in-out infinite;
  }

  .label {
    color: #e5e5e5;
    font-size: 13px;
    font-weight: 500;
    letter-spacing: -0.01em;
    white-space: nowrap;
  }

  @keyframes pulse-red {
    0%, 100% { box-shadow: 0 0 0 0 rgba(239, 68, 68, 0.55); }
    55%       { box-shadow: 0 0 0 6px rgba(239, 68, 68, 0); }
  }
  @keyframes pulse-blue {
    0%, 100% { box-shadow: 0 0 0 0 rgba(59, 130, 246, 0.55); }
    55%       { box-shadow: 0 0 0 6px rgba(59, 130, 246, 0); }
  }
</style>
</head>
<body>
<div class="pill" id="pill">
  <div class="dot" id="dot"></div>
  <span class="label" id="label">Recording...</span>
</div>
<script>
  var pill  = document.getElementById('pill');
  var dot   = document.getElementById('dot');
  var label = document.getElementById('label');

  window.set_state = function(state) {
    // Reset
    pill.className = 'pill';
    dot.className  = 'dot';

    if (state === 'recording') {
      pill.className = 'pill visible recording';
      dot.className  = 'dot recording';
      label.textContent = 'Recording...';
    } else if (state === 'processing') {
      pill.className = 'pill visible processing';
      dot.className  = 'dot processing';
      label.textContent = 'Transcribing...';
    }
    // 'idle' → just reset = invisible (opacity 0 via CSS default)
  };
</script>
</body>
</html>"#;

pub struct OverlayManager {
    /// Owned window — MUST NOT be dropped. wry only borrows &window to extract
    /// the raw OS handle; it does NOT keep the Window alive internally.
    window: Window,
    #[allow(dead_code)]
    webview: WebView,
    #[allow(dead_code)]
    ui_tx: Sender<UiCommand>,
}

impl OverlayManager {
    /// Takes ownership of `window`. The window is created visible but the pill
    /// inside it starts transparent (opacity: 0 via CSS). This avoids GTK
    /// show/hide race conditions on Linux where XMapWindow and WebKitGTK
    /// initialization can conflict.
    pub fn new(window: Window, ui_tx: Sender<UiCommand>) -> Result<Self> {
        // Position at top-center of the current monitor.
        if let Some(monitor) = window.current_monitor() {
            let screen = monitor.size();
            let scale  = monitor.scale_factor();
            let pill_w = 320.0_f64;
            let x = (screen.width as f64 / scale - pill_w) / 2.0;
            window.set_outer_position(winit::dpi::LogicalPosition::new(x, 20.0));
        }

        // Idle state: window is transparent + click-through so it never
        // occludes the user's work while no dictation is happening.
        let _ = window.set_cursor_hittest(false);

        let webview = WebViewBuilder::new()
            .with_transparent(true)
            .with_html(PILL_HTML)
            .build(&window)?;

        Ok(Self { window, webview, ui_tx })
    }

    pub fn set_recording(&self) -> Result<()> {
        // Restore click-target before animating in.
        let _ = self.window.set_cursor_hittest(true);
        let _ = self.webview.evaluate_script("window.set_state('recording')");
        Ok(())
    }

    pub fn set_processing(&self) -> Result<()> {
        let _ = self.window.set_cursor_hittest(true);
        let _ = self.webview.evaluate_script("window.set_state('processing')");
        Ok(())
    }

    pub fn set_idle(&self) -> Result<()> {
        // Fade out first (CSS transition = 0.22s), then disable hit-testing.
        let _ = self.webview.evaluate_script("window.set_state('idle')");
        let _ = self.window.set_cursor_hittest(false);
        Ok(())
    }
}

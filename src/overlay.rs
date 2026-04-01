use anyhow::Result;
use winit::window::WindowLevel;
use wry::{WebViewBuilder, WebView};
use std::sync::mpsc::Sender;
use crate::ui_bridge::{UiCommand, DaemonEvent};

pub struct OverlayManager {
    #[allow(dead_code)]
    webview: WebView,
    ui_tx: Sender<UiCommand>,
}

impl OverlayManager {
    pub fn new(window: &winit::window::Window, ui_tx: Sender<UiCommand>) -> Result<Self> {
        // Pill HTML
        let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <script src="https://cdn.tailwindcss.com"></script>
            <style>
                body { background: transparent; overflow: hidden; margin: 0; padding: 0; }
                .pill {
                    background: rgba(10, 10, 10, 0.85);
                    backdrop-filter: blur(12px);
                    border-radius: 28px;
                    border: 1px solid rgba(255, 255, 255, 0.1);
                    color: white;
                    display: flex;
                    align-items: center;
                    padding: 8px 16px;
                    font-family: system-ui, sans-serif;
                    box-shadow: 0 10px 25px rgba(0,0,0,0.5);
                    height: 100vh;
                    box-sizing: border-box;
                }
                .dot {
                    width: 12px; height: 12px; border-radius: 50%;
                    background: #ef4444; margin-right: 12px;
                    animation: pulse 2s infinite;
                }
                @keyframes pulse {
                    0% { box-shadow: 0 0 0 0 rgba(239, 68, 68, 0.7); }
                    70% { box-shadow: 0 0 0 10px rgba(239, 68, 68, 0); }
                    100% { box-shadow: 0 0 0 0 rgba(239, 68, 68, 0); }
                }
            </style>
        </head>
        <body>
            <div class="pill">
                <div class="dot" id="dot"></div>
                <span id="text" class="text-sm font-medium tracking-wide">Recording...</span>
            </div>
            <script>
                window.set_state = function(state) {
                    if (state === 'processing') {
                        document.getElementById('dot').style.background = '#3b82f6';
                        document.getElementById('text').innerText = 'Transcribing...';
                    } else if (state === 'recording') {
                        document.getElementById('dot').style.background = '#ef4444';
                        document.getElementById('text').innerText = 'Recording...';
                    } else if (state === 'idle') {
                        // For idle, we might not show it, but just in case
                        document.getElementById('dot').style.background = '#10b981';
                        document.getElementById('text').innerText = 'Idle';
                    }
                }
            </script>
        </body>
        </html>
        "#;

        let webview_builder = WebViewBuilder::new()
            .with_transparent(true)
            .with_html(html);

        let webview = webview_builder.build(window)?;

        Ok(Self { webview, ui_tx })
    }

    pub fn set_recording(&self) -> Result<()> {
        let _ = self.webview.evaluate_script("window.set_state('recording')");
        Ok(())
    }

    pub fn set_processing(&self) -> Result<()> {
        let _ = self.webview.evaluate_script("window.set_state('processing')");
        Ok(())
    }
    
    pub fn set_idle(&self) -> Result<()> {
        let _ = self.webview.evaluate_script("window.set_state('idle')");
        Ok(())
    }
}

use anyhow::Result;
use muda::{Menu, MenuItem, PredefinedMenuItem};
use tracing::info;
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::ui_bridge::UiCommand;

pub struct TrayManager {
    pub tray: TrayIcon,
}

impl TrayManager {
    pub fn new(_ui_tx: std::sync::mpsc::Sender<UiCommand>) -> Result<Self> {
        let menu = Menu::new();

        let settings_i = MenuItem::new("Settings...", true, None);
        let quit_i = MenuItem::new("Quit G-Type", true, None);

        menu.append(&settings_i)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit_i)?;

        let mut rgba = Vec::with_capacity(16 * 16 * 4);
        for _ in 0..(16 * 16) {
            rgba.push(20);
            rgba.push(200);
            rgba.push(20);
            rgba.push(255);
        }
        let icon = Icon::from_rgba(rgba, 16, 16)?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("G-Type: Ready")
            .with_icon(icon)
            .build()?;

        info!("Tray icon instanziata");
        Ok(Self { tray })
    }

    pub fn set_recording(&self, profile: &str) {
        let mut rgba = Vec::with_capacity(16 * 16 * 4);
        for _ in 0..(16 * 16) {
            rgba.push(200);
            rgba.push(20);
            rgba.push(20);
            rgba.push(255);
        }
        if let Ok(icon) = Icon::from_rgba(rgba, 16, 16) {
            let _ = self.tray.set_icon(Some(icon));
        }
        let _ = self
            .tray
            .set_tooltip(Some(format!("G-Type: Recording [{}]", profile)));
    }

    pub fn set_processing(&self) {
        let mut rgba = Vec::with_capacity(16 * 16 * 4);
        for _ in 0..(16 * 16) {
            rgba.push(20);
            rgba.push(20);
            rgba.push(200);
            rgba.push(255);
        }
        if let Ok(icon) = Icon::from_rgba(rgba, 16, 16) {
            let _ = self.tray.set_icon(Some(icon));
        }
        let _ = self.tray.set_tooltip(Some("G-Type: Processing..."));
    }

    pub fn set_idle(&self) {
        let mut rgba = Vec::with_capacity(16 * 16 * 4);
        for _ in 0..(16 * 16) {
            rgba.push(20);
            rgba.push(200);
            rgba.push(20);
            rgba.push(255);
        }
        if let Ok(icon) = Icon::from_rgba(rgba, 16, 16) {
            let _ = self.tray.set_icon(Some(icon));
        }
        let _ = self.tray.set_tooltip(Some("G-Type: Ready"));
    }
}

// app.rs — Finite State Machine orchestrating the G-Type daemon.
// States: Idle → Recording → Processing → Injecting → Idle
// All inter-thread communication via tokio::sync::mpsc channels.

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use winit::event_loop::EventLoopProxy;

use crate::audio;
use crate::config::{ConfigV2, Profile};
use crate::injector;
use crate::input::{self, InputRx, InputSignal, InputTx};
use crate::providers;
use crate::transforms;
use crate::ui_bridge::{DaemonEvent, DaemonState, ProfileInfo, UiCommand};

/// Run the main event loop.
///
/// The configuration is shared with the web settings server. Changes made from
/// the dashboard are picked up at runtime, including profile hotkeys, without
/// requiring a daemon restart.
pub async fn run_with_ui(
    config: Arc<RwLock<ConfigV2>>,
    ui_proxy: EventLoopProxy<DaemonEvent>,
    ui_rx: std::sync::mpsc::Receiver<UiCommand>,
) -> Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));

    let initial_cfg = config.read().await.clone();
    let initial_hotkeys = parsed_hotkeys(&initial_cfg);
    let mut hotkey_signature = profile_hotkey_signature(&initial_cfg);
    let shared_hotkeys = input::SharedHotkeys::new(initial_hotkeys);

    let (input_tx, mut input_rx): (InputTx, InputRx) = mpsc::channel(32);

    let shutdown_clone = shutdown.clone();
    let _input_handle = crate::input::spawn_listener(
        input_tx,
        shutdown_clone,
        shared_hotkeys.clone(),
    )
    .context("Failed to spawn keyboard listener")?;

    let shutdown_sig = shutdown.clone();
    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate()).ok();
            tokio::select! {
                _ = ctrl_c => {},
                _ = async {
                    if let Some(ref mut s) = sigterm { s.recv().await; }
                    else { std::future::pending::<()>().await; }
                } => {},
            }
        }
        #[cfg(not(unix))]
        {
            let _ = ctrl_c.await;
        }
        info!("Shutdown signal received, cleaning up...");
        shutdown_sig.store(true, Ordering::SeqCst);
    });

    info!(profiles = initial_cfg.profiles.len(), "Ready — hold hotkey to dictate.");
    let _ = ui_proxy.send_event(DaemonEvent::StateChanged(DaemonState::Idle));

    let mut last_config_refresh = tokio::time::Instant::now();
    let ui_rx = ui_rx;

    loop {
        if shutdown.load(Ordering::SeqCst) {
            info!("Shutting down gracefully.");
            return Ok(());
        }

        // Refresh hotkeys periodically so dashboard edits become active live.
        if last_config_refresh.elapsed() >= std::time::Duration::from_millis(400) {
            let cfg = config.read().await.clone();
            let signature = profile_hotkey_signature(&cfg);
            if signature != hotkey_signature {
                shared_hotkeys.update(parsed_hotkeys(&cfg));
                hotkey_signature = signature;
                let _ = ui_proxy.send_event(DaemonEvent::ProfilesUpdated(
                    cfg.profiles
                        .iter()
                        .map(|p| ProfileInfo {
                            name: p.name.clone(),
                            model_name: p.model.clone(),
                            active: false,
                        })
                        .collect(),
                ));
                info!("Runtime profile configuration refreshed");
            }
            last_config_refresh = tokio::time::Instant::now();
        }

        while let Ok(cmd) = ui_rx.try_recv() {
            match cmd {
                UiCommand::Quit => shutdown.store(true, Ordering::SeqCst),
                UiCommand::OpenSettings => {
                    if let Err(e) = open::that("http://127.0.0.1:9741") {
                        error!("Failed to open settings: {}", e);
                    }
                }
                UiCommand::SwitchProfile(name) => {
                    info!("Profile selected over UI: {}", name);
                }
            }
        }

        match input_rx.try_recv() {
            Ok(InputSignal::Start(profile_name)) => {
                let snapshot = config.read().await.clone();
                let profile = snapshot
                    .profiles
                    .iter()
                    .find(|p| p.name == profile_name)
                    .cloned();

                if let Some(profile) = profile {
                    info!(profile = %profile_name, "🎤 Recording...");
                    if snapshot.global.sound_enabled {
                        crate::audio_feedback::play_start_beep();
                    }

                    let _ = ui_proxy.send_event(DaemonEvent::StateChanged(
                        DaemonState::Recording {
                            profile: profile.name.clone(),
                        },
                    ));
                    let _ = ui_proxy.send_event(DaemonEvent::ProfileActivated(ProfileInfo {
                        name: profile.name.clone(),
                        model_name: profile.model.clone(),
                        active: true,
                    }));

                    state_recording(&snapshot, &profile, &mut input_rx, &ui_proxy).await;
                    let _ = ui_proxy.send_event(DaemonEvent::StateChanged(DaemonState::Idle));
                } else {
                    warn!(profile = %profile_name, "Unknown profile triggered");
                }
            }
            Ok(InputSignal::Stop) => continue,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                error!("Input channel closed unexpectedly");
                return Ok(());
            }
            Err(mpsc::error::TryRecvError::Empty) => {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }
}

fn parsed_hotkeys(config: &ConfigV2) -> Vec<(input::Hotkey, String)> {
    config
        .profiles
        .iter()
        .filter_map(|p| match input::parse_hotkey(&p.hotkey) {
            Ok(hk) => Some((hk, p.name.clone())),
            Err(e) => {
                warn!(profile = %p.name, hotkey = %p.hotkey, %e, "Ignoring invalid hotkey");
                None
            }
        })
        .collect()
}

fn profile_hotkey_signature(config: &ConfigV2) -> Vec<(String, String)> {
    config
        .profiles
        .iter()
        .map(|p| (p.name.clone(), p.hotkey.clone()))
        .collect()
}

async fn state_recording(
    config: &ConfigV2,
    profile: &Profile,
    input_rx: &mut InputRx,
    ui_proxy: &EventLoopProxy<DaemonEvent>,
) {
    debug!("Capturing audio to buffer");

    let (audio_tx, audio_rx) = audio::audio_channel();
    let recording_flag = Arc::new(AtomicBool::new(true));

    let recording_flag_clone = recording_flag.clone();
    let audio_thread_handle = match audio::start_capture(
        audio_tx,
        recording_flag_clone,
        config.global.audio_device.clone(),
    ) {
        Ok(handle) => handle,
        Err(e) => {
            error!(%e, "Failed to start audio capture");
            warn!("Returning to idle due to audio capture failure");
            return;
        }
    };

    let collector_handle = tokio::task::spawn_blocking(move || {
        let mut all_samples = Vec::<i16>::with_capacity(480_000);
        while let Ok(chunk) = audio_rx.recv() {
            all_samples.extend_from_slice(&chunk);
        }
        all_samples
    });

    loop {
        match input_rx.recv().await {
            Some(InputSignal::Stop) => break,
            Some(InputSignal::Start(_)) => continue,
            None => {
                error!("Input channel closed during recording");
                recording_flag.store(false, Ordering::Relaxed);
                collector_handle.abort();
                return;
            }
        }
    }

    recording_flag.store(false, Ordering::Relaxed);

    let all_samples = match collector_handle.await {
        Ok(samples) => samples,
        Err(e) => {
            error!(%e, "Audio collector task failed");
            return;
        }
    };

    if let Err(e) = audio_thread_handle.join() {
        error!("Audio capture thread panicked: {:?}", e);
    }

    let duration = all_samples.len() as f64 / 16_000.0;
    info!(duration = format!("{:.1}s", duration), "⏹ Stopped. Transcribing...");
    if config.global.sound_enabled {
        crate::audio_feedback::play_stop_beep();
    }

    if all_samples.is_empty() {
        warn!("No audio captured, skipping transcription");
        return;
    }

    let _ = ui_proxy.send_event(DaemonEvent::StateChanged(DaemonState::Processing {
        profile: profile.name.clone(),
    }));

    let provider = match providers::create_provider(profile, &config.keys) {
        Ok(p) => p,
        Err(e) => {
            error!(%e, "Failed to create provider");
            if config.global.sound_enabled {
                crate::audio_feedback::play_error_beep();
            }
            return;
        }
    };

    let (transcription, usage) = match provider
        .transcribe(&all_samples, &config.global.language)
        .await
    {
        Ok(result) => result,
        Err(e) => {
            error!(%e, "Transcription failed");
            if config.global.sound_enabled {
                crate::audio_feedback::play_error_beep();
            }
            return;
        }
    };

    if transcription.is_empty() {
        warn!("Empty transcription received, skipping injection");
        return;
    }

    let final_text = if !profile.transforms.is_empty() {
        transforms::run_pipeline(
            &profile.transforms,
            &transcription,
            &config.global.language,
        )
        .await
    } else {
        transcription.clone()
    };

    let record = crate::tracking::build_record(&profile.model, duration, &usage, &final_text);
    let log_line = crate::tracking::format_log_line(&record, &config.global.currency);
    info!("{}", log_line);

    if let Err(e) = crate::tracking::append_record(&record) {
        warn!(%e, "Failed to save tracking record (non-fatal)");
    }

    let text = final_text.clone();
    let inject_result = tokio::task::spawn_blocking(move || injector::inject(&text)).await;

    match inject_result {
        Ok(Ok(())) => info!(text = %truncate(&final_text, 80), "✅ Injected"),
        Ok(Err(e)) => error!(%e, "Text injection failed"),
        Err(e) => error!(%e, "Injection task panicked"),
    }
}

/// UTF-8 safe truncation for log display.
fn truncate(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let prefix: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}…", prefix)
    } else {
        prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_ascii() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello…");
    }

    #[test]
    fn test_truncate_utf8_is_char_boundary_safe() {
        assert_eq!(truncate("A me così è già", 8), "A me cos…");
        assert_eq!(truncate("èèè", 2), "èè…");
    }
}

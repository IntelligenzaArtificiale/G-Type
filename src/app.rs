// app.rs — Finite State Machine orchestrating the G-Type daemon.
// States: Idle → Recording → Processing → Injecting → Idle
// All inter-thread communication via tokio::sync::mpsc channels.

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use winit::event_loop::EventLoopProxy;
use crate::ui_bridge::{DaemonEvent, DaemonState, ProfileInfo, UiCommand};

use crate::audio;
use crate::config::{ConfigV2, Profile};
use crate::injector;
use crate::input::{self, InputRx, InputSignal, InputTx};
use crate::providers;
use crate::transforms;

/// FSM states for the daemon.

/// Run the main event loop.
///
/// This function owns the FSM and coordinates all subsystems:
/// - Input listener (keyboard hooks)
/// - Audio capture
/// - Network (WebSocket to Gemini)
/// - Text injection
pub async fn run_with_ui(
    config: ConfigV2,
    ui_proxy: EventLoopProxy<DaemonEvent>,
    mut ui_rx: std::sync::mpsc::Receiver<UiCommand>
) -> Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));

    // Build shared hotkeys from all profiles
    let hotkey_profiles: Vec<(input::Hotkey, String)> = config.profiles.iter()
        .filter_map(|p| {
            input::parse_hotkey(&p.hotkey).ok().map(|hk| (hk, p.name.clone()))
        })
        .collect();

    let shared_hotkeys = input::SharedHotkeys::new(hotkey_profiles);

    // Channel for keyboard input signals (Start/Stop)
    let (input_tx, mut input_rx): (InputTx, InputRx) = mpsc::channel(32);

    // Spawn the global keyboard listener on a dedicated OS thread
    let shutdown_clone = shutdown.clone();
    let _input_handle = crate::input::spawn_listener(input_tx, shutdown_clone, shared_hotkeys.clone())
        .context("Failed to spawn keyboard listener")?;

    // Register SIGINT/SIGTERM handler for graceful shutdown
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

    info!(profiles = config.profiles.len(), "Ready — hold hotkey to dictate.");
    
    // Initial UI state
    let _ = ui_proxy.send_event(DaemonEvent::StateChanged(DaemonState::Idle));

    loop {
        if shutdown.load(Ordering::SeqCst) {
            info!("Shutting down gracefully.");
            return Ok(());
        }

        // Drain UI commands (non-blocking)
        while let Ok(cmd) = ui_rx.try_recv() {
            match cmd {
                UiCommand::Quit => {
                    shutdown.store(true, Ordering::SeqCst);
                }
                UiCommand::OpenSettings => {
                    // Open settings UI
                    if let Err(e) = open::that("http://127.0.0.1:9741") {
                        error!("Failed to open settings: {}", e);
                    }
                }
                UiCommand::SwitchProfile(name) => {
                    info!("Switched to profile over UI: {}", name);
                }
            }
        }

        match input_rx.try_recv() {
            Ok(InputSignal::Start(profile_name)) => {
                // Find the matching profile
                let profile = config.profiles.iter().find(|p| p.name == profile_name);

                if let Some(profile) = profile {
                    info!(profile = %profile_name, "🎤 Recording...");
                    if config.global.sound_enabled {
                        crate::audio_feedback::play_start_beep();
                    }
                    
                    let _ = ui_proxy.send_event(DaemonEvent::StateChanged(DaemonState::Recording { 
                        profile: profile.name.clone() 
                    }));
                    let _ = ui_proxy.send_event(DaemonEvent::ProfileActivated(ProfileInfo {
                        name: profile.name.clone(),
                        model_name: profile.model.clone(),
                        active: true,
                    }));

                    // Run the recording→transcribe→inject pipeline
                    state_recording(&config, profile, &mut input_rx, &ui_proxy).await;
                    
                    let _ = ui_proxy.send_event(DaemonEvent::StateChanged(DaemonState::Idle));
                } else {
                    warn!(profile = %profile_name, "Unknown profile triggered");
                }
            }
            Ok(InputSignal::Stop) => {
                // Spurious stop while idle, ignore
                continue;
            }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                error!("Input channel closed unexpectedly");
                return Ok(());
            }
            Err(mpsc::error::TryRecvError::Empty) => {
                // Prevent busy-looping
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }
}

/// Recording state: capture audio to buffer, then send to Gemini REST API.
/// Handles the full lifecycle: Recording → Processing → Injecting → Idle.
async fn state_recording(config: &ConfigV2, profile: &Profile, input_rx: &mut InputRx, ui_proxy: &EventLoopProxy<DaemonEvent>) {
    debug!("Capturing audio to buffer");

    // Audio capture channel — uses std::sync::mpsc (NOT tokio) because
    // the cpal audio callback runs on a non-tokio OS thread.
    let (audio_tx, audio_rx) = audio::audio_channel();

    // Atomic flag to control audio capture thread
    let recording_flag = Arc::new(AtomicBool::new(true));

    // Start audio capture on a dedicated OS thread
    let recording_flag_clone = recording_flag.clone();
    let audio_thread_handle = match audio::start_capture(audio_tx, recording_flag_clone, config.global.audio_device.clone()) {
        Ok(handle) => handle,
        Err(e) => {
            error!(%e, "Failed to start audio capture");
            warn!("Returning to idle due to audio capture failure");
            return;
        }
    };

    // Spawn a blocking task that drains the std::sync::mpsc receiver.
    // This runs on tokio's blocking thread pool so it won't block the async runtime.
    let collector_handle = tokio::task::spawn_blocking(move || {
        // Pre-allocate buffer for ~30 seconds of audio (480,000 samples)
        // to avoid reallocations during recording.
        let mut all_samples = Vec::<i16>::with_capacity(480_000);
        // recv() blocks until a chunk arrives or all senders are dropped
        while let Ok(chunk) = audio_rx.recv() {
            all_samples.extend_from_slice(&chunk);
        }
        all_samples
    });

    // Wait for Stop signal from keyboard (this blocks until hotkey release)
    loop {
        match input_rx.recv().await {
            Some(InputSignal::Stop) => {
                break;
            }
            Some(InputSignal::Start(_)) => {
                // Double press while recording, ignore
                continue;
            }
            None => {
                error!("Input channel closed during recording");
                recording_flag.store(false, Ordering::Relaxed);
                collector_handle.abort();
                return;
            }
        }
    }

    // Stop audio capture — this causes the audio thread to exit its loop,
    // drop the audio_tx sender, which closes the channel, which makes
    // the collector task finish and return the accumulated samples.
    recording_flag.store(false, Ordering::Relaxed);

    // Wait for the collector to finish (it will end once audio_tx is dropped)
    let all_samples = match collector_handle.await {
        Ok(samples) => samples,
        Err(e) => {
            error!(%e, "Audio collector task failed");
            return;
        }
    };

    // Cleanly join the audio capture thread
    if let Err(e) = audio_thread_handle.join() {
        error!("Audio capture thread panicked: {:?}", e);
    }

    let duration = all_samples.len() as f64 / 16_000.0;
    info!(
        duration = format!("{:.1}s", duration),
        "⏹ Stopped. Transcribing..."
    );
    if config.global.sound_enabled {
        crate::audio_feedback::play_stop_beep();
    }

    if all_samples.is_empty() {
        warn!("No audio captured, skipping transcription");
        return;
    }

    let _ = ui_proxy.send_event(DaemonEvent::StateChanged(DaemonState::Processing {
        profile: profile.name.clone()
    }));

    // Pass the active profile and its model
    let provider = match providers::create_provider(profile, &config.keys) {
        Ok(p) => p,
        Err(e) => {
            error!(%e, "Failed to create provider");
            warn!("Returning to idle due to provider error");
            if config.global.sound_enabled {
                crate::audio_feedback::play_error_beep();
            }
            return;
        }
    };

    let (transcription, usage) = match provider.transcribe(&all_samples, &config.global.language).await {
        Ok(result) => result,
        Err(e) => {
            error!(%e, "Transcription failed");
            warn!("Returning to idle due to transcription failure");
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
        transforms::run_pipeline(&profile.transforms, &transcription, &config.global.language).await
    } else {
        transcription.clone()
    };

    // Track cost and usage with the correct model from the active profile
    let record = crate::tracking::build_record(&profile.model, duration, &usage, &final_text);

    let log_line = crate::tracking::format_log_line(&record, &config.global.currency);
    info!("{}", log_line);

    if let Err(e) = crate::tracking::append_record(&record) {
        warn!(%e, "Failed to save tracking record (non-fatal)");
    }

    // Inject the transcribed text
    // Run injection on a blocking thread to avoid blocking the async runtime
    let text = final_text.clone();
    let inject_result = tokio::task::spawn_blocking(move || injector::inject(&text)).await;

    match inject_result {
        Ok(Ok(())) => {
            info!(text = %truncate(&final_text, 80), "✅ Injected");
        }
        Ok(Err(e)) => {
            error!(%e, "Text injection failed");
        }
        Err(e) => {
            error!(%e, "Injection task panicked");
        }
    }
}

/// Truncate a string for log display.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello…");
    }

    #[test]
    fn test_state_display() {
        assert_eq!(format!("{}", State::Idle), "IDLE");
        assert_eq!(format!("{}", State::Recording), "RECORDING");
        assert_eq!(format!("{}", State::Processing), "PROCESSING");
        assert_eq!(format!("{}", State::Injecting), "INJECTING");
    }
}

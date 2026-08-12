// app.rs — Finite State Machine orchestrating the G-Type daemon.
// States: Idle → Recording → Processing → Injecting → Idle
// All inter-thread communication via channels; failures return safely to Idle.

#[path = "recovery.rs"]
pub(crate) mod recovery;

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

const MAX_RECORDING_SECS: u64 = 10 * 60;

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
    let _ = crate::config::take_runtime_dirty();

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
                    if let Some(ref mut signal) = sigterm { signal.recv().await; }
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

    loop {
        if shutdown.load(Ordering::SeqCst) {
            info!("Shutting down gracefully.");
            return Ok(());
        }

        if crate::config::take_runtime_dirty() {
            let cfg = config.read().await.clone();
            let signature = profile_hotkey_signature(&cfg);
            if signature != hotkey_signature {
                shared_hotkeys.update(parsed_hotkeys(&cfg));
                hotkey_signature = signature;
                let _ = ui_proxy.send_event(DaemonEvent::ProfilesUpdated(
                    cfg.profiles
                        .iter()
                        .map(|profile| ProfileInfo {
                            name: profile.name.clone(),
                            model_name: profile.model.clone(),
                            active: false,
                        })
                        .collect(),
                ));
                info!("Runtime profile configuration refreshed");
            }
        }

        while let Ok(command) = ui_rx.try_recv() {
            match command {
                UiCommand::Quit => shutdown.store(true, Ordering::SeqCst),
                UiCommand::OpenSettings => {
                    if let Err(error) = open::that("http://127.0.0.1:9741") {
                        error!(%error, "Failed to open settings");
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
                    .find(|profile| profile.name == profile_name)
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
        .filter_map(|profile| match input::parse_hotkey(&profile.hotkey) {
            Ok(hotkey) => Some((hotkey, profile.name.clone())),
            Err(error) => {
                warn!(profile = %profile.name, hotkey = %profile.hotkey, %error, "Ignoring invalid hotkey");
                None
            }
        })
        .collect()
}

fn profile_hotkey_signature(config: &ConfigV2) -> Vec<(String, String)> {
    config
        .profiles
        .iter()
        .map(|profile| (profile.name.clone(), profile.hotkey.clone()))
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
    let configured_device = config.global.audio_device.clone();

    let audio_thread_handle = match audio::start_capture(
        audio_tx.clone(),
        recording_flag.clone(),
        configured_device.clone(),
    ) {
        Ok(handle) => handle,
        Err(first_error)
            if configured_device
                .as_deref()
                .is_some_and(|name| !name.is_empty() && name != "default") =>
        {
            warn!(
                device = configured_device.as_deref().unwrap_or_default(),
                %first_error,
                "Configured microphone unavailable; falling back to system default"
            );
            match audio::start_capture(audio_tx.clone(), recording_flag.clone(), None) {
                Ok(handle) => handle,
                Err(fallback_error) => {
                    error!(%fallback_error, "Failed to start fallback audio capture");
                    if config.global.sound_enabled {
                        crate::audio_feedback::play_error_beep();
                    }
                    return;
                }
            }
        }
        Err(error) => {
            error!(%error, "Failed to start audio capture");
            if config.global.sound_enabled {
                crate::audio_feedback::play_error_beep();
            }
            return;
        }
    };
    drop(audio_tx);

    let collector_handle = tokio::task::spawn_blocking(move || {
        let mut all_samples = Vec::<i16>::with_capacity(480_000);
        while let Ok(chunk) = audio_rx.recv() {
            all_samples.extend_from_slice(&chunk);
        }
        all_samples
    });

    let watchdog = tokio::time::sleep(std::time::Duration::from_secs(MAX_RECORDING_SECS));
    tokio::pin!(watchdog);
    let mut can_transcribe = true;

    loop {
        tokio::select! {
            signal = input_rx.recv() => {
                match signal {
                    Some(InputSignal::Stop) => break,
                    Some(InputSignal::Start(_)) => continue,
                    None => {
                        error!("Input channel closed during recording");
                        can_transcribe = false;
                        break;
                    }
                }
            }
            _ = &mut watchdog => {
                warn!(max_seconds = MAX_RECORDING_SECS, "Recording watchdog reached; stopping capture safely");
                break;
            }
        }
    }

    recording_flag.store(false, Ordering::Relaxed);

    if let Err(error) = audio_thread_handle.join() {
        error!("Audio capture thread panicked: {:?}", error);
        can_transcribe = false;
    }

    let all_samples = match collector_handle.await {
        Ok(samples) => samples,
        Err(error) => {
            error!(%error, "Audio collector task failed");
            return;
        }
    };

    if !can_transcribe {
        return;
    }

    let duration = all_samples.len() as f64 / 16_000.0;
    info!(duration = format!("{:.1}s", duration), "⏹ Stopped. Transcribing...");
    if config.global.sound_enabled {
        crate::audio_feedback::play_stop_beep();
    }

    if all_samples.is_empty() {
        warn!("No audio captured, skipping transcription");
        if config.global.sound_enabled {
            crate::audio_feedback::play_error_beep();
        }
        return;
    }

    // Persist the complete stopped recording before touching the network. If
    // Gemini times out, returns 503, the process is interrupted during the API
    // call, or the user retries later, the spoken audio remains recoverable.
    let recovery_id = match recovery::persist(
        &all_samples,
        &profile.name,
        &profile.model,
        &config.global.language,
    ) {
        Ok(item) => {
            debug!(id = %item.id, "Recovery copy persisted");
            Some(item.id)
        }
        Err(error) => {
            error!(%error, "Could not persist recovery audio; continuing transcription");
            None
        }
    };

    let _ = ui_proxy.send_event(DaemonEvent::StateChanged(DaemonState::Processing {
        profile: profile.name.clone(),
    }));

    let provider = match providers::create_provider(profile, &config.keys) {
        Ok(provider) => provider,
        Err(error) => {
            preserve_failure(recovery_id.as_deref(), &error.to_string());
            error!(%error, "Failed to create provider");
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
        Err(error) => {
            preserve_failure(recovery_id.as_deref(), &error.to_string());
            error!(%error, "Transcription failed; audio preserved in Recovery");
            if config.global.sound_enabled {
                crate::audio_feedback::play_error_beep();
            }
            return;
        }
    };

    if transcription.is_empty() {
        preserve_failure(recovery_id.as_deref(), "Gemini returned an empty transcription");
        warn!("Empty transcription received; audio preserved in Recovery");
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

    if final_text.trim().is_empty() {
        preserve_failure(recovery_id.as_deref(), "Transforms produced empty text");
        warn!("Transforms produced empty text; audio preserved in Recovery");
        return;
    }

    let record = crate::tracking::build_record(&profile.model, duration, &usage, &final_text);
    let log_line = crate::tracking::format_log_line(&record, &config.global.currency);
    info!("{}", log_line);

    match crate::tracking::append_record(&record) {
        Ok(()) => {
            if let Some(id) = recovery_id.as_deref() {
                if let Err(error) = recovery::remove(id) {
                    warn!(%error, id, "Transcription saved but recovery cleanup failed");
                }
            }
        }
        Err(error) => {
            preserve_failure(recovery_id.as_deref(), &format!("Tracking save failed: {error}"));
            warn!(%error, "Failed to save tracking record; recovery audio kept");
        }
    }

    let text = final_text.clone();
    let inject_result = tokio::task::spawn_blocking(move || injector::inject(&text)).await;

    match inject_result {
        Ok(Ok(())) => info!(text = %truncate(&final_text, 80), "✅ Injected"),
        Ok(Err(error)) => {
            error!(%error, "Text injection failed; transcription is available in dashboard history");
            if config.global.sound_enabled {
                crate::audio_feedback::play_error_beep();
            }
        }
        Err(error) => {
            error!(%error, "Injection task panicked; transcription is available in dashboard history");
            if config.global.sound_enabled {
                crate::audio_feedback::play_error_beep();
            }
        }
    }
}

fn preserve_failure(id: Option<&str>, message: &str) {
    if let Some(id) = id {
        match recovery::mark_failure(id, message) {
            Ok(()) => warn!(id, "Recording saved for dashboard recovery"),
            Err(error) => error!(%error, id, "Failed to update recovery metadata"),
        }
    }
}

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
        assert_eq!(truncate("🙂🙂🙂", 2), "🙂🙂…");
    }
}

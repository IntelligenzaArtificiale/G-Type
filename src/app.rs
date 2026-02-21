// app.rs — Finite State Machine orchestrating the G-Type daemon.
// States: Idle → Recording → Processing → Injecting → Idle
// All inter-thread communication via tokio::sync::mpsc channels.

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::audio;
use crate::config::Config;
use crate::injector;
use crate::input::{self, InputRx, InputSignal, InputTx};
use crate::network;

/// FSM states for the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum State {
    /// Waiting for CTRL+T. Minimal resource usage.
    Idle,
    /// Microphone active, streaming audio to Gemini.
    Recording,
    /// Audio stopped, waiting for final transcription from API.
    Processing,
    /// Injecting transcribed text into the focused application.
    Injecting,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Idle => write!(f, "IDLE"),
            State::Recording => write!(f, "RECORDING"),
            State::Processing => write!(f, "PROCESSING"),
            State::Injecting => write!(f, "INJECTING"),
        }
    }
}

/// Run the main event loop.
///
/// This function owns the FSM and coordinates all subsystems:
/// - Input listener (keyboard hooks)
/// - Audio capture
/// - Network (WebSocket to Gemini)
/// - Text injection
pub async fn run(config: Config) -> Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));

    // Parse the configured hotkey
    let hotkey = input::parse_hotkey(&config.hotkey)
        .context("Invalid hotkey in config")?;
    let hotkey_label = hotkey.label.clone();

    // Channel for keyboard input signals (Start/Stop)
    let (input_tx, mut input_rx): (InputTx, InputRx) = mpsc::channel(32);

    // Spawn the global keyboard listener on a dedicated OS thread
    let shutdown_clone = shutdown.clone();
    let _input_handle = crate::input::spawn_listener(input_tx, shutdown_clone, hotkey)
        .context("Failed to spawn keyboard listener")?;

    info!(hotkey = %hotkey_label, "Ready — hold hotkey to dictate.");

    let mut state = State::Idle;

    loop {
        match state {
            State::Idle => {
                state = state_idle(&mut input_rx, &hotkey_label).await;
            }
            State::Recording => {
                state = state_recording(&config, &mut input_rx, &hotkey_label).await;
            }
            State::Processing => {
                // Processing is handled inline within state_recording
                // This state exists for completeness but transitions happen
                // within the recording flow
                unreachable!("Processing state handled within recording flow");
            }
            State::Injecting => {
                // Injecting is also handled inline
                unreachable!("Injecting state handled within recording flow");
            }
        }
    }
}

/// Idle state: block until we receive a Start signal.
async fn state_idle(input_rx: &mut InputRx, _hotkey_label: &str) -> State {
    debug!("Idle, waiting for hotkey...");

    loop {
        match input_rx.recv().await {
            Some(InputSignal::Start) => {
                info!("🎤 Recording...");
                return State::Recording;
            }
            Some(InputSignal::Stop) => {
                // Spurious stop while idle, ignore
                continue;
            }
            None => {
                error!("Input channel closed unexpectedly");
                // Channel closed — re-enter idle (will block forever, effectively shutting down)
                std::future::pending::<()>().await;
                return State::Idle;
            }
        }
    }
}

/// Recording state: capture audio to buffer, then send to Gemini REST API.
/// Handles the full lifecycle: Recording → Processing → Injecting → Idle.
async fn state_recording(config: &Config, input_rx: &mut InputRx, _hotkey_label: &str) -> State {
    debug!("Capturing audio to buffer");

    // Audio capture channel — uses std::sync::mpsc (NOT tokio) because
    // the cpal audio callback runs on a non-tokio OS thread.
    let (audio_tx, audio_rx) = audio::audio_channel();

    // Atomic flag to control audio capture thread
    let recording_flag = Arc::new(AtomicBool::new(true));

    // Start audio capture on a dedicated OS thread
    let recording_flag_clone = recording_flag.clone();
    if let Err(e) = audio::start_capture(audio_tx, recording_flag_clone) {
        error!(%e, "Failed to start audio capture");
        warn!("Returning to idle due to audio capture failure");
        return State::Idle;
    }

    // Spawn a blocking task that drains the std::sync::mpsc receiver.
    // This runs on tokio's blocking thread pool so it won't block the async runtime.
    let collector_handle = tokio::task::spawn_blocking(move || {
        let mut all_samples = Vec::<i16>::new();
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
            Some(InputSignal::Start) => {
                // Double press while recording, ignore
                continue;
            }
            None => {
                error!("Input channel closed during recording");
                recording_flag.store(false, Ordering::Relaxed);
                collector_handle.abort();
                return State::Idle;
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
            return State::Idle;
        }
    };

    let duration = all_samples.len() as f64 / 16_000.0;
    info!(duration = format!("{:.1}s", duration), "⏹ Stopped. Transcribing...");

    if all_samples.is_empty() {
        warn!("No audio captured, skipping transcription");
        return State::Idle;
    }

    let transcription = match network::transcribe(config, &all_samples).await {
        Ok(text) => text,
        Err(e) => {
            error!(%e, "Transcription failed");
            warn!("Returning to idle due to transcription failure");
            return State::Idle;
        }
    };

    if transcription.is_empty() {
        warn!("Empty transcription received, skipping injection");
        return State::Idle;
    }

    // Inject the transcribed text

    // Run injection on a blocking thread to avoid blocking the async runtime
    let threshold = config.injection_threshold;
    let text = transcription.clone();
    let inject_result = tokio::task::spawn_blocking(move || {
        injector::inject(&text, threshold)
    })
    .await;

    match inject_result {
        Ok(Ok(())) => {
            info!(text = %truncate(&transcription, 80), "✅ Injected");
        }
        Ok(Err(e)) => {
            error!(%e, "Text injection failed");
        }
        Err(e) => {
            error!(%e, "Injection task panicked");
        }
    }

    State::Idle
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

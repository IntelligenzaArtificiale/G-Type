// app.rs — Finite State Machine orchestrating the G-Type daemon.
// States: Idle → Recording → Processing → Injecting → Idle
// All inter-thread communication via tokio::sync::mpsc channels.

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::audio;
use crate::config::Config;
use crate::injector;
use crate::input::{InputRx, InputSignal, InputTx};
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

    // Channel for keyboard input signals (Start/Stop)
    let (input_tx, mut input_rx): (InputTx, InputRx) = mpsc::channel(32);

    // Spawn the global keyboard listener on a dedicated OS thread
    let shutdown_clone = shutdown.clone();
    let _input_handle = crate::input::spawn_listener(input_tx, shutdown_clone)
        .context("Failed to spawn keyboard listener")?;

    info!("G-Type daemon running. Press CTRL+T to start dictation.");

    let mut state = State::Idle;

    loop {
        match state {
            State::Idle => {
                state = state_idle(&mut input_rx).await;
            }
            State::Recording => {
                state = state_recording(&config, &mut input_rx).await;
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
async fn state_idle(input_rx: &mut InputRx) -> State {
    info!("State: IDLE — waiting for CTRL+T...");

    loop {
        match input_rx.recv().await {
            Some(InputSignal::Start) => {
                info!("State: IDLE → RECORDING");
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

/// Recording state: start audio capture + WebSocket, stream until Stop signal.
/// Handles the full lifecycle: Recording → Processing → Injecting → Idle.
async fn state_recording(config: &Config, input_rx: &mut InputRx) -> State {
    info!("State: RECORDING — capturing audio and streaming to Gemini");

    // Audio capture channel
    let (audio_tx, audio_rx) = mpsc::channel::<audio::AudioChunk>(64);

    // Stop signal channel for the network task
    let (stop_tx, stop_rx) = mpsc::channel::<()>(1);

    // Atomic flag to control audio capture thread
    let recording_flag = Arc::new(AtomicBool::new(true));

    // Start audio capture on a dedicated OS thread
    let recording_flag_clone = recording_flag.clone();
    if let Err(e) = audio::start_capture(audio_tx, recording_flag_clone) {
        error!(%e, "Failed to start audio capture");
        warn!("Returning to idle due to audio capture failure");
        return State::Idle;
    }

    // Spawn the network transcription task
    let config_clone = config.clone();
    let transcribe_handle = tokio::spawn(async move {
        network::transcribe(&config_clone, audio_rx, stop_rx).await
    });

    // Wait for Stop signal from keyboard
    loop {
        match input_rx.recv().await {
            Some(InputSignal::Stop) => {
                info!("State: RECORDING → PROCESSING");
                break;
            }
            Some(InputSignal::Start) => {
                // Double press while recording, ignore
                continue;
            }
            None => {
                error!("Input channel closed during recording");
                recording_flag.store(false, Ordering::Relaxed);
                return State::Idle;
            }
        }
    }

    // Stop audio capture
    recording_flag.store(false, Ordering::Relaxed);

    // Signal the network task that recording is done
    if let Err(e) = stop_tx.send(()).await {
        warn!(%e, "Failed to send stop signal to network task");
    }

    // Wait for transcription result
    info!("State: PROCESSING — waiting for transcription...");
    let transcription = match transcribe_handle.await {
        Ok(Ok(text)) => text,
        Ok(Err(e)) => {
            error!(%e, "Transcription failed");
            warn!("Returning to idle due to transcription failure");
            return State::Idle;
        }
        Err(e) => {
            error!(%e, "Transcription task panicked");
            return State::Idle;
        }
    };

    if transcription.is_empty() {
        warn!("Empty transcription received, skipping injection");
        return State::Idle;
    }

    // Inject the transcribed text
    info!(
        text_len = transcription.len(),
        "State: PROCESSING → INJECTING"
    );

    // Run injection on a blocking thread to avoid blocking the async runtime
    let threshold = config.injection_threshold;
    let text = transcription.clone();
    let inject_result = tokio::task::spawn_blocking(move || {
        injector::inject(&text, threshold)
    })
    .await;

    match inject_result {
        Ok(Ok(())) => {
            info!(
                text_preview = %truncate(&transcription, 60),
                "Text injected successfully"
            );
        }
        Ok(Err(e)) => {
            error!(%e, "Text injection failed");
        }
        Err(e) => {
            error!(%e, "Injection task panicked");
        }
    }

    info!("State: INJECTING → IDLE");
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

// input.rs — Global keyboard hook using rdev.
// Runs on a dedicated OS thread (rdev::listen is blocking).
// Detects CTRL+T press/release and sends signals via tokio mpsc.

use anyhow::{Context, Result};
use rdev::{Event, EventType, Key};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Signals sent from the input thread to the main event loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSignal {
    /// CTRL+T pressed — start recording.
    Start,
    /// CTRL+T released — stop recording.
    Stop,
}

/// Sender type for input signals.
pub type InputTx = mpsc::Sender<InputSignal>;
/// Receiver type for input signals.
pub type InputRx = mpsc::Receiver<InputSignal>;

/// Minimum time between Start signals to prevent bouncing (ms).
const DEBOUNCE_MS: u64 = 200;

/// State tracked inside the keyboard hook callback.
struct HookState {
    ctrl_held: bool,
    t_held: bool,
    recording: bool,
    last_trigger: Instant,
    tx: InputTx,
}

impl HookState {
    fn new(tx: InputTx) -> Self {
        Self {
            ctrl_held: false,
            t_held: false,
            recording: false,
            last_trigger: Instant::now() - std::time::Duration::from_secs(10),
            tx,
        }
    }

    fn handle_event(&mut self, event: &Event) {
        match event.event_type {
            EventType::KeyPress(key) => {
                if is_ctrl(key) {
                    self.ctrl_held = true;
                }
                if key == Key::KeyT {
                    self.t_held = true;
                }
                self.check_combo();
            }
            EventType::KeyRelease(key) => {
                if is_ctrl(key) {
                    self.ctrl_held = false;
                }
                if key == Key::KeyT {
                    self.t_held = false;
                }
                self.check_release();
            }
            _ => {}
        }
    }

    fn check_combo(&mut self) {
        if self.ctrl_held && self.t_held && !self.recording {
            let now = Instant::now();
            if now.duration_since(self.last_trigger).as_millis() < DEBOUNCE_MS as u128 {
                debug!("CTRL+T debounced");
                return;
            }
            self.last_trigger = now;
            self.recording = true;
            info!("CTRL+T pressed — START recording");
            if self.tx.blocking_send(InputSignal::Start).is_err() {
                error!("Input channel closed, cannot send Start signal");
            }
        }
    }

    fn check_release(&mut self) {
        // Stop recording when either CTRL or T is released while recording
        if self.recording && (!self.ctrl_held || !self.t_held) {
            self.recording = false;
            info!("CTRL+T released — STOP recording");
            if self.tx.blocking_send(InputSignal::Stop).is_err() {
                error!("Input channel closed, cannot send Stop signal");
            }
        }
    }
}

/// Check if a key is any variant of Control.
fn is_ctrl(key: Key) -> bool {
    matches!(key, Key::ControlLeft | Key::ControlRight)
}

/// Spawn a dedicated OS thread that listens for global keyboard events.
///
/// This function returns immediately. The thread runs until `shutdown` is set to true
/// or the process exits.
///
/// `tx` — channel for sending Start/Stop signals to the async event loop.
pub fn spawn_listener(tx: InputTx, shutdown: Arc<AtomicBool>) -> Result<std::thread::JoinHandle<()>> {
    let handle = std::thread::Builder::new()
        .name("g-type-input".into())
        .spawn(move || {
            info!("Global keyboard listener started (CTRL+T to toggle recording)");
            let state = Arc::new(std::sync::Mutex::new(HookState::new(tx)));

            let callback = move |event: Event| {
                if shutdown.load(Ordering::Relaxed) {
                    // Cannot cleanly stop rdev::listen, but we stop processing
                    return;
                }
                if let Ok(mut s) = state.lock() {
                    s.handle_event(&event);
                }
            };

            if let Err(e) = rdev::listen(callback) {
                error!(?e, "Global keyboard listener crashed");
            }
        })
        .context("Failed to spawn input listener thread")?;

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ctrl() {
        assert!(is_ctrl(Key::ControlLeft));
        assert!(is_ctrl(Key::ControlRight));
        assert!(!is_ctrl(Key::KeyT));
        assert!(!is_ctrl(Key::ShiftLeft));
    }

    #[test]
    fn test_hook_state_combo() {
        // Run inside a standalone thread to avoid tokio runtime blocking conflict.
        let handle = std::thread::spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let (tx, mut rx) = mpsc::channel(16);
            let mut state = HookState::new(tx);

            // Press Ctrl
            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyPress(Key::ControlLeft),
            });
            assert!(state.ctrl_held);
            assert!(!state.recording);

            // Press T
            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyPress(Key::KeyT),
            });
            assert!(state.recording);

            let signal = rt.block_on(async { rx.recv().await });
            assert_eq!(signal, Some(InputSignal::Start));

            // Release T
            state.handle_event(&Event {
                time: std::time::SystemTime::now(),
                name: None,
                event_type: EventType::KeyRelease(Key::KeyT),
            });
            assert!(!state.recording);

            let signal = rt.block_on(async { rx.recv().await });
            assert_eq!(signal, Some(InputSignal::Stop));
        });

        handle.join().expect("Test thread panicked");
    }
}

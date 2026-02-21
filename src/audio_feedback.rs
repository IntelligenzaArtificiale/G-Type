use rodio::{source::SineWave, OutputStream, Sink, Source};
use std::sync::mpsc;
use std::time::Duration;
use tracing::warn;

/// Commands sent to the persistent audio playback thread.
enum BeepCmd {
    /// Single tone: frequency (Hz), duration, amplitude (0.0–1.0).
    Tone(f32, Duration, f32),
    /// Two consecutive tones (for error beep).
    DoubleTone(f32, Duration, f32, f32, Duration, f32),
}

/// Lazily-initialized channel sender to the persistent beep thread.
/// Using a persistent thread avoids the race condition where
/// `OutputStream::try_default()` fails when the OS audio device is
/// already held by the cpal capture thread.
fn beep_sender() -> &'static mpsc::Sender<BeepCmd> {
    use std::sync::OnceLock;
    static TX: OnceLock<mpsc::Sender<BeepCmd>> = OnceLock::new();

    TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<BeepCmd>();

        std::thread::Builder::new()
            .name("audio-feedback".into())
            .spawn(move || {
                // Open the output stream ONCE and keep it alive for the
                // entire process lifetime.  This is the key fix: by
                // holding `_stream` we never contend with cpal for the
                // audio device.
                let (_stream, stream_handle) = match OutputStream::try_default() {
                    Ok(pair) => pair,
                    Err(e) => {
                        warn!("Audio feedback unavailable: failed to open output stream: {e}");
                        return;
                    }
                };

                while let Ok(cmd) = rx.recv() {
                    let sink = match Sink::try_new(&stream_handle) {
                        Ok(s) => s,
                        Err(e) => {
                            warn!("Audio feedback: failed to create sink: {e}");
                            continue;
                        }
                    };

                    match cmd {
                        BeepCmd::Tone(freq, dur, amp) => {
                            sink.append(SineWave::new(freq).take_duration(dur).amplify(amp));
                        }
                        BeepCmd::DoubleTone(f1, d1, a1, f2, d2, a2) => {
                            sink.append(SineWave::new(f1).take_duration(d1).amplify(a1));
                            sink.append(SineWave::new(f2).take_duration(d2).amplify(a2));
                        }
                    }

                    sink.sleep_until_end();
                }
            })
            .expect("failed to spawn audio-feedback thread");

        tx
    })
}

/// Play a "start recording" beep — rising double-chirp (distinctive).
pub fn play_start_beep() {
    let _ = beep_sender().send(BeepCmd::DoubleTone(
        880.0,
        Duration::from_millis(120),
        0.50,
        1100.0,
        Duration::from_millis(120),
        0.50,
    ));
}

/// Play a "stop recording" beep — single lower tone.
pub fn play_stop_beep() {
    let _ = beep_sender().send(BeepCmd::Tone(440.0, Duration::from_millis(200), 0.50));
}

/// Play an "error" beep — two descending low tones.
pub fn play_error_beep() {
    let _ = beep_sender().send(BeepCmd::DoubleTone(
        400.0,
        Duration::from_millis(200),
        0.50,
        300.0,
        Duration::from_millis(300),
        0.50,
    ));
}

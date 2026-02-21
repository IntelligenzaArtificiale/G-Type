use rodio::{OutputStream, Sink, source::SineWave, Source};
use std::time::Duration;
use tracing::debug;

/// Play a short "start recording" beep (high pitch).
pub fn play_start_beep() {
    play_beep(880.0, Duration::from_millis(150));
}

/// Play a short "stop recording" beep (lower pitch).
pub fn play_stop_beep() {
    play_beep(440.0, Duration::from_millis(150));
}

/// Play a short "error" beep (two low pitches).
pub fn play_error_beep() {
    std::thread::spawn(|| {
        if let Ok((_stream, stream_handle)) = OutputStream::try_default() {
            if let Ok(sink) = Sink::try_new(&stream_handle) {
                let source1 = SineWave::new(300.0)
                    .take_duration(Duration::from_millis(150))
                    .amplify(0.20);
                let source2 = SineWave::new(250.0)
                    .take_duration(Duration::from_millis(200))
                    .amplify(0.20);
                
                sink.append(source1);
                sink.append(source2);
                sink.sleep_until_end();
            }
        }
    });
}

fn play_beep(freq: f32, duration: Duration) {
    std::thread::spawn(move || {
        match OutputStream::try_default() {
            Ok((_stream, stream_handle)) => {
                match Sink::try_new(&stream_handle) {
                    Ok(sink) => {
                        let source = SineWave::new(freq)
                            .take_duration(duration)
                            .amplify(0.20); // 20% volume to not be too loud
                        sink.append(source);
                        sink.sleep_until_end();
                    }
                    Err(e) => debug!("Failed to create audio sink: {}", e),
                }
            }
            Err(e) => debug!("Failed to get default audio output stream: {}", e),
        }
    });
}

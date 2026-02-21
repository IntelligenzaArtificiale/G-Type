# Voice vs Typing: Benchmark & Research

## External research

### Stanford / University of Washington / Baidu (2016)

The most cited academic study comparing speech-to-text with keyboard input on mobile devices:

| Metric | English | Mandarin |
|--------|---------|----------|
| **Speed advantage (voice vs keyboard)** | ~3.0× faster | ~2.8× faster |
| **Error rate reduction** | −20.4% | −63.4% |

> "Speech recognition is about three times faster than typing on a mobile phone."

- Paper: *"Speech Is 3x Faster than Typing for English and Mandarin Text Entry on Mobile Devices"* (Ruan et al., 2016)
- Stanford summary: [stanford.edu](https://news.stanford.edu/stories/2016/08/stanford-study-speech-recognition-faster-texting)
- Stanford HCI lab: [hci.stanford.edu/research/speech](https://hci.stanford.edu/research/speech/)

### Important caveats

- The study measured **mobile** text entry (touchscreen keyboard), not full desktop keyboards.
- Desktop typing speed with a physical keyboard (60–100+ WPM for proficient typists) narrows the gap.
- The advantage depends on: ambient noise, vocabulary complexity, language, and ASR model quality.
- Editing and correction workflows differ between voice and keyboard.

## G-Type performance characteristics

| Stage | Typical latency | Notes |
|-------|----------------|-------|
| Hotkey detection | <10ms | rdev global hook, OS-level |
| Audio capture | Real-time | 16kHz mono, cpal, pre-allocated buffer |
| WAV encoding | <5ms | In-memory, no disk I/O |
| Network round-trip | 1–4s | Depends on audio length, model, network |
| Text injection | <50ms | enigo keystrokes, clipboard fallback for >500 chars |
| **Total end-to-end** | **~2–5s** | From key release to text appearing |

### Platform support matrix

| Platform | Audio capture | Keyboard hook | Text injection | Status |
|----------|--------------|---------------|----------------|--------|
| Linux (X11) | ✅ ALSA/PulseAudio | ✅ rdev | ✅ enigo/xdotool | **Supported** |
| Linux (Wayland) | ✅ PulseAudio | ⚠️ Varies by compositor | ⚠️ Limited | **Partial** |
| macOS | ✅ CoreAudio | ✅ rdev | ✅ enigo | **Supported** |
| Windows | ✅ WASAPI | ✅ rdev | ✅ enigo | **Supported** |

## Methodology for your own benchmarks

If you want to measure G-Type's speed advantage in your workflow:

1. Pick a standard text passage (~100 words).
2. Time yourself typing it normally (WPM = words / minutes).
3. Time yourself dictating it with G-Type (include the transcription wait time).
4. Compare: `speedup = typing_time / dictation_time`.

Typical results for casual text (emails, notes, chat):
- Hunt-and-peck typists (20–30 WPM): **4–6× speedup**
- Touch typists (60–80 WPM): **1.5–2.5× speedup**
- Fast typists (100+ WPM): **0.8–1.5× speedup** (voice may be slower for highly technical text)

## References

1. Ruan, S., Wobbrock, J. O., Liou, K., Ng, A., & Landay, J. A. (2016). *Speech Is 3x Faster than Typing for English and Mandarin Text Entry on Mobile Devices.* Stanford University.
2. GitHub Octoverse 2024. *Developer demographics and tool usage.* [github.blog](https://github.blog/news-insights/octoverse/octoverse-2024/)

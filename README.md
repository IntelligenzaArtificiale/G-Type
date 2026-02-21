# G-Type

**Global voice dictation daemon.** Hold a hotkey anywhere on your system, speak, release — your words appear as typed text.

Speaking is **3–5× faster** than typing. An average person types 40 WPM but speaks at 150 WPM. G-Type removes the friction: one hotkey, zero UI, works in every app.

Powered by Google Gemini REST API. Single static binary. ~5 MB.

---

## How it works

```
┌─────────┐    Hotkey     ┌───────────┐    PCM 16kHz    ┌───────────┐
│ Keyboard │──────────────▶│   Audio   │────────────────▶│ REST API  │
│  Hook    │   (rdev)      │  Capture  │   (buffered)    │  Gemini   │
└─────────┘               └───────────┘                 └─────┬─────┘
                                                              │
                                                         text │
                                                              ▼
                          ┌───────────┐    keystrokes    ┌───────────┐
                          │  Focused  │◀────────────────│ Injector  │
                          │   App     │   or clipboard   │           │
                          └───────────┘                 └───────────┘
```

1. **Idle:** Daemon waits for your hotkey. Minimal resource usage.
2. **Recording:** Microphone captures audio → converts to 16kHz mono PCM → buffers in memory.
3. **Processing:** On key release, audio is encoded as WAV, sent to Gemini REST API, transcription returned.
4. **Injection:** Text is typed via keystroke emulation. Falls back to clipboard paste for text >500 chars.

## Install

### One-click install (Linux & macOS)

```bash
curl -sSf https://raw.githubusercontent.com/IntelligenzaArtificiale/g-type/main/install.sh | bash
```

### One-click install (Windows)

Open PowerShell and run:

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/g-type/main/install.ps1 | iex
```

Both installers will automatically:
- Detect your OS and architecture
- Install required system dependencies (Linux)
- Download the latest pre-built binary
- Add it to your PATH
- Run the interactive setup wizard on first run

### Pre-built binaries

Download from [Releases](https://github.com/IntelligenzaArtificiale/g-type/releases).

### From source (all platforms)

```bash
# Prerequisites: Rust toolchain + system audio/input libraries
# Linux: sudo apt install libasound2-dev libx11-dev libxtst-dev libxdo-dev libevdev-dev
cargo install --path .
```

## First run

On first launch, G-Type runs an interactive setup wizard:

```
╔══════════════════════════════════════════════╗
║         G-Type — First Time Setup            ║
╚══════════════════════════════════════════════╝

  G-Type needs a Google Gemini API key to work.
  Get one free at: https://aistudio.google.com/apikey

? 🔑 Gemini API Key: ****************************************
⠋ Verifying API key...
✔ API key is valid!

? 🤖 Select Gemini Model:
  > models/gemini-2.0-flash
    models/gemini-2.0-flash-lite
    models/gemini-2.5-flash
    models/gemini-2.5-pro
    models/gemini-1.5-pro
    models/gemini-1.5-flash

⌨️ Press your desired hotkey combo (e.g. hold Ctrl+Shift+Space)...
  Captured hotkey: ctrl+shift+space

  ✔ Config saved to ~/.config/g-type/config.toml
```

Re-run anytime with `g-type setup`.

## Usage

```bash
g-type                # Start daemon (auto-setup on first run)
g-type setup          # Re-run setup wizard
g-type set-key KEY    # Update API key
g-type config         # Show config file path
g-type test-audio     # Test microphone (3 seconds)
g-type list-devices   # List audio input devices
RUST_LOG=g_type=debug g-type  # Verbose logging
```

Then in **any** application:
1. Hold your hotkey (default: `CTRL+SHIFT+SPACE`) and speak
2. Release the hotkey
3. Text appears at cursor position

## Configuration

Config file locations:

| OS      | Path                                           |
|---------|------------------------------------------------|
| Linux   | `~/.config/g-type/config.toml`                 |
| macOS   | `~/Library/Application Support/g-type/config.toml` |
| Windows | `%APPDATA%\g-type\config.toml`                 |

| Key            | Default                   | Description                    |
|----------------|---------------------------|--------------------------------|
| `api_key`      | —                         | Google Gemini API key (required)|
| `model`        | `models/gemini-2.0-flash` | Gemini model identifier        |
| `hotkey`       | `ctrl+shift+space`        | Trigger key combination        |
| `timeout_secs` | `10`                      | HTTP request timeout (seconds) |

## Architecture

```
src/
├── main.rs           CLI entry point, subcommands
├── app.rs            FSM: Idle → Recording → Processing → Injecting
├── audio.rs          cpal capture, real-time downsample to 16kHz mono
├── audio_feedback.rs rodio start/stop/error beeps
├── network.rs        REST client, reqwest-retry, WAV encoding
├── input.rs          rdev global keyboard hook
├── injector.rs       enigo keystrokes, arboard clipboard fallback
└── config.rs         TOML config, dialoguer setup wizard
```

Key design choices:
- **API key via header:** Sent as `x-goog-api-key`, never in URL or logs.
- **API key verified at setup:** A test call to Gemini validates your key before saving.
- **Auto-retry:** Exponential backoff on transient HTTP errors (429, 503).
- **Error injection:** API errors are typed into the focused field so the user sees them.
- **Audio feedback:** Beeps on record start, stop, and error (via `rodio`).
- **Pre-allocated buffers:** Audio buffer pre-sized for ~10s to avoid reallocations.
- **Interactive hotkey capture:** Press your desired combo during setup — no manual typing.
- **Graceful shutdown:** Catches SIGINT/SIGTERM for clean exit.

## Building

```bash
cargo build            # Debug
cargo build --release  # Optimized + stripped (~5 MB)
cargo test             # Unit tests (35+ tests)
```

## Requirements

- Google Gemini API key ([get one free](https://aistudio.google.com/apikey))
- Working microphone
- **Linux:** ALSA, X11, XTest libs (`libasound2-dev libx11-dev libxtst-dev libxdo-dev libevdev-dev`)
- **macOS:** Accessibility permissions for keyboard injection
- **Windows:** No additional requirements

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Security

See [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)

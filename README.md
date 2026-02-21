# G-Type

**Global voice dictation daemon.** Hold your hotkey anywhere on your system, speak, release — your words appear as typed text.

Powered by Google Gemini REST API. Single static binary.

---

## Enterprise Features

- **Robust Network Handling:** Automatic exponential backoff for 429 Rate Limit errors.
- **Smart Error Injection:** API errors are injected directly into your text field so you never miss a failure.
- **Cross-Platform Audio Feedback:** Subtle beeps indicate when recording starts, stops, or fails.
- **Optimized Memory:** Pre-allocated audio buffers prevent memory fragmentation during long dictations.
- **Interactive Setup Wizard:** Beautiful CLI wizard with model selection and API key validation.
- **Smart Text Injection:** Uses keystroke emulation for natural typing, automatically falling back to clipboard pasting for text over 500 characters or if keystrokes fail.

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

1. **Idle:** Daemon sleeps at <15MB RAM. Global keyboard hook waits for your hotkey.
2. **Recording:** Microphone captures audio → converts to 16kHz mono PCM → buffers in memory.
3. **Processing:** On key release, sends audio to Gemini REST API → waits for transcription.
4. **Injection:** Emulates keystrokes to type the text. Falls back to clipboard for long text (>500 chars).

## Install

### Linux / macOS

```bash
curl -sSf https://raw.githubusercontent.com/IntelligenzaArtificiale/g-type/main/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/g-type/main/install.ps1 | iex
```

### From source

```bash
# Prerequisites: Rust toolchain, system audio/input libraries
# Linux: sudo apt install libasound2-dev libx11-dev libxtst-dev libxdo-dev libevdev-dev
cargo install --path .
```

## Configuration

On first run, G-Type launches an **interactive setup wizard** — no manual file editing needed:

```
╔══════════════════════════════════════════════╗
║         G-Type — First Time Setup            ║
╚══════════════════════════════════════════════╝

  G-Type needs a Google Gemini API key to work.
  Get one free at: https://aistudio.google.com/apikey

? Gemini API Key: ***************************************
? Select Gemini Model: models/gemini-2.0-flash
? Hotkey: ctrl+shift+space

  ✔ Config saved to ~/.config/g-type/config.toml
  ✔ You can re-run setup anytime with: g-type setup
```

Config file locations:

| OS      | Path                                    |
|---------|-----------------------------------------|
| Linux   | `~/.config/g-type/config.toml`          |
| macOS   | `~/Library/Application Support/g-type/config.toml` |
| Windows | `%APPDATA%\g-type\config.toml`          |

### Options

| Key                    | Default                    | Description                              |
|------------------------|----------------------------|------------------------------------------|
| `api_key`              | —                          | Google Gemini API key (required)         |
| `model`                | `models/gemini-2.0-flash`  | Gemini model to use                      |
| `hotkey`               | `ctrl+shift+space`         | Trigger key combination                  |
| `timeout_secs`         | `10`                       | Max wait time for transcription (secs)   |

## Usage

```bash
# Start the daemon (auto-setup on first run)
g-type

# Re-run setup wizard
g-type setup

# Update just the API key
g-type set-key YOUR_NEW_KEY

# Show config file path
g-type config

# Test audio capture
g-type test-audio

# List audio devices
g-type list-devices

# With debug logging
RUST_LOG=g_type=debug g-type
```

Then in **any** application:
1. Hold your hotkey (default: `CTRL+SHIFT+SPACE`) and speak
2. Release the hotkey
3. Text appears at cursor position

## Architecture

```
src/
├── main.rs           Entry point, logger setup, CLI commands
├── app.rs            FSM: Idle → Recording → Processing → Injecting
├── audio.rs          cpal microphone capture, PCM conversion
├── audio_feedback.rs rodio cross-platform audio cues
├── network.rs        REST client with reqwest-retry for Gemini API
├── input.rs          rdev global keyboard hook
├── injector.rs       enigo keystrokes / arboard clipboard paste
└── config.rs         TOML config with dialoguer interactive setup
```

**Design principles:**
- **Crash-only:** Module failures are isolated, logged, and recovered. Zero `unwrap()`.
- **Memory Optimized:** Audio buffers are pre-allocated to prevent fragmentation.
- **Lock-free channels:** All inter-thread communication via `tokio::sync::mpsc`.
- **Small files:** Every file < 400 lines, single responsibility.

## Building

```bash
# Debug build
cargo build

# Release build (optimized, stripped)
cargo build --release

# Run tests
cargo test
```

## Requirements

- Google Gemini API key
- Working microphone
- **Linux:** ALSA, X11, XTest libraries
- **macOS:** Accessibility permissions for keyboard injection
- **Windows:** No additional requirements

## License

MIT

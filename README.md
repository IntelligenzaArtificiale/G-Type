# G-Type

**Global voice dictation daemon.** Hold `CTRL+T` anywhere on your system, speak, release — your words appear as typed text.

Powered by Google Gemini 2.0 Flash Live API. Zero-latency WebSocket streaming. Single static binary.

---

## How it works

```
┌─────────┐    CTRL+T     ┌───────────┐    PCM 16kHz    ┌───────────┐
│ Keyboard │──────────────▶│   Audio   │────────────────▶│ WebSocket │
│  Hook    │   (rdev)      │  Capture  │   (streaming)   │  Gemini   │
└─────────┘               └───────────┘                 └─────┬─────┘
                                                              │
                                                         text │
                                                              ▼
                          ┌───────────┐    keystrokes    ┌───────────┐
                          │  Focused  │◀────────────────│ Injector  │
                          │   App     │   or clipboard   │           │
                          └───────────┘                 └───────────┘
```

1. **Idle:** Daemon sleeps at <15MB RAM. Global keyboard hook waits for `CTRL+T`.
2. **Recording:** Microphone captures audio → converts to 16kHz mono PCM → streams via WebSocket to Gemini.
3. **Processing:** On key release, sends `turnComplete` → waits for transcription.
4. **Injection:** Short text (<80 chars) → keystroke emulation. Long text → clipboard paste.

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

On first run, G-Type creates a config file at:

| OS      | Path                                    |
|---------|-----------------------------------------|
| Linux   | `~/.config/g-type/config.toml`          |
| macOS   | `~/Library/Application Support/g-type/config.toml` |
| Windows | `%APPDATA%\g-type\config.toml`          |

```toml
api_key = "YOUR_GEMINI_API_KEY_HERE"
model = "models/gemini-2.0-flash"
hotkey = "ctrl+t"
injection_threshold = 80
timeout_secs = 3
```

Get your API key at [aistudio.google.com/apikey](https://aistudio.google.com/apikey).

### Options

| Key                    | Default                    | Description                              |
|------------------------|----------------------------|------------------------------------------|
| `api_key`              | —                          | Google Gemini API key (required)         |
| `model`                | `models/gemini-2.0-flash`  | Gemini model to use                      |
| `hotkey`               | `ctrl+t`                   | Trigger key combination                  |
| `injection_threshold`  | `80`                       | Char count threshold for clipboard mode  |
| `timeout_secs`         | `3`                        | Max wait time for transcription (secs)   |

## Usage

```bash
# Start the daemon
g-type

# With debug logging
RUST_LOG=g_type=debug g-type
```

Then in **any** application:
1. Hold `CTRL+T` and speak
2. Release `CTRL+T`
3. Text appears at cursor position

## Architecture

```
src/
├── main.rs       Entry point, logger setup, config load
├── app.rs        FSM: Idle → Recording → Processing → Injecting
├── audio.rs      cpal microphone capture, PCM conversion
├── network.rs    WebSocket client for Gemini Live API
├── input.rs      rdev global keyboard hook (CTRL+T)
├── injector.rs   enigo keystrokes / arboard clipboard paste
└── config.rs     TOML config with XDG directory resolution
```

**Design principles:**
- **Crash-only:** Module failures are isolated, logged, and recovered. Zero `unwrap()`.
- **Zero-copy streaming:** Audio flows mic → base64 → WebSocket with minimal allocations.
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

- Google Gemini API key with access to `gemini-2.0-flash`
- Working microphone
- **Linux:** ALSA, X11, XTest libraries
- **macOS:** Accessibility permissions for keyboard injection
- **Windows:** No additional requirements

## License

MIT

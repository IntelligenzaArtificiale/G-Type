# G-Type

**Global push-to-talk voice dictation powered by Google Gemini.**

G-Type runs locally in the background, records only while you hold a configured hotkey, sends the audio to Gemini for transcription, and inserts the result into the active application.

[Italiano](README.it.md)

## Highlights

- Global push-to-talk dictation.
- Multiple profiles with hotkey, Gemini model, timeout and optional prompt.
- Local dashboard at `http://127.0.0.1:9741/`.
- Local history, search, statistics and cost tracking.
- Failed-audio recovery with local WAV preservation.
- Automatic fallback to stable Flash-Lite models for transient failures.
- Ready-to-use templates for common writing and work flows.
- Web onboarding with Gemini API key verification before saving.
- Background release checks and rollback-safe self-update.
- Optional startup at login controlled from the dashboard.

G-Type does not require a G-Type account, hosted backend, cloud database or browser extension.

## Prebuilt platforms

| Platform | Architecture |
|---|---:|
| Linux | x86_64 |
| Windows | x86_64 |
| macOS | Intel x86_64 |
| macOS | Apple Silicon arm64 |

## Install

Linux and macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.sh | bash
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.ps1 | iex
```

The installers download the latest compatible GitHub Release, install G-Type for the current user and start it.

**Startup at login is not enabled automatically.** Enable it explicitly from **Dashboard → Settings → System → Start with computer** if you want it. The setting is per-user and does not require administrator privileges.

## First run

On first launch G-Type opens:

```text
http://127.0.0.1:9741/setup
```

Setup has three steps: verify the Gemini API key, choose a compatible model, and select the global hotkey. The default hotkey is `Ctrl+Shift+Space`.

## Daily use

Start G-Type:

```bash
g-type
```

Hold the configured hotkey, speak, then release it. G-Type records only while the hotkey is held, transcribes the audio and inserts the text into the active application.

Dashboard:

```text
http://127.0.0.1:9741/
```

## Dashboard

**History** provides recent transcriptions, search, quick copy, duration, word count, model and cost information.

**Statistics** shows total usage, words, audio time, estimated typing time saved, speaking speed, costs and recent activity.

**Settings** manages language, currency, microphone, feedback sounds, tray icon, startup at login, Gemini API key, profiles, hotkeys, models, timeouts, prompts and update status.

**Recovery** keeps failed recordings available locally so the same audio can be retried, opened or deleted.

## Updates

G-Type checks for new releases in the background without blocking dictation.

```bash
g-type upgrade
g-type version
```

The updater validates the download, keeps a temporary backup and restores the previous binary if replacement fails.

## Useful commands

```text
g-type                 Start the daemon
g-type setup           Open web setup
g-type stats           Show usage and cost statistics
g-type upgrade         Update to the latest release
g-type version         Show installed version
g-type config          Show the configuration path
g-type set-key <KEY>   Replace the Gemini API key
g-type test-audio      Run a microphone test
g-type list-devices    List input devices
g-type help            Show CLI help
```

## Data and privacy

The dashboard binds only to `127.0.0.1`. Configuration, history and recovery data remain in the current user's local directories. The Gemini API key is not returned in clear text by the dashboard API. Audio needed for transcription is sent to the configured Gemini API.

## Build from source

```bash
git clone https://github.com/IntelligenzaArtificiale/G-Type.git
cd G-Type
cargo build --release
```

Before contributing:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Official releases are built by GitHub Actions for all supported targets.

## License

MIT. See [LICENSE](LICENSE).

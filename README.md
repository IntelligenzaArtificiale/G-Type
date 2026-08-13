# G-Type

**Global push-to-talk voice dictation powered by Google Gemini.**

G-Type runs locally in the background, listens only while you hold a configured hotkey, sends the captured audio to Gemini for transcription, and inserts the resulting text into the application you are currently using.

It is designed around one idea: **install once, configure in the browser, then dictate anywhere with almost no friction.**

[Italiano](README.it.md)

## What G-Type includes

- Global push-to-talk dictation on supported desktop platforms.
- Multiple profiles, each with its own hotkey, Gemini model, timeout and optional prompt.
- A local dashboard at `http://127.0.0.1:9741/`.
- Local transcription history, search, usage statistics and cost tracking.
- USD / EUR display for current and historical costs.
- Recovery of failed transcriptions: the WAV is kept locally when a request fails, then it can be retried with another model or deleted.
- Automatic fallback to inexpensive stable models for transient Gemini failures.
- Built-in profile templates for email, cleaned-up dictation, meeting notes, brainstorming, task lists, bug reports and other common work flows.
- Automatic read-only check for newer G-Type releases.
- Self-update with rollback protection through `g-type upgrade`.
- Web-based first-run onboarding with Gemini API key verification before it is saved.
- Crash-safe configuration writes with local backup recovery.

G-Type does **not** require an account, a cloud database, a hosted G-Type backend or a browser extension.

## Supported prebuilt platforms

| Platform | Architecture | Prebuilt release |
|---|---:|---:|
| Linux | x86_64 | Yes |
| Windows | x86_64 | Yes |
| macOS | Intel x86_64 | Yes |
| macOS | Apple Silicon arm64 | Yes |

Other targets can be built from source, but the one-command installers below are intended for the prebuilt platforms above.

## Install in one command

### Linux and macOS

```bash
curl -fsSL https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.sh | bash
```

The installer downloads the latest compatible GitHub Release, places `g-type` in your user environment and starts it. On the first run G-Type opens the onboarding page in your default browser.

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.ps1 | iex
```

Run the command in a normal PowerShell window. Administrator privileges are not required for the standard per-user installation.

## First-run onboarding

There is no API-key questionnaire to complete in the terminal.

On the first start, G-Type launches its local server and opens:

```text
http://127.0.0.1:9741/setup
```

The onboarding is intentionally short:

1. **Gemini API key** — enter the key and let G-Type verify it directly against the Gemini API before saving it.
2. **Gemini model** — choose from the current audio-to-text-compatible model catalog; the recommended model is preselected.
3. **Global hotkey** — keep the default `Ctrl+Shift+Space` or record another combination.

After the final step, G-Type saves the local configuration and opens the dashboard.

You can create a Gemini API key from Google AI Studio.

## Daily use

Start G-Type:

```bash
g-type
```

Then hold your profile hotkey, speak, and release it when you are done. G-Type records only while the hotkey is held, transcribes the audio and inserts the result into the active application.

The default profile starts with:

```text
Ctrl + Shift + Space
```

Open the dashboard at:

```text
http://127.0.0.1:9741/
```

## Dashboard

The dashboard is served only on the local loopback interface.

### History

- Recent transcriptions with five records per page.
- Search by transcription text or model.
- Expand / collapse long text.
- Copy a transcription with one click.
- Duration, word count, model and cost per transcription.
- Repair display for older records whose cost can be reconstructed from stored token usage and model pricing.

### Statistics

- Total transcriptions and words.
- Audio time and estimated typing time saved.
- Speaking speed.
- Cost totals and cost per 1,000 words.
- Activity over the latest 14 days.
- Usage and cost by model.

### Settings

From the dashboard you can manage the settings used by day-to-day dictation:

- Language / automatic language detection.
- Display currency: USD or EUR.
- Input microphone.
- Audio feedback sounds.
- Tray icon.
- Gemini API key, verified before replacement.
- Profiles and profile templates.
- Hotkeys, models, timeouts and custom profile prompts.
- Current G-Type version and update status.

Most profile and global changes are applied without restarting G-Type. Settings that depend on the desktop GUI integration can require a restart; the dashboard tells you when that is the case.

### Recovery

If a Gemini request fails, G-Type keeps the recorded WAV locally instead of silently losing the dictation.

From **Recovery** you can:

- Retry the exact same audio.
- Select a different compatible Gemini model.
- Open the local WAV.
- Delete the failed recording permanently.

Successful recovery writes the transcription to normal history and removes the recovery item.

## Profiles

A profile is a reusable dictation behavior bound to a hotkey.

Each profile can define:

- Name.
- Global hotkey.
- Gemini model.
- Request timeout.
- Optional prompt that turns raw speech into a specific work output.

The dashboard includes ready-to-use templates such as:

- Clean dictation.
- Professional email.
- Quick message.
- Brainstorming to structured plan.
- Meeting notes.
- Tasks and checklist.
- Prompt for an AI assistant.
- Status update.
- Formal text.
- Bug report.

Templates are normal profiles after creation; you can edit or delete them at any time.

## Updates

G-Type performs a lightweight, read-only release check in the background when its local dashboard starts. A failed update check never prevents dictation from starting.

The dashboard also shows when a newer release is available.

Update G-Type on every supported platform with the same command:

```bash
g-type upgrade
```

Then restart the running daemon to use the new binary.

Check the installed version:

```bash
g-type version
```

The updater downloads the release asset next to the current executable, validates the download before replacement, keeps a temporary backup and restores the previous executable if installation of the new binary fails.

## Useful commands

```text
g-type                 Start the dictation daemon
g-type setup           Start G-Type if needed and open web setup
g-type stats           Show usage and cost statistics in the terminal
g-type upgrade         Update to the latest compatible GitHub Release
g-type version         Show the installed version
g-type config          Show the exact configuration file path
g-type set-key <KEY>   Replace the Gemini API key from the terminal
g-type test-audio      Capture a short microphone test
g-type list-devices    List available input devices
g-type help            Show CLI help
```

For normal configuration, the dashboard is preferred over editing files manually.

## Data and privacy

G-Type is local-first:

- The dashboard binds to `127.0.0.1` rather than a public network interface.
- Configuration and usage history are stored in the current user's local application directories.
- The Gemini API key is stored in the local G-Type configuration and is not returned by the dashboard API; the UI receives only a masked representation.
- Audio used for a transcription is sent to the configured Gemini API.
- A failed transcription can leave a local WAV in the recovery spool so that it can be retried instead of lost.
- G-Type does not require its own hosted account or cloud database.

Use:

```bash
g-type config
```

to print the exact configuration path on the current operating system.

## Reliability design

The desktop daemon is intentionally small, but the critical paths are defensive:

- Configuration writes use a temporary file plus backup and recovery.
- A corrupt primary configuration can be restored from the last valid backup.
- Gemini failures are classified so temporary overload / network problems are treated differently from authentication or invalid-request errors.
- Temporary API failures can fall back to other stable Flash-Lite models instead of repeatedly hammering the same failing endpoint.
- Failed recordings are preserved locally for manual recovery.
- Self-update uses a temporary asset and rollback rather than blindly overwriting the running executable.
- Update discovery is best-effort and never part of the critical recording path.
- The local dashboard remains independent from any hosted G-Type service.

## Troubleshooting

### The dashboard does not open

Make sure G-Type is running:

```bash
g-type
```

Then open:

```text
http://127.0.0.1:9741/
```

If G-Type reports that another instance is already running, the dashboard from that running instance should already be available on port `9741`.

### Test the microphone

```bash
g-type test-audio
```

List input devices:

```bash
g-type list-devices
```

You can then select the preferred input device from Dashboard → Settings.

### API key problems

Open setup again:

```bash
g-type setup
```

or replace the key from Dashboard → Settings. G-Type verifies the new key before persisting it.

### A transcription failed

Open Dashboard → Recovery. If the WAV was preserved, you can retry it with another model without dictating the content again.

### Linux overlay

The dictation daemon, tray integration and local dashboard can run while the optional visual overlay is disabled. On Linux the overlay is intentionally conservative because mixed Wayland/XWayland/GTK environments can be unstable.

## Build from source

G-Type is written in Rust.

```bash
git clone https://github.com/IntelligenzaArtificiale/G-Type.git
cd G-Type
cargo build --release
```

Linux source builds require the native audio, X11/GTK/WebKit development libraries used by the desktop integration. See the CI workflow for the exact Ubuntu packages used by official builds.

Run checks before contributing:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Release process

Official releases are built by GitHub Actions for the supported targets. The release workflow runs quality checks before creating platform binaries and publishing the GitHub Release.

`g-type upgrade` always resolves the latest published release and chooses the asset matching the current operating system and architecture.

## License

MIT. See [LICENSE](LICENSE).

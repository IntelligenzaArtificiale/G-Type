# G-Type

**Local-first, context-aware voice input for your computer.**

G-Type runs in the background, records only when you invoke it, sends audio to your own Google Gemini API key and inserts the result into the active application. Version 1.5 adds application context, Modes, app bindings, voice snippets, Hands-Free and Voice Edit without requiring a G-Type account, hosted backend or cloud database.

[Italiano](README.it.md)

## What G-Type does

- **Push-to-talk dictation** with configurable global hotkeys.
- **Context Awareness**: captures the foreground application at recording start when the operating system exposes it, sends that context to Gemini only to improve understanding, and stores it in local history.
- **Modes**: each Mode has its own hotkey, Gemini model, timeout and optional instructions.
- **Application bindings**: an application/context already observed in history can be linked to one Mode. Bindings affect the default Mode and Hands-Free; an explicitly pressed non-default Mode hotkey always wins.
- **Voice snippets**: map a spoken trigger such as `calendar link` to an exact text, URL, email, number or signature.
- **Backtrack**: explicit spoken corrections such as “four o'clock — actually five” keep the final corrected version without otherwise rewriting normal dictation.
- **Hands-Free**: press the Hands-Free hotkey once to start and again to stop. Default: `Ctrl+Shift+H`.
- **Voice Edit**: select text, hold the Voice Edit hotkey, speak an editing instruction, then release. G-Type uses one multimodal Gemini request and replaces the selection only if the active application is still the same. Default: `Ctrl+Shift+E`.
- **Local history, statistics and cost tracking** with application, Mode and operation metadata.
- **Failed-audio Recovery**: stopped recordings are persisted locally before network processing so provider errors do not lose your speech.
- **Gemini fallback** for transient provider failures.
- **Background update checks** and rollback-safe self-update.
- **Optional startup at login** from the dashboard.

Context Awareness is deliberately best-effort. If an operating system or Wayland compositor does not expose the foreground application through a safe portable mechanism, G-Type simply continues without context.

## Install

### Linux and macOS — one command

```bash
curl -fsSL https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.sh | bash
```

### Windows PowerShell — one command

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.ps1 | iex
```

The installer downloads the latest compatible GitHub Release, installs G-Type for the current user and launches it. Startup at login is **not** forced during installation; enable it later from **Dashboard → Settings → System** if desired.

Prebuilt releases currently target:

| Platform | Architecture |
|---|---|
| Linux | x86_64 |
| Windows | x86_64 |
| macOS | Intel x86_64 |
| macOS | Apple Silicon arm64 |

## First run

The first launch opens the local onboarding page:

```text
http://127.0.0.1:9741/setup
```

Setup verifies your Gemini API key, lets you choose a compatible transcription model and configures the initial push-to-talk hotkey.

## Daily use

Start G-Type:

```bash
g-type
```

Dashboard:

```text
http://127.0.0.1:9741/
```

Default controls:

```text
Ctrl+Shift+Space   Standard push-to-talk Mode
Ctrl+Shift+H       Hands-Free start / stop
Ctrl+Shift+E       Voice Edit (hold while speaking)
```

All hotkeys can be changed from the dashboard. G-Type rejects collisions between Mode hotkeys, Hands-Free and Voice Edit.

## Modes and application bindings

**Modes** replace the previous Profiles/Templates distinction in the user interface. Internally the configuration remains intentionally simple and backward compatible.

A Mode can define:

- global hotkey;
- Gemini model;
- request timeout;
- optional dedicated instructions.

The dashboard also includes ready-to-copy Mode presets such as clean transcription, professional email, meeting notes, brainstorming, checklists and bug reports.

### Binding an app to a Mode

G-Type never scans your whole computer for applications. It only lists contexts that have already appeared in transcription history.

1. Open the application.
2. Make at least one normal transcription inside it.
3. Open **Settings → Applications** and refresh.
4. Link that observed context to a Mode.

One context can point to only one Mode; one Mode can serve many contexts.

Resolution is deterministic:

```text
Explicit non-default Mode hotkey  → that Mode always wins
Default Mode / Hands-Free         → app binding if present, otherwise default Mode
```

There is no AI classifier or automatic Mode guessing.

## Voice snippets

Open **Settings → Snippets** and add entries such as:

```text
Trigger: calendar link
Value:   https://example.com/calendar
```

or:

```text
Trigger: work signature
Value:   Your Name
         Your Company
```

Enabled snippets are supplied to Gemini as contextual data and G-Type also applies a deterministic post-transcription replacement where safe. Limits are intentionally modest to keep the feature lightweight: up to 100 snippets, 100 characters per trigger and 4,000 characters per value.

## Voice Edit

1. Select editable text in the current application.
2. Hold the configured Voice Edit hotkey.
3. Say an instruction such as `make it shorter and more professional`.
4. Release the hotkey.

G-Type waits for the hotkey modifiers to be released, copies the current selection through the normal clipboard shortcut, sends the selected text + spoken instruction to Gemini in a **single request**, and replaces the selection with the result.

For safety, G-Type captures the application context at the beginning and checks it again before insertion. If focus moved to a different application, the generated result is retained in History but is **not inserted into the wrong window**.

## Recovery

Before every network request, G-Type saves a temporary local WAV plus enough metadata to reproduce the operation. If Gemini, networking, tracking or post-processing fails, the item remains available at:

```text
http://127.0.0.1:9741/recovery
```

Recovery keeps the original Mode, application context and operation. Voice Edit recovery also keeps the selected source text needed to regenerate the edit. Manual recovery saves the result to History but deliberately does not inject it into whichever application happens to be focused later.

## Dashboard

- **History** — latest transcriptions with text search, application/context, Mode, operation, duration, model and precise per-item cost.
- **Statistics** — usage, words, audio time, estimated typing time saved, models, tokens and costs.
- **Settings → General** — language, currency, microphone, default Mode, Hands-Free, Voice Edit, sounds and tray.
- **Settings → Modes** — Mode CRUD and ready-made presets.
- **Settings → Applications** — observed application contexts and optional Mode binding.
- **Settings → Snippets** — voice snippet editor.
- **Settings → API** — Gemini key management.
- **Settings → System** — startup at login, update status and runtime information.

## Updates

G-Type checks for releases in the background without blocking dictation.

Update at any time with:

```bash
g-type upgrade
g-type version
```

The updater validates the download, replaces the current binary only after a successful download and keeps a rollback path if replacement fails.

## Useful commands

```text
g-type                 Start the daemon
g-type setup           Open web onboarding
g-type stats           Show usage and cost statistics
g-type upgrade         Update to the latest release
g-type version         Show installed version
g-type config          Show the configuration path
g-type set-key <KEY>   Replace the Gemini API key
g-type test-audio      Test microphone capture
g-type list-devices    List input devices
g-type help            Show CLI help
```

## Data and privacy

- The dashboard binds to `127.0.0.1` only.
- Configuration, history and Recovery files remain in the current user's local directories.
- The Gemini API key is not returned in clear text by the dashboard API.
- Audio is sent to the configured Gemini API for transcription/editing.
- For new context-aware operations, foreground application metadata such as application name and, when safely available, a short window title/context may be included in the Gemini prompt and saved in local History.
- G-Type has no hosted account system or G-Type cloud database.

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

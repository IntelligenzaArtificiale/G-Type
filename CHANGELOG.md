# Changelog

## v1.6.0 — 2026-08-16

- Rebuilds the local dashboard around a calmer product shell inspired by modern desktop SaaS patterns: persistent navigation, stronger visual hierarchy, tighter spacing rules and a single coherent dark design system.
- Redesigns Cronologia as compact row-cards with clearer metadata, precise per-item costs, fast copy and a dedicated side drawer for complete transcription details without expanding the whole page.
- Keeps Statistics functionally unchanged while refining KPI cards, charts, model usage and technical metrics for better legibility and less visual noise.
- Reorganizes Settings into a stable vertical section navigator for General, Modes, Applications, Snippets, API and System, reducing the previous dense wall of controls without changing the underlying APIs or configuration model.
- Redesigns Recovery to match the main dashboard while preserving model-selectable retry, WAV opening, deletion confirmation and the existing local safety guarantees.
- Redesigns the three-step first-run onboarding as a cleaner split-layout setup flow while preserving API-key verification, model selection and hotkey capture.
- Adds an explicit transcription-details drawer and a fourth global KPI for transcription count without introducing new backend state or services.
- Extends CI to syntax-check the embedded JavaScript in Dashboard, Recovery and Setup, and keeps inline click handlers forbidden so UI behavior remains testable through event delegation.
- Preserves the local-only architecture, all existing endpoints, context awareness, Modes, application bindings, snippets, Hands-Free, Voice Edit, Recovery, cost tracking, update checks and autostart behavior.

## v1.5.0 — 2026-08-15

- Adds best-effort Context Awareness at recording start on Windows, macOS and Linux X11/XWayland, while keeping dictation functional when native Wayland does not expose foreground-app metadata.
- Unifies Profiles and Templates in the dashboard as Modalità/Modes, preserving the existing configuration model while adding application-to-Mode bindings with deterministic explicit-hotkey precedence.
- Adds Hands-Free dictation with a configurable toggle hotkey and keeps it on the same recording, Recovery, fallback, history and cost-tracking pipeline as normal push-to-talk.
- Adds Voice Edit: select text, hold the edit hotkey, dictate an instruction and release; G-Type captures the selection after key release, sends one contextual Gemini request and refuses final injection if focus moved to a different application.
- Adds local voice snippets with bounded trigger/value sizes, prompt context and deterministic post-transcription replacement where safe.
- Adds explicit backtrack/correction guidance to transcription prompts without introducing a separate AI classifier or hidden mode-selection logic.
- Extends local history with Mode, application context and operation metadata, while keeping older JSON-lines records readable through serde defaults.
- Extends Recovery metadata with application context, operation and Voice Edit source text so failed operations remain reconstructable without injecting recovered results into an arbitrary later-focused window.
- Adds live dashboard management for default Mode, Hands-Free/Voice Edit hotkeys, observed applications, app bindings and snippets; all hotkey collisions are validated before persistence.
- Logs successful Gemini fallback use explicitly and removes obsolete internal compatibility wrappers that were no longer called.
- Keeps the dashboard local-only on 127.0.0.1 and retains the existing crash-safe configuration, provider fallback and rollback-safe self-update behavior.
- Validates Linux with tests and strict Clippy, and compiles the supported release matrix for Windows x86_64, macOS Intel, macOS Apple Silicon and Linux x86_64 through GitHub Actions.

## v1.4.9 — 2026-08-14

- Reorganizes the dashboard Settings page into five focused tabs: General, Profiles, Templates, API and System, without changing the underlying configuration model or adding new services.
- Replaces the previous dense two-column settings wall with a single focused content area, clearer section descriptions and less visual competition between unrelated controls.
- Keeps profile editing, template creation, Gemini key management, autostart, update checks and global settings fully functional while making each area reachable with one internal tab click.
- Clarifies history scope: the persistent header and Statistics use the complete local history, while Cronologia explicitly labels the most recent records loaded for fast browsing and shows their partial cost/audio totals as such.
- Normalizes dashboard numeric formatting to Italian conventions, using commas for decimals and periods for thousands across costs, durations and averages.
- Rounds summary and total spending values to two decimals while retaining six-decimal precision for individual transcription costs and higher precision where small technical/model costs would otherwise lose useful detail.
- Keeps history pagination at five transcriptions per page and preserves all existing recovery, profile, currency conversion and update behavior.

## v1.4.8 — 2026-08-13

- Replaces the first-run setup page with a compact three-step browser onboarding for API key, model and global hotkey; no terminal questionnaire is required.
- Verifies Gemini API keys directly against the selected Gemini model before initial setup or dashboard key replacement is persisted.
- Adds a best-effort, read-only update check that runs outside the recording path and never blocks daemon startup when GitHub is unavailable.
- Exposes update state to the local dashboard and shows a concise notice when a newer release is available.
- Hardens self-update with connection/overall timeouts, minimum download-size validation, synced temporary writes and the existing rollback path before replacing the current executable.
- Fixes the default profile model for clean installations so a retired Gemini 2.0 endpoint can no longer be selected by the default configuration.
- Rewrites the English and Italian README files around the current one-command install, web onboarding, dashboard, recovery, profile and update flows.
- Clarifies the CLI setup command and first-run logs so they consistently describe browser-based configuration.

## v1.4.7 — 2026-08-13

- Redesigns Cronologia, Statistiche, Impostazioni and Recovery with a denser, more consistent desktop UI.
- Reduces history pagination to five transcriptions per page and improves search, expand/collapse, copy and cost visibility.
- Repairs zero-cost historical records in memory when the stored model and token counts are sufficient to reconstruct the cost.
- Limits dashboard display currency to USD and EUR and applies the selected conversion consistently to historical data.
- Makes language, currency, microphone, feedback sounds and tray configuration editable from the dashboard.
- Adds full profile editing for name, hotkey, Gemini model, timeout and custom prompt.
- Adds ten ready-to-use profile templates for common professional dictation workflows.
- Adds explicit deletion of preserved recovery audio with confirmation while keeping model-selectable retry and WAV opening.

## v1.4.6 — 2026-08-13

- Refreshes the Gemini audio-to-text model catalog and standard paid pricing from the official Google Gemini Developer API documentation, reviewed 2026-08-13.
- Adds current selectable one-shot audio→text models through Gemini 3.6 Flash, 3.5 Flash / Flash-Lite, 3.1 Flash-Lite / Pro Preview / 3 Flash Preview, and the supported Gemini 2.5 family; retired endpoints remain non-selectable.
- Corrects cost accounting by using prompt modality details when Google returns them: audio input and text prompt tokens are priced separately, while thinking tokens are included in output cost.
- Stops repeatedly retrying the same overloaded Gemini endpoint. On transient 503/5xx, 429, timeout or network failures, normal dictation can move to up to two stable inexpensive Flash-Lite fallbacks. Auth, configuration and bad-request failures never trigger automatic fallback.
- Records the model that actually produced the transcription, so history and cost tracking remain correct when a fallback model succeeds.
- Recovery now lets the user explicitly choose a different compatible Gemini model before retrying a preserved WAV; failed retries never remove the audio.
- Removes deprecated sampling parameters from Gemini 3.6 / 3.5 requests and uses low-cost thinking levels for transcription where supported.
- Exposes the model catalog through `/api/models`; dashboard/setup selectors use the same catalog instead of stale hard-coded model IDs.
- Keeps Live API native-audio models separate from one-shot generateContent transcription models so protocol-incompatible endpoints cannot be selected accidentally.

## v1.4.5 — 2026-08-12

- Preserves every completed recording as a local WAV before the Gemini request starts, so provider timeouts, 503 responses, network failures or interruptions during transcription no longer destroy the spoken audio.
- Adds a local Recovery queue with `/recovery`, surfaced directly from the dashboard whenever preserved recordings exist.
- Recovery items show duration, profile, model, failure reason and retry count; users can open the WAV or retry transcription later. Successful retries are written back into normal dashboard history.
- Keeps recovery WAVs on disk until the transcription has also been persisted to history; injection failures therefore cannot erase the recovered text.
- Replaces the fixed effective 10-second Gemini request timeout with an adaptive timeout based on recording duration. Typical budgets are ~25s for 20s audio, ~35s for 60s audio and ~82s for 245s audio, capped at 180s.
- Detects microphone streams that start but emit no audio within 1.5 seconds, aborting early with an error beep instead of letting the user dictate for a long time into an empty buffer.
- Keeps all recovery state local and filesystem-based; no database, background service or additional infrastructure is introduced.

## v1.4.4 — 2026-08-11

- Fixes dashboard transcription copy failures caused by embedding arbitrary transcript text inside inline JavaScript attributes; UI actions now use safe event delegation and a clipboard fallback.
- Makes all active transcription log truncation UTF-8 safe, including accented characters and emoji, and removes the obsolete duplicate Gemini network implementation.
- Applies each profile's configured request timeout to Gemini, honors custom prompts end-to-end, and keeps API/rate-limit errors out of the text injection path.
- Persists configuration through temporary-file + sync + backup replacement and automatically recovers from a valid backup if the primary TOML becomes unreadable.
- Makes text injection more reliable for Unicode, multiline text and slower applications by using a conservative clipboard strategy with safer settling times.
- Adds a recording watchdog and cleaner failure paths so a lost hotkey release or input-channel failure cannot leave an unbounded recording running.
- Falls back to the system microphone when a configured audio device has disappeared or been renamed.
- Implements a real Linux Wayland evdev hotkey listener with XWayland/rdev fallback when evdev devices are not readable.
- Keeps the existing Linux overlay safe mode and cross-platform CI matrix for Linux, Windows, macOS Intel and macOS Apple Silicon.

## v1.4.3 — 2026-08-11

- Fixes macOS/Windows compilation of the global input listener by using compile-time platform selection instead of `cfg!()` around the Linux-only evdev path.
- Adds macOS Intel, macOS Apple Silicon and Windows compile checks to normal CI so cross-platform regressions are caught before release.
- Includes the complete three-page dashboard redesign, live settings and Linux `GLXBadWindow` safe mode from v1.4.x.

## v1.4.2 — 2026-08-11

- Definitive release of the redesigned three-page dashboard and Linux safe mode.
- Release pipeline can now publish from a merged `release: v*` pull request as well as a release push, avoiding missed release triggers.
- Cross-platform binary matrix remains Linux x86_64, macOS Intel/Apple Silicon and Windows x86_64.

## v1.4.1 — 2026-08-11

- Keeps the full v1.4 dashboard redesign and Linux `GLXBadWindow` safe-mode fix.
- Makes GTK a Linux-only dependency so macOS/Windows release jobs do not require GTK system libs.
- Patch release prepared for the cross-platform binary matrix.

## v1.4.0 — 2026-08-11

- Dashboard reorganized into Cronologia, Statistiche and Impostazioni.
- Persistent header KPIs for total cost, time saved and dictated words.
- Full-width transcription history with search, pagination, copy action and correct total cost field.
- New local analytics endpoint and statistics dashboard with 14-day activity, model usage, token and efficiency metrics.
- Gemini API key can be updated from settings and is no longer returned in clear text by the dashboard state API.
- Profile name, hotkey, model and custom prompt remain editable live without daemon restart.
- Linux safe mode disables the WebKit/wry overlay by default to prevent `GLXBadWindow` crashes on X11/XWayland; set `G_TYPE_FORCE_OVERLAY=1` to opt in.
- Release workflow now derives the GitHub release tag from `Cargo.toml`.

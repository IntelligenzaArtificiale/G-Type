# Changelog

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
- Makes GTK a Linux-only dependency so macOS and Windows release builds do not require GTK system libraries.
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

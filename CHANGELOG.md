# Changelog

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

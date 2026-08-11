# Changelog

## v1.4.0 — 2026-08-11

- Dashboard reorganized into Cronologia, Statistiche and Impostazioni.
- Persistent header KPIs for total cost, time saved and dictated words.
- Full-width transcription history with search, pagination, copy action and correct total cost field.
- New local analytics endpoint and statistics dashboard with 14-day activity, model usage, token and efficiency metrics.
- Gemini API key can be updated from settings and is no longer returned in clear text by the dashboard state API.
- Profile name, hotkey, model and custom prompt remain editable live without daemon restart.
- Linux safe mode disables the WebKit/wry overlay by default to prevent `GLXBadWindow` crashes on X11/XWayland; set `G_TYPE_FORCE_OVERLAY=1` to opt in.
- Release workflow now derives the GitHub release tag from `Cargo.toml`.

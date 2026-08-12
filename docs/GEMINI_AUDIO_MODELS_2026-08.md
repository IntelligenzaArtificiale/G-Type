# Gemini audio-to-text model matrix — 2026-08-13

G-Type uses one-shot Gemini `generateContent` audio input → text output. This table is the engineering snapshot used by v1.4.6 for model selection and standard paid-tier cost tracking.

| Model | Status | Audio input | Standard input USD / 1M | Standard output USD / 1M |
|---|---|---:|---:|---:|
| `gemini-3.6-flash` | Stable | Yes | 1.50 | 7.50 |
| `gemini-3.5-flash` | Stable | Yes | 1.50 | 9.00 |
| `gemini-3.5-flash-lite` | Stable | Yes | 0.30 | 2.50 |
| `gemini-3.1-flash-lite` | Stable | Yes | 0.50 audio / 0.25 text | 1.50 |
| `gemini-3.1-pro-preview` | Preview | Yes | 2.00 <=200k; 4.00 >200k | 12.00 <=200k; 18.00 >200k |
| `gemini-3-flash-preview` | Preview | Yes | 1.00 audio / 0.50 text | 3.00 |
| `gemini-2.5-pro` | Stable | Yes | 1.25 <=200k; 2.50 >200k | 10.00 <=200k; 15.00 >200k |
| `gemini-2.5-flash` | Stable | Yes | 1.00 audio / 0.30 text | 2.50 |
| `gemini-2.5-flash-lite` | Stable | Yes | 0.30 audio / 0.10 text | 0.40 |

Notes:

- Gemini 3.6 Flash and Gemini 3.5 Flash-Lite became GA on 2026-07-21.
- `minimal`, `low`, `medium`, and `high` are thinking levels, not separate model IDs. G-Type requests minimal thinking for Flash/Flash-Lite transcription and low for 3.1 Pro Preview to minimize latency and thinking-token cost.
- Output prices above include thinking tokens. v1.4.6 accounts returned `thoughtsTokenCount` at the output rate.
- When `promptTokensDetails` is returned, audio and text prompt tokens are priced separately. If older responses omit modality details, G-Type conservatively prices all prompt tokens at the audio rate.
- `gemini-3.1-pro-preview-customtools` accepts audio but is intentionally excluded from the UI because it is specialized for custom/bash tool workflows, not dictation.
- Live API native-audio endpoints such as `gemini-3.1-flash-live-preview` and `gemini-2.5-flash-native-audio-preview-12-2025` use a different streaming protocol and are not drop-in one-shot transcription models.
- TTS endpoints accept text and generate audio, so they are not transcription models.
- Gemini 2.0 was shut down on 2026-06-01; old 2.0 pricing remains only for historical accounting and is never offered for new requests.

Official references:

- https://ai.google.dev/gemini-api/docs/audio
- https://ai.google.dev/gemini-api/docs/generate-content/audio
- https://ai.google.dev/gemini-api/docs/pricing
- https://ai.google.dev/gemini-api/docs/models
- https://ai.google.dev/gemini-api/docs/deprecations
- https://ai.google.dev/gemini-api/docs/changelog
- https://ai.google.dev/gemini-api/docs/generate-content/thinking

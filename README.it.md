# G-Type

<p align="center">
  <a href="README.md">English</a> •
  <a href="README.it.md"><b>Italiano</b></a> •
  <a href="README.es.md">Español</a> •
  <a href="README.pt-BR.md">Português (BR)</a> •
  <a href="README.hi.md">हिन्दी</a>
</p>

> 🔄 Tradotto da [`README.md`](README.md) — ultimo sync: commit `de04abd` (21 feb 2026)

**Daemon globale di dettatura vocale.** Tieni premuto un tasto ovunque nel sistema, parla, rilascia — le tue parole appaiono come testo digitato.

L'input vocale è [**~3× più veloce**](BENCHMARK.md) della digitazione negli scenari di inserimento testo ([Stanford/UW/Baidu, 2016](https://news.stanford.edu/stories/2016/08/stanford-study-speech-recognition-faster-texting)). G-Type elimina l'attrito: un solo tasto, zero interfaccia, funziona in ogni app.

Basato su Google Gemini REST API. Singolo binario statico. ~5 MB.

---

## Come funziona

```
┌─────────┐    Hotkey     ┌───────────┐    PCM 16kHz    ┌───────────┐
│ Tastiera │──────────────▶│   Audio   │────────────────▶│ REST API  │
│  Hook    │   (rdev)      │  Cattura  │   (buffered)    │  Gemini   │
└─────────┘               └───────────┘                 └─────┬─────┘
                                                              │
                                                         testo│
                                                              ▼
                          ┌───────────┐    keystrokes    ┌───────────┐
                          │   App     │◀────────────────│ Iniettore │
                          │  Attiva   │   o clipboard    │           │
                          └───────────┘                 └───────────┘
```

1. **Idle:** Il daemon attende il tuo hotkey. Uso minimo di risorse.
2. **Registrazione:** Il microfono cattura l'audio → converte in PCM mono 16kHz → bufferizza in memoria.
3. **Elaborazione:** Al rilascio del tasto, l'audio viene codificato in WAV, inviato all'API REST Gemini, trascrizione restituita.
4. **Iniezione:** Il testo viene digitato tramite emulazione tastiera. Fallback su clipboard per testi >500 caratteri.

## Installazione

### Installazione rapida (Linux e macOS)

```bash
curl -sSf https://raw.githubusercontent.com/IntelligenzaArtificiale/g-type/main/install.sh | bash
```

### Installazione rapida (Windows)

Apri PowerShell e esegui:

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/g-type/main/install.ps1 | iex
```

Entrambi gli installer automaticamente:
- Rilevano il tuo OS e architettura
- Installano le dipendenze di sistema necessarie (Linux)
- Scaricano l'ultimo binario pre-compilato
- Lo aggiungono al PATH
- Avviano il wizard di configurazione interattivo al primo avvio

### Binari pre-compilati

Scarica dalle [Release](https://github.com/IntelligenzaArtificiale/g-type/releases).

### Da sorgente (tutte le piattaforme)

```bash
# Prerequisiti: toolchain Rust + librerie audio/input di sistema
# Linux: sudo apt install libasound2-dev libx11-dev libxtst-dev libxdo-dev libevdev-dev
cargo install --path .
```

## Primo avvio

Al primo lancio, G-Type avvia un wizard di configurazione interattivo:

```
╔══════════════════════════════════════════════╗
║         G-Type — Prima Configurazione        ║
╚══════════════════════════════════════════════╝

  G-Type ha bisogno di una API key di Google Gemini.
  Ottienine una gratis su: https://aistudio.google.com/apikey

? 🔑 Gemini API Key: ****************************************
⠋ Verifica API key in corso...
✔ API key valida!

? 🤖 Seleziona Modello Gemini:
  > models/gemini-2.0-flash
    models/gemini-2.0-flash-lite
    models/gemini-2.5-flash
    ...

? 🌍 Lingua di trascrizione:
  > Auto-detect  (auto)
    Italiano  (it)
    English  (en)
    ...

? 🔊 Abilitare feedback sonoro?
  > Sì — beep all'inizio/fine registrazione
    No — modalità silenziosa

⌨️ Premi la combinazione di tasti desiderata (es. tieni premuto Ctrl+Shift+Spazio)...
  Hotkey catturata: ctrl+shift+space

  ✔ Config salvata in ~/.config/g-type/config.toml
```

Riesegui quando vuoi con `g-type setup`.

## Utilizzo

```bash
g-type                # Avvia il daemon (setup automatico al primo avvio)
g-type setup          # Riesegui il wizard di configurazione
g-type set-key KEY    # Aggiorna la API key
g-type config         # Mostra il percorso del file di configurazione
g-type test-audio     # Testa il microfono (3 secondi)
g-type list-devices   # Elenca dispositivi audio di input
```

Poi in **qualsiasi** applicazione:
1. Tieni premuto il tuo hotkey (default: `CTRL+SHIFT+SPAZIO`) e parla
2. Rilascia il tasto
3. Il testo appare nella posizione del cursore

## Configurazione

| Chiave           | Default                   | Descrizione                    |
|------------------|---------------------------|--------------------------------|
| `api_key`        | —                         | API key Google Gemini (obbligatoria) |
| `model`          | `models/gemini-2.0-flash` | Identificatore modello Gemini  |
| `hotkey`         | `ctrl+shift+space`        | Combinazione di tasti trigger  |
| `language`       | `auto`                    | Lingua trascrizione (auto, it, en, es, fr, de, ...) |
| `sound_enabled`  | `true`                    | Beep all'inizio/fine registrazione |
| `timeout_secs`   | `10`                      | Timeout richiesta HTTP (secondi) |

## Requisiti

- API key Google Gemini ([ottienine una gratis](https://aistudio.google.com/apikey))
- Microfono funzionante
- **Linux:** ALSA, X11, XTest libs (`libasound2-dev libx11-dev libxtst-dev libxdo-dev libevdev-dev`)
- **macOS:** Permessi di accessibilità per iniezione tastiera
- **Windows:** Nessun requisito aggiuntivo

## Contribuire

Vedi [CONTRIBUTING.md](CONTRIBUTING.md) (in inglese).

## Sicurezza

Vedi [SECURITY.md](SECURITY.md) (in inglese).

## Licenza

[MIT](LICENSE)

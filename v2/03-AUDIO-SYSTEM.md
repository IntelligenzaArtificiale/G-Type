# 03 — SISTEMA AUDIO: Cattura, Beep, Gestione Risorse

## Stato Attuale — Analisi Dettagliata

### File: `src/audio.rs` (891 righe)

Il modulo audio è il pezzo più solido della codebase. L'analisi riga per riga conferma:

**Punti eccellenti già implementati:**

1. **Downsampler streaming** (riga 258-337): Mantiene `resample_pos` fra callback. Interpolazione lineare corretta. Emette chunk di esattamente `SAMPLES_PER_CHUNK` (1600 samples = 100ms a 16kHz). Questo è codice da produzione.

2. **Device selection Linux** (riga 77-171): Legge `/proc/asound/cards` per identificare dispositivi USB. Filtra ALSA virtual devices (null, jack, oss, etc.). Preferisce USB > hw > default. Questo risolve un problema reale che la maggior parte dei competitor ignora.

3. **Soppressione stderr ALSA** (riga 16-55): RAII guard che redirige fd 2 a /dev/null durante l'enumerazione. Impedisce i messaggi spam ALSA/PipeWire. Geniale.

4. **Multi-formato** (riga 467-540): Gestisce I16, F32, U8, I32 con conversione corretta. F32 clamping a [-1, 1], U8 con center a 128.

### Problemi Trovati

**PROBLEMA 1: Nessuna selezione device configurabile**

`start_capture()` (riga 390) chiama sempre `default_input_device()`. Se l'utente ha 3 microfoni (built-in, USB, Bluetooth), non può scegliere quale usare.

**FIX**: Aggiungere campo `audio_device` nel profilo/config globale. Se impostato, cerca il device per nome. Se non trovato, fallback a default con warning.

```rust
// audio.rs — NUOVA funzione
pub fn find_device_by_name(name: &str) -> Result<Device> {
    let _guard = suppress_alsa_stderr();
    let host = cpal::default_host();
    
    if let Ok(devices) = host.input_devices() {
        for device in devices {
            if let Ok(dev_name) = device.name() {
                if dev_name.to_lowercase().contains(&name.to_lowercase()) {
                    return Ok(device);
                }
            }
        }
    }
    
    anyhow::bail!("Audio device '{}' not found. Use 'g-type list-devices' to see available devices.", name)
}

// Modificare start_capture per accettare device opzionale
pub fn start_capture(
    tx: AudioTx, 
    running: Arc<AtomicBool>,
    device_name: Option<&str>,  // NUOVO parametro
) -> Result<()> {
    let device = match device_name {
        Some(name) => find_device_by_name(name)?,
        None => default_input_device()?,
    };
    // ... rest unchanged ...
}
```

**PROBLEMA 2: Thread audio non fa cleanup esplicito**

In `start_capture()` (riga 416-580), il thread audio viene spawnato con `std::thread::spawn` (non Builder, non named in quel punto — ma il thread per rdev usa Builder). Quando `running` diventa false, il thread dropppa lo stream e termina. Ma il thread stesso non è joined — resta dangling fino al prossimo recording.

**FIX**: Restituire il JoinHandle e joinare esplicitamente in app.rs dopo il collector.

```rust
// audio.rs — Restituisci l'handle
pub fn start_capture(
    tx: AudioTx, 
    running: Arc<AtomicBool>,
    device_name: Option<&str>,
) -> Result<std::thread::JoinHandle<()>> {
    let device = match device_name {
        Some(name) => find_device_by_name(name)?,
        None => default_input_device()?,
    };
    let (config, sample_format) = pick_input_config(&device)?;
    // ...
    
    let handle = std::thread::Builder::new()
        .name("g-type-audio-capture".into())
        .spawn(move || {
            // ... current thread body ...
        })?;
    
    Ok(handle)
}
```

```rust
// app.rs — Join esplicito dopo recording
recording_flag.store(false, Ordering::Relaxed);
let all_samples = collector_handle.await??;
// Join audio thread to free resources
if let Some(handle) = audio_thread_handle {
    let _ = handle.join();
}
```

**PROBLEMA 3: Buffer pre-allocato fisso**

In `app.rs:169`:
```rust
let mut all_samples = Vec::<i16>::with_capacity(160_000);
```

160,000 samples = 10 secondi a 16kHz. Per dettature lunghe (>10s), il Vec reallocherà. Non è un bug, ma la pre-allocation potrebbe essere più intelligente.

**FIX**: Usare un ring buffer o semplicemente aumentare a 480,000 (30 secondi) che copre il 99% dei casi d'uso.

### File: `src/audio_feedback.rs` (110 righe)

**Analisi**: Ben fatto. Il pattern con `OnceLock<Sender>` e thread persistente è corretto. Il `_prev_sink` che tiene vivo il Sink precedente risolve un bug reale di rodio.

**PROBLEMA: Conflitto potenziale con capture**

`audio_feedback.rs:34` apre `OutputStream::try_default()` per l'output. Se il sistema audio ha un singolo device condiviso (alcuni setup ALSA), questo potrebbe entrare in conflitto con il capture aperto in `audio.rs`.

**FIX**: Il beep thread è già persistente e apre lo stream UNA volta. Il problema si verifica solo se il device di output E input sono lo stesso device e il backend non supporta full-duplex. Su PipeWire e PulseAudio questo non è un problema. Su ALSA raw potrebbe esserlo.

La soluzione è: aprire l'output stream PRIMA del capture (il beep thread si inizializza al primo `play_start_beep()`). Dato che la sequenza è `beep_start` → `start_capture`, questo è già l'ordine corretto. Nessuna modifica necessaria.

**OPZIONALE: Feedback più ricco**

Attualmente i beep sono sinusoidi pure. Per un prodotto premium:
- Start: un suono breve tipo "click" o "pop" (file WAV embedded, ~5KB)
- Stop: un suono di conferma sottile
- Error: un suono di errore discreto

```rust
// audio_feedback.rs — Suoni custom da file embedded
const START_SOUND: &[u8] = include_bytes!("../assets/sounds/start.wav");
const STOP_SOUND: &[u8] = include_bytes!("../assets/sounds/stop.wav");
const ERROR_SOUND: &[u8] = include_bytes!("../assets/sounds/error.wav");

// Usa rodio::Decoder per decodificare WAV embedded
fn play_embedded(data: &'static [u8]) {
    let cursor = std::io::Cursor::new(data);
    if let Ok(source) = rodio::Decoder::new(cursor) {
        let _ = beep_sender().send(BeepCmd::Custom(source));
    }
}
```

Generare i suoni: usa `ffmpeg` per creare WAV mono 16kHz di 100-200ms. Tienili sotto 10KB ciascuno.

---

## Gestione Risorse — Best Practices

### Microfono non bloccato

Il design attuale è già corretto: il microfono viene aperto SOLO quando si entra in stato Recording e chiuso (drop dello stream) quando si esce. Il microfono non è mai tenuto aperto in idle.

Verifica nel codice: `start_capture()` viene chiamato in `state_recording()` (app.rs:160). Quando la funzione termina (riga 271, return `State::Idle`), lo stream viene droppato perché è owned dal thread che termina.

### CPU durante idle

In idle, le uniche risorse usate sono:
- Thread rdev: bloccato su `rdev::listen()` — zero CPU, solo wake on event
- Thread beep: bloccato su `rx.recv()` — zero CPU
- Tokio runtime: bloccato su `input_rx.recv()` — zero CPU

Questo è ottimale. Non servono modifiche.

### Memoria durante recording

Il pattern collector (app.rs:168-177) accumula TUTTI i samples in un `Vec<i16>`. Per una dettatura di 60 secondi a 16kHz: 960,000 samples × 2 bytes = 1.9MB. Accettabile.

Per dettature molto lunghe (meeting recording, 1 ora): 57.6MB. Se questo diventa un caso d'uso, considera lo streaming diretto all'API (Gemini Live API via WebSocket) invece di accumulare tutto in RAM.

---

## Nuove Feature Audio

### Voice Activity Detection (VAD) — Futuro

Attualmente la registrazione è push-to-talk puro. Un miglioramento futuro è il VAD: rileva silenzio e stoppa automaticamente dopo N secondi di silenzio.

```rust
// audio.rs — VAD semplice basato su energia
const SILENCE_THRESHOLD: i16 = 500;  // calibrare empiricamente
const SILENCE_TIMEOUT_MS: u64 = 2000; // 2 secondi di silenzio = stop

fn is_silent(chunk: &[i16]) -> bool {
    let rms = (chunk.iter()
        .map(|&s| (s as f64).powi(2))
        .sum::<f64>() / chunk.len() as f64)
        .sqrt();
    rms < SILENCE_THRESHOLD as f64
}
```

Non implementare ora. È una feature per v2.1+.

### Audio Level Meter — Per l'Overlay

L'overlay pill può mostrare un indicatore del livello audio durante il recording. Il Downsampler già calcola i chunk, basta esporre il peak:

```rust
// audio.rs — Aggiungere peak tracking
pub struct AudioChunkWithPeak {
    pub samples: Vec<i16>,
    pub peak: f32, // 0.0-1.0 normalized
}

// Nel Downsampler::feed(), calcola il peak del chunk:
let peak = chunk.iter()
    .map(|s| s.unsigned_abs())
    .max()
    .unwrap_or(0) as f32 / i16::MAX as f32;
```

Questo peak viene passato all'overlay per animare una barra di volume nel pill.

---

## Checklist

- [ ] Aggiungere `audio_device: Option<String>` nella config globale
- [ ] Implementare `find_device_by_name()` in audio.rs
- [ ] Modificare `start_capture()` per accettare device opzionale
- [ ] Usare `std::thread::Builder::new().name()` per il thread audio
- [ ] Restituire JoinHandle da `start_capture()` e joinare in app.rs
- [ ] Aumentare pre-allocazione buffer a 480,000 (30 sec)
- [ ] Opzionale: suoni custom embedded (start.wav, stop.wav, error.wav)
- [ ] Opzionale: peak tracking per audio level meter nell'overlay
- [ ] Documentare che il microfono è aperto SOLO durante recording
- [ ] Test: verificare che dopo recording il device è rilasciato (altro programma può usare il mic)

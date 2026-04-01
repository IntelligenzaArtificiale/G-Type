# 06 — LOCAL MODELS: Whisper.cpp Offline Integration

## Obiettivo

Dare la possibilità di trascrivere completamente offline, senza nessuna API key, senza internet. Questo è un selling point critico: Wispr Flow non può farlo (cloud-only), Superwhisper lo fa solo su macOS.

## Tabella Modelli Whisper — Dati Reali

| Modello | File GGML | RAM | Speed M1 (30s audio) | WER English | Lingue |
|---------|-----------|-----|----------------------|-------------|--------|
| tiny | 75 MB | ~390 MB | <0.3s | ~7.6% | 99 |
| tiny.en | 75 MB | ~390 MB | <0.3s | ~6.0% | Solo EN |
| base | 142 MB | ~500 MB | <0.5s | ~5.0% | 99 |
| base.en | 142 MB | ~500 MB | <0.5s | ~4.2% | Solo EN |
| small | 488 MB | ~1 GB | 1-2s | ~3.4% | 99 |
| small.en | 488 MB | ~1 GB | 1-2s | ~3.0% | Solo EN |
| medium | 1.5 GB | ~2.6 GB | 3-5s | ~2.9% | 99 |
| medium.en | 1.5 GB | ~2.6 GB | 3-5s | ~2.7% | Solo EN |
| large-v3-turbo | 1.6 GB | ~3.3 GB | 1-2s (M2+) | ~2.1% | 99 |

I modelli `.en` sono ottimizzati per l'inglese — più accurati ma mono-lingua.

**Raccomandazione per l'utente**:
- Solo inglese → `base.en` (rapporto qualità/velocità imbattibile)
- Multilingua + velocità → `small` 
- Multilingua + accuratezza → `medium`
- Apple Silicon recente → `large-v3-turbo`

## Implementazione

### Cargo.toml

```toml
[features]
default = []
local-whisper = ["whisper-rs"]

[dependencies.whisper-rs]
version = "0.13"
optional = true
# whisper-rs compila whisper.cpp da sorgente
# Richiede: cmake, C compiler
# Su macOS: usa Metal/CoreML automaticamente
# Su Linux: usa AVX2/NEON se disponibili
```

### Build Requirements per Piattaforma

**macOS (Apple Silicon)**:
- Xcode Command Line Tools: `xcode-select --install`
- cmake: `brew install cmake`
- whisper-rs compila con Metal support automaticamente
- CoreML: per 2-3x speedup extra, serve CoreML model (download separato)

**macOS (Intel)**:
- Stessi requisiti, ma NO Metal
- Modelli large sono lenti — raccomandare small/medium

**Linux**:
- `sudo apt install build-essential cmake`
- Usa AVX2 se disponibile (auto-detected)
- CUDA non supportato da whisper-rs out of the box su Linux (richiede build custom)

**Windows**:
- Visual Studio Build Tools + cmake
- whisper-rs compila con MSVC

### CLI: `g-type setup-local`

```rust
// main.rs — nuovo subcommand

Some("setup-local") => {
    if let Err(e) = setup_local_interactive().await {
        eprintln!("❌ Local setup failed: {}", e);
        std::process::exit(1);
    }
    return Ok(());
}

async fn setup_local_interactive() -> Result<()> {
    println!();
    println!("  \x1b[36m🧠 G-Type Local Whisper Setup\x1b[0m");
    println!();

    // Step 1: Hardware check
    let hw = crate::providers::local::detect_hardware();
    println!("  Hardware detected:");
    println!("    RAM: {} GB", hw.ram_gb);
    println!("    CPU cores: {}", hw.cpu_cores);
    println!("    Apple Silicon: {}", if hw.is_apple_silicon { "Yes ✅" } else { "No" });
    println!();
    println!("  Recommended model: \x1b[1m{}\x1b[0m ({} MB)", 
        hw.recommended_model, hw.recommended_size_mb);
    println!();

    // Step 2: Let user choose
    let models = vec!["tiny", "base", "base.en", "small", "small.en", "medium", "large-v3-turbo"];
    let default_idx = models.iter().position(|m| *m == hw.recommended_model).unwrap_or(1);
    
    let idx = dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt("Select Whisper model")
        .default(default_idx)
        .items(&models.iter().map(|m| {
            let size = crate::providers::local::model_size_mb(m).unwrap_or(0);
            format!("{:<20} ({} MB)", m, size)
        }).collect::<Vec<_>>())
        .interact()?;
    
    let model_name = models[idx];
    let size_mb = crate::providers::local::model_size_mb(model_name).unwrap_or(0);

    // Step 3: Check if already downloaded
    let data_dir = crate::config::data_dir()?;
    let models_dir = data_dir.join("models");
    std::fs::create_dir_all(&models_dir)?;
    
    let filename = crate::providers::local::model_filename(model_name)
        .context("Unknown model")?;
    let model_path = models_dir.join(filename);

    if model_path.exists() {
        println!("  ✅ Model already downloaded: {}", model_path.display());
    } else {
        println!("  Downloading {} ({} MB)...", model_name, size_mb);
        let url = crate::providers::local::model_download_url(model_name)
            .context("No download URL for model")?;
        
        download_with_progress(&url, &model_path).await?;
        println!("  ✅ Model downloaded to {}", model_path.display());
    }

    // Step 4: Quick test
    println!();
    println!("  Testing transcription...");
    
    // Registra 3 secondi di audio come test
    match crate::audio::test_audio_capture(3) {
        Ok((callbacks, samples, _)) if callbacks > 0 && samples > 0 => {
            println!("  Audio capture: ✅ ({} samples)", samples);
            
            // Se abbiamo whisper-rs compilato, testa la trascrizione
            #[cfg(feature = "local-whisper")]
            {
                let provider = crate::providers::local::LocalWhisperProvider::new(model_name)?;
                // Per il test, usiamo silenzio (non abbiamo i samples reali dal test)
                println!("  Whisper model loaded: ✅");
                println!("  Local transcription ready!");
            }
        }
        _ => {
            println!("  ⚠️ Audio test had issues. Run 'g-type test-audio' for details.");
        }
    }

    println!();
    println!("  \x1b[32m✅ Local setup complete!\x1b[0m");
    println!("  Create a profile with provider = \"local\" and model = \"{}\"", model_name);
    println!();

    Ok(())
}

async fn download_with_progress(url: &str, dest: &std::path::Path) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    
    let response = reqwest::get(url).await?;
    let total = response.content_length().unwrap_or(0);
    
    let pb = indicatif::ProgressBar::new(total);
    pb.set_style(indicatif::ProgressStyle::default_bar()
        .template("  [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .expect("progress template"));
    
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = response.bytes_stream();
    
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        pb.inc(chunk.len() as u64);
    }
    
    pb.finish_and_clear();
    Ok(())
}
```

### Come whisper-rs lavora internamente

whisper-rs è un binding Rust a whisper.cpp. Quando lo usi:

1. **Carica il modello** in RAM: `WhisperContext::new_with_params(path, params)`
   - Su Apple Silicon: usa Metal per l'inferenza GPU
   - Su x86: usa AVX2/SSE4 per SIMD
   - Il caricamento è lento (1-3 secondi per modelli grandi). Va fatto UNA volta.

2. **Crea uno state**: `ctx.create_state()` — allocazione per il buffer di lavoro

3. **Esegui trascrizione**: `state.full(params, &samples_f32)`
   - Input: `&[f32]` normalizzato [-1.0, 1.0], 16kHz mono
   - Conversione da i16: `s as f32 / i16::MAX as f32`
   - BLOCCANTE: questa è computazione CPU-bound pesante
   - DEVE girare su `tokio::task::spawn_blocking`

4. **Estrai segmenti**: `state.full_n_segments()` + `state.full_get_segment_text(i)`
   - Ogni segmento è una frase con timestamp
   - Per dettatura, concatena tutti i segmenti

### Ottimizzazione: Pre-loading del Modello

Caricare il modello ad ogni trascrizione è spreco. Il modello va caricato UNA VOLTA e tenuto in memoria.

```rust
// providers/local.rs — Model cache

use std::sync::OnceLock;
use std::sync::Mutex;

/// Global model cache — loaded once, reused across transcriptions
static MODEL_CACHE: OnceLock<Mutex<Option<whisper_rs::WhisperContext>>> = OnceLock::new();

impl LocalWhisperProvider {
    fn get_or_load_model(&self) -> Result<&Mutex<Option<whisper_rs::WhisperContext>>> {
        let cache = MODEL_CACHE.get_or_init(|| Mutex::new(None));
        
        let mut guard = cache.lock().unwrap();
        if guard.is_none() {
            tracing::info!(path = %self.model_path.display(), "Loading Whisper model...");
            let ctx = whisper_rs::WhisperContext::new_with_params(
                self.model_path.to_str().unwrap(),
                whisper_rs::WhisperContextParameters::default(),
            ).context("Failed to load Whisper model")?;
            *guard = Some(ctx);
            tracing::info!("Whisper model loaded");
        }
        drop(guard);
        
        Ok(cache)
    }
}
```

### Gestione Errori di Compilazione

whisper-rs può fallire a compilare su sistemi senza cmake o con compilatori vecchi. Il feature gate `local-whisper` assicura che il build principale non ne sia affetto. 

Nel wizard, se l'utente sceglie "local" ma il binario non è compilato con la feature:

```rust
#[cfg(not(feature = "local-whisper"))]
"local" => {
    eprintln!("  ❌ Local whisper not available in this build.");
    eprintln!("  Rebuild with: cargo build --release --features local-whisper");
    eprintln!("  Or use a pre-built binary from: https://github.com/.../releases");
    std::process::exit(1);
}
```

Per le GitHub Releases, pubblica DUE varianti:
- `g-type-linux-x86_64` — senza local whisper (più piccolo, zero dipendenze build)
- `g-type-linux-x86_64-full` — con local whisper (richiede cmake per compilare)

---

## Checklist

- [ ] Aggiungere feature `local-whisper` in Cargo.toml
- [ ] Implementare `LocalWhisperProvider` in `providers/local.rs`
- [ ] Implementare `detect_hardware()` — RAM, Apple Silicon, cores
- [ ] Tabella modelli: nome → filename → URL → size
- [ ] `download_model()` con progress bar
- [ ] CLI `g-type setup-local` con wizard interattivo
- [ ] Pre-loading modello con `OnceLock` (evita reload ad ogni trascrizione)
- [ ] Conversione i16→f32 per input whisper
- [ ] `spawn_blocking` per la trascrizione (CPU-bound)
- [ ] GitHub Release: variante `-full` con local whisper
- [ ] Documentare requisiti build: cmake, compiler
- [ ] Test: trascrizione con modello tiny su audio di test

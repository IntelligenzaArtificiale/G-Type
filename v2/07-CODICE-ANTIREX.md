# 07 — FILOSOFIA CODICE: Principi Antirex per G-Type

## Il Manifesto

Questo documento definisce i principi di sviluppo per G-Type. Non sono linee guida — sono leggi. Ogni riga di codice, ogni architettura, ogni decisione viene valutata contro questi principi.

L'obiettivo non è scrivere codice "bello" o "elegante". L'obiettivo è scrivere codice che **funziona in produzione, è manutenibile da una persona sola per anni, e crea valore reale per chi lo usa**.

---

## Principio 1: Zero Fuffa

**Definizione**: Ogni riga di codice deve giustificare la propria esistenza. Se non puoi spiegare PERCHÉ una riga è lì, la riga non dovrebbe esistere.

**Cosa significa in pratica**:

❌ **FUFFA**: Pattern inseriti "perché si fa così"
```rust
// MALE: factory pattern per un singolo caso
trait AudioBackend { fn start(&self); }
struct CpalBackend;
impl AudioBackend for CpalBackend { fn start(&self) { /* unico backend */ } }
fn create_backend() -> Box<dyn AudioBackend> { Box::new(CpalBackend) }
```

✅ **ANTIREX**: Funzione diretta
```rust
// BENE: una sola implementazione = niente trait
fn start_audio_capture(tx: AudioTx, running: Arc<AtomicBool>) -> Result<()> {
    // Codice diretto. Se un giorno servono 2 backend, ALLORA fai il trait.
}
```

**Eccezione legittima**: Il `SttProvider` trait ha senso perché ci sono davvero 4 implementazioni diverse (Gemini, OpenAI, Deepgram, locale). L'astrazione è giustificata dalla molteplicità reale.

---

## Principio 2: Il Codice Più Semplice Che Funziona

**Non** il codice più corto. Non il codice più clever. Il codice che un developer mediocre può leggere e capire in 30 secondi.

**Esempio da G-Type attuale — BUONO**:
```rust
// tracking.rs:169 — calcolo costo
pub fn calculate_cost(model: &str, usage: &TokenUsage) -> (f64, f64, f64) {
    match model_pricing(model) {
        Some(pricing) => {
            let input_cost = usage.prompt_tokens as f64 * pricing.input_audio_per_m / 1_000_000.0;
            let output_cost = usage.candidates_tokens as f64 * pricing.output_per_m / 1_000_000.0;
            let total = input_cost + output_cost;
            (input_cost, output_cost, total)
        }
        None => (0.0, 0.0, 0.0),
    }
}
```

Nessuna astrazione. Nessun builder. Nessun pattern. Match, calcolo, return. Chiunque capisce cosa fa.

**Esempio da EVITARE**:
```rust
// MALE: over-engineering per "flessibilità futura"
struct CostCalculator<P: PricingProvider> {
    provider: P,
    rounding: RoundingStrategy,
    currency_converter: Box<dyn CurrencyConverter>,
}
impl<P: PricingProvider> CostCalculator<P> {
    fn calculate(&self, usage: &TokenUsage) -> CostResult {
        let raw = self.provider.lookup(usage.model())
            .map(|p| self.compute_raw(p, usage))
            .unwrap_or(CostResult::zero());
        self.rounding.apply(self.currency_converter.convert(raw))
    }
}
```

Questo è codice da tutorial enterprise. Nessuno ha bisogno di iniettare un `RoundingStrategy` in un dictation daemon. La moltiplicazione diretta basta.

---

## Principio 3: Gestione Errori Onesta

Non nascondere gli errori. Non inventare recovery "intelligente" che nessuno testerà mai. Sii chiaro su cosa è fallito e perché.

**Pattern G-Type — BUONO**:
```rust
// app.rs:226-236 — errore trascrizione
Err(e) => {
    error!(%e, "Transcription failed");
    warn!("Returning to idle due to transcription failure");
    if config.sound_enabled {
        crate::audio_feedback::play_error_beep();
    }
    return State::Idle;
}
```

L'errore viene loggato, l'utente sente un beep, il daemon torna idle. Non crasha. Non retria all'infinito. Non "degrada" verso un modello che non esiste.

**Pattern per i transform — RESILIENTE MA ONESTO**:
```rust
// Se un transform fallisce, usa il testo precedente, non bloccare
match t.apply(&text, ctx).await {
    Ok(transformed) => text = transformed,
    Err(e) => {
        warn!(%e, "Transform failed, keeping previous text");
        // NON interrompiamo. L'utente riceve testo meno processed ma riceve qualcosa.
    }
}
```

---

## Principio 4: Dipendenze Giustificate

Ogni dipendenza in `Cargo.toml` ha un costo: tempo di compilazione, rischio security, manutenzione, dimensione binario. Aggiungi una dipendenza SOLO se:

1. Implementarla da zero richiederebbe >200 righe di codice non banale
2. La libreria è matura (>1.0, o usata da progetti grandi)
3. Non porta con sé un albero di transitive deps gigante

**Esempio: Perché G-Type NON usa `chrono`**

In `tracking.rs:210-247`, la conversione epoch→data è fatta a mano con l'algoritmo di Howard Hinnant. Sono 37 righe di codice. Chrono aggiungerebbe una dipendenza con decine di file e 200KB di binario. Per convertire un timestamp. La scelta è corretta.

**Esempio: Perché G-Type USA `reqwest`**

Implementare un client HTTP con TLS, retry, connection pooling da zero sarebbe migliaia di righe. `reqwest` è la scelta ovvia.

**Dipendenze da NON aggiungere mai**:
- `serde_yaml` — TOML basta, non aggiungere un secondo formato config
- `log` — `tracing` è già lì e fa di più
- `thiserror` — `anyhow` è già lì e basta per un'app (thiserror serve per librerie)
- `clap` — il parsing CLI manuale in main.rs è 36 righe. Clap ne aggiungerebbe 200 di derive macro

---

## Principio 5: Naming Che Documenta

I nomi devono dire cosa fa il codice. Se serve un commento per spiegare cosa fa una funzione, il nome è sbagliato.

**BUONO (dal codebase attuale)**:
```rust
fn default_input_device() -> Result<Device>
fn pick_input_config(device: &Device) -> Result<(StreamConfig, SampleFormat)>
fn suppress_alsa_stderr() -> Option<StderrGuard>
fn is_newer(current: &str, latest: &str) -> bool
fn inject_clipboard(text: &str) -> Result<()>
```

Ogni nome dice esattamente cosa fa. Nessuna ambiguità.

**DA CORREGGERE**:
```rust
// network.rs:127 — "build_request_body" è troppo generico
fn build_request_body(wav_b64: &str, language: &str) -> Value
// MEGLIO:
fn build_gemini_transcription_request(wav_b64: &str, language: &str) -> Value
```

---

## Principio 6: Struttura File = Struttura Mentale

Ogni file ha UNA responsabilità. Se un file fa due cose, splittalo. Se un file ha >500 righe, probabilmente fa troppo.

**Struttura attuale G-Type — BUONA**:
```
main.rs      → CLI dispatch (nient'altro)
app.rs       → FSM (nient'altro)
audio.rs     → Cattura audio (nient'altro)
network.rs   → HTTP a Gemini (nient'altro)
injector.rs  → Injection testo (nient'altro)
config.rs    → Config + wizard (borderline: wizard potrebbe essere separato)
tracking.rs  → Storage + stats (borderline: 752 righe, potrebbe splittarsi)
```

**Struttura v2 — MIGLIORE**:
```
main.rs              → CLI dispatch
app.rs               → FSM + orchestrazione
config.rs            → Strutture config + load/save
config_wizard.rs     → Wizard interattivo (SPLIT da config.rs)
audio.rs             → Cattura mic
audio_encoding.rs    → WAV encoding (SPLIT da network.rs)
audio_feedback.rs    → Beep
input.rs             → Hotkey
injector.rs          → Injection + Wayland support
tray.rs              → System tray
overlay.rs           → Floating pill
providers/           → STT backends
transforms/          → Pipeline processing
settings/            → Web dashboard
tracking.rs          → Storage (≤400 righe)
tracking_stats.rs    → Aggregazione + display stats (SPLIT)
upgrade.rs           → Self-update
```

---

## Principio 7: Test Che Proteggono, Non Che Decorano

Non scrivere test per coverage. Scrivi test per i punti dove il codice PUÒ ROMPERSI.

**Test UTILI (già in G-Type)**:
```rust
// Test che la API key NON finisca nell'URL — questo è un test di sicurezza
fn test_api_url_no_key_leak() {
    let cfg = Config { api_key: "AIzaSySECRET".into(), /* ... */ };
    let url = cfg.api_url();
    assert!(!url.contains("SECRET"), "API key must not appear in URL");
}
```

**Test UTILI da aggiungere**:
```rust
// Test che il multi-hotkey non triggera il profilo sbagliato
fn test_multi_hotkey_no_cross_trigger() { /* ... */ }

// Test che il cleanup transform non distrugge testo valido
fn test_cleanup_preserves_normal_text() {
    assert_eq!(cleanup::apply("Hello world").unwrap(), "Hello world");
    assert_eq!(cleanup::apply("um hello uh world").unwrap(), "Hello world");
}

// Test che la migrazione v1→v2 preserva tutti i campi
fn test_config_migration_preserves_data() { /* ... */ }
```

**Test INUTILI da NON scrivere**:
```rust
// INUTILE: testa che Rust funziona
fn test_vec_push() {
    let mut v = vec![];
    v.push(1);
    assert_eq!(v.len(), 1);
}
```

---

## Principio 8: Performance Dove Serve, Non Ovunque

G-Type è un daemon che si attiva per pochi secondi alla volta. L'utente preme un tasto, parla 5-30 secondi, rilascia. Il 99% del tempo il processo è in idle a zero CPU.

**Dove la performance conta**:
- Latenza tra rilascio tasto e apparizione testo (percepita dall'utente)
- Allocazioni nel callback audio cpal (real-time thread, no allocazioni!)
- RAM durante recording lungo

**Dove la performance NON conta**:
- Tempo di caricamento config (una volta al boot)
- Rendering stats CLI (usato raramente)
- Setup wizard (usato una volta)

Il `Downsampler` (audio.rs:258) è correttamente ottimizzato: pre-allocazione buffer, zero allocazioni nel hot path, interpolazione lineare inline. Il wizard (config.rs) non è ottimizzato — e non deve esserlo.

---

## Principio 9: Backward Compatibility è Sacra

Quando un utente ha un `config.toml` v1, il software v2 DEVE leggerlo e migrarlo automaticamente. Mai chiedere all'utente di riscrivere la config. Mai rompere il formato di `usage.jsonl`.

```rust
// Pattern di migrazione
pub fn load() -> Result<ConfigV2> {
    let raw = fs::read_to_string(&path)?;
    
    // Try newest format first
    if let Ok(v2) = toml::from_str::<ConfigV2>(&raw) { return Ok(v2); }
    
    // Fallback to old format + auto-migrate
    if let Ok(v1) = toml::from_str::<ConfigV1>(&raw) {
        let v2 = migrate_v1(v1);
        save_v2(&v2, &path)?; // Sovrascrivi con nuovo formato
        return Ok(v2);
    }
    
    bail!("Unreadable config")
}
```

Per `usage.jsonl`: i nuovi campi (`text`, `profile`, `provider`) hanno `#[serde(default)]`. I record vecchi senza questi campi vengono letti senza errore.

---

## Principio 10: Ship > Perfect

Un feature al 90% in produzione batte un feature al 100% nel tuo branch locale. Il valore si crea quando l'utente lo usa, non quando il code review è clean.

**Ordine di priorità**:
1. Funziona (no crash, no data loss)
2. È usabile (UX non frustrante)
3. È veloce (latenza accettabile)
4. È bello (codice pulito)
5. È ottimale (micro-ottimizzazioni)

Non passare al livello N+1 finché il livello N non è solido.

---

## Applicazione ai Prossimi Step

Per ogni feature:

1. **Prima** scrivi il test che definisce il comportamento atteso
2. **Poi** scrivi l'implementazione più semplice che passa il test
3. **Poi** verifica che non rompe nulla di esistente (run `cargo test`)
4. **Poi** testa manualmente il flusso utente end-to-end
5. **Solo dopo** refactora se il codice è confuso

Non refactorare codice che funziona e che nessuno toccherà per mesi. Il refactoring ha valore solo se il codice verrà modificato a breve.

---

## Anti-Pattern da Evitare Assolutamente

| Anti-Pattern | Perché è male | Cosa fare invece |
|-------------|---------------|-----------------|
| Wrapper di wrapper | Aggiunge indirezione senza valore | Chiamata diretta |
| Config in 3 formati (TOML+YAML+JSON) | Triplo costo di manutenzione | Solo TOML |
| Trait con una sola implementazione | Astrazione prematura | Funzione diretta |
| Macro per risparmiare 3 righe | Illeggibile, non debuggabile | Codice esplicito |
| `unwrap()` in codice che tocca I/O | Crash in produzione | `?` o `.context()` |
| `clone()` ovunque per "evitare lifetimes" | Spreco memoria, nasconde ownership | Ragiona sui lifetimes |
| Commenti che ripetono il codice | Noise, si desincronizzano | Nomi auto-documentanti |
| File da 1000+ righe | Impossibile navigare | Split per responsabilità |
| Dipendenza per 10 righe di codice | Bloat, rischio supply chain | Scrivi le 10 righe |
| Feature flag per tutto | Complessità combinatoria | Ship o non ship |

---

## Checklist Pre-Commit

Per ogni PR/commit:

- [ ] `cargo build --release` compila senza warning
- [ ] `cargo test` passa al 100%
- [ ] `cargo clippy` senza warning (o warning giustificati)
- [ ] Nessun `unwrap()` in codice che tocca rete/file/config
- [ ] Nessuna dipendenza aggiunta senza giustificazione
- [ ] Nessun file >500 righe creato
- [ ] Backward compatibility config preservata
- [ ] Messaggi errore utili (non "Error: 1")
- [ ] Log con context sufficiente per debuggare in produzione

L'ambizione di creare un prodotto "ready for acquisition" richiede di abbandonare la mentalità da script amatoriale e adottare standard ingegneristici ferrei. Il codice in stile Antirez impone zero astrazioni inutili e gestione esplicita della memoria; lo standard Google impone telemetria (logging), tolleranza ai guasti (error recovery) e pipeline di distribuzione automatizzate.

Se i file superano le 500 righe, l'architettura è sbagliata. Se fallisce silenziosamente, è codice spazzatura.

Ecco il Master Document e il Prompt di generazione.

---

# 📄 MASTER SPECIFICATION: G-Type Core

**Architettura:** Rust Daemon Asincrono (Tokio) a singola binaria statica.
**Target:** Acquisizione enterprise / Adozione di massa open-source.

## 1. Filosofia di Sviluppo (First Principles)

* **Crash-Only Design:** Se un modulo fallisce (es. il microfono si disconnette), il thread isolato gestisce l'errore, logga l'evento e si riavvia senza far crashare il demone principale. Zero chiamate `unwrap()` o `expect()` nel codice di produzione. Solo propagazione degli errori tramite `anyhow`.
* **Zero-Copy Streaming:** L'audio catturato dal buffer hardware viene convertito e inviato nel socket di rete senza allocazioni intermedie su disco o in RAM superflua.
* **Separation of Concerns Rigida:** File piccoli (< 400 righe). Ognuno fa una sola cosa.

## 2. Alberatura del Repository

Un repository scalabile non contiene solo codice, ma infrastruttura.

```text
g-type/
├── .github/workflows/
│   └── release.yml        # CI/CD: Compila binari statici per Win/Mac/Linux ad ogni tag
├── src/
│   ├── main.rs            # [~150 righe] Entry point, inizializzazione Tokio e logger
│   ├── app.rs             # [~300 righe] FSM (Finite State Machine) e orchestrazione thread
│   ├── audio.rs           # [~250 righe] `cpal`: Cattura e ring buffer lock-free
│   ├── network.rs         # [~350 righe] `tokio-tungstenite`: WebSocket Client per Gemini Live API
│   ├── input.rs           # [~300 righe] `rdev`: Hook globale CTRL+T (cattura e soppressione)
│   ├── injector.rs        # [~200 righe] `enigo`/`arboard`: Logica adattiva Typewriter vs Clipboard
│   └── config.rs          # [~150 righe] `serde_json` o `toml`: Gestione API key locale
├── install.sh             # Script Unix per fetch automatico dell'ultima release
├── install.ps1            # Script Windows per fetch automatico dell'ultima release
├── Cargo.toml             # Manifest con dipendenze bloccate (strict versioning)
└── README.md              # Documentazione Google-style

```

## 3. Logica di Funzionamento (Il Loop Vitale)

1. **Boot:** Il programma legge `~/.config/g-type/config.toml`. Se manca l'API key, esce con un errore chiaro nel terminale.
2. **Idle:** Un thread C-level blocca in ascolto globale su `rdev`. RAM occupata < 15MB.
3. **Trigger (CTRL+T Down):**
* Avvia stream audio (`audio.rs`).
* Apre handshake TLS WebSocket (`network.rs`).
* Inizia lo stream PCM a 16kHz in chunk di 100ms.


4. **Release (CTRL+T Up):**
* Chiude lo stream audio.
* Invia segnale `turnComplete` all'API.
* Attende testo. Se latenza > 3s, abortisce e logga un warning.


5. **Injection:** Analizza lunghezza testo. < 80 char = emula tastiera; > 80 char = manipola clipboard nativa.

## 4. Distribuzione "One-Click"

L'utente non deve sapere cosa sia Rust o un compilatore. Il repository GitHub sfrutterà le Actions per compilare i binari.

*Comando Unix:*
`curl -sSf https://raw.githubusercontent.com/TUO_NOME/g-type/main/install.sh | bash`
*Comando Windows (PowerShell):*
`irm https://raw.githubusercontent.com/TUO_NOME/g-type/main/install.ps1 | iex`

Questo script scaricherà il binario compilato da GitHub Releases, lo metterà nel `PATH` e creerà il file di configurazione chiedendo l'API key a terminale.


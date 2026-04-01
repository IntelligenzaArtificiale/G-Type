# ⚡ TASKS-ANTIREX: Il Master Plan per G-Type V2

Questo documento è la Bibbia architetturale per l'upgrade a G-Type V2.
È stato redatto secondo le sacre Regole Antirex: *Zero Fuffa, Cross-Platform First, Gestione Onesta, Backward Compatibility Assoluta, Ship > Perfect.*

---

## 🎯 Da dove partire? L'Ordine Cronologico Logico

L'UI e i fronzoli si fanno alla fine. La UI deve essere guidata da una FSM (State Machine) solida e da un livello dati impeccabile. Non si parte mai dai bottoni, si parte dai bit.

Ecco la roadmap passo-passo della V2:

### 1. FONDAMENTA: Migrazione Storage e Multi-Profilo (Rif: 01) [COMPLETATO]
Tutto il resto dipende da *cosa* e *come* configuriamo il demone.
- **[x] Aggiornamento Configurazione**: Strutturare `global` (configurazioni generali, UI, server) + array `profiles` (lista di hotkey, provider, model).
- **[x] Layer di Migrazione V1 -> V2**: Scrittura del parser che prende un vecchio file `config.toml` (di `v1.1.0`), lo mappa su memoria e lo riscrive in formato V2 (Principio 9: non disturbare mai l'utente).
- **[x] Implementazione SharedHotkeys**: Scrittura e test isolato (senza demone backend) del sistema ad `Arc<RwLock>` per abilitare il rebinding on-the-fly degli hotkey.

### 2. CUORE NERVOSO: Riconoscimento Input (Rif: 01) [COMPLETATO]
Su Linux/Mac/Win ci sono enormi differenze e limiti con `rdev`.
- **[x] Refactoring Event Hook**: Integrazione multi-profilo dentro il sub-thread (`HookState`). Gestione delle overlap/priority dei tasti (es: Ctrl+Shift+E non deve triggerare Ctrl+Shift+Spazio).
- **[x] Linux Evdev Honesto**: Fix specifico Linux in `find_keyboard_devices()` (leggere `/dev/input/` e capire dal bitmask `EV_KEY` se è una vera tastiera per aggirare in modo pulito Wayland/X11 boundaries nei limiti del possibile).

### 3. CERVELLO: Motore Audio e Provider STT (Rif: 03 e 04) [COMPLETATO]
Il backend deve astrarre un *pochino* solo perché abbiamo 4 motori logici molto diversi (Gemini/OpenAI/Deepgram/Locale).
- **[x] FSM Backend Audio Isolabile**: Implementare la selezione opzionale dell'hardware (`audio_device` override), un refactoring della join logic (terminazione onesta del thread audio per evitare dangling `JoinHandle`).
- **[x] Memoria Preallocata Intelligente**: Espandere a ~30 secondi di Vec preallocata la registrazione per tagliare il 99% dei trigger di ri-allocamento (160k a 480k). E implementazione del pass-through del picco audio (`peak`) dal chunker.
- **[x] Il Trait `SttProvider`**: Creare `src/providers`. Astrazione minima indispensabile. Estrazione di `gemini.rs` dall'attuale `network.rs` + preparazione dello scheletro per OpenAI e Deepgram.
- **[x] Transform Pipeline (Cleanup/AI_Rewrite)**: Inserimento della pulizia regex per i "fillers". Principio Antirex di fallback: se l'AI Rewrite va in loop o dà errore, logghi warning e *restituisci il testo originale sporco*, non rompi il flusso!

### 4. MUSCOLI OFFLINE: Whisper Local (Rif: 06)
Feature fondamentale ma pesante, quindi la isoliamo (Principio 4: controllo rigoroso di `Cargo.toml`).
- **[ ] Inclusione `local-whisper` feature-gated**: Usa `whisper-rs`. Compilazione opzionale. Chi ha macchine deboli fa la release standard (solo backend cloud).
- **[ ] Memory OnceLock Cache**: Instanziazione del modello VRAM/RAM su primo start sfruttando lo starting delay per non bloccare l'IO durante il runtime.
- **[ ] Thread Dispatching (No Block)**: Trascrizione Whisper forzata su un pool `tokio::task::spawn_blocking` (computazione hardcore che non deve spegnere o crashare l'event-loop asincrono di tokio).

### 5. PELLE & SENSI: Vetrina Overlay & Web Settings (Rif: 02 e 05)
Ora che il corpo è un trattore Cingolato, mettiamo l'estetica. L'interfaccia deve competere con SaaS da 30$/mese come Wispr Flow, ma pesare 5MB di RAM. *Nessun framework GUI Rust gigante.* Tutto Web-Tech embedded.
- **[ ] Tray Icon Impeccabile**: Integrazione di `tray-icon` + `muda`. Feedback degli status `Idle`, `Recording`, `Processing`, `Error`. Menù contestuale nativo MacOS/Windows/Linux.
- **[ ] Bubble "Wispr Flow" Overlay (PILL)**: Interfaccia UI flottante (usiamo un approccio `wry` leggero). Il layer si aggancia all'hook e al `peak` audio. Finestra borderless, always-on-top, background trasparente.
- **[ ] Impostazioni alla "Server Locale"**: Un micro-server web Axum su TCP locale `127.0.0.1:9741` con HTTP/Tailwind standalone embeddato con `include_str!`. Zero installazioni frontend per l'utente.

---

## 💎 IL FATTORE "WOW": Ingegneria dell'UI & UX Sbalorditiva

Noi non facciamo robetta da smanettoni con interfacce anni '90. G-Type V2 deve fare **esplodere il cervello** all'utente medio appena lo usa. L'arte non è solo nel codice, è nell'emozione dell'utilizzo.

### A. La Bubble Trasparente (Glassmorphism Reale)
La `PILL` in sovrimpressione non è un noioso form. È un Webview (`wry`) borderless con `transparent=true`. 
- **CSS Avanzato**: Usiamo `backdrop-filter: blur(24px) saturate(180%)` per un effetto vetro smerigliato nativo stile macOS Sequoia.
- **Animazioni Organiche**: Transizioni `cubic-bezier` per l'espansione della pillola quando inizi a parlare. Il pallino rosso non lampeggia brutalmente, ma usa un `box-shadow` radiale pulsante che scala con l'intensità della tua voce (il `peak` level calcolato in Rust!).
- **Multi-Profilo Veloce**: A fianco del pallino rosso, compaiono piccoli "Chip" traslucidi per i profili. Il chip si auto-illumina di blu se usi il profilo secondario (es. "Risposta Mail"). Cliccabili o controllati da scorciatoia.

### B. Il Backend UI Web (SaaS Level)
L'utente clicca "Settings" nella Tray, si apre il browser di default a `localhost:9741`. Cosa vede?
- **Un Design Premium**: Un file `.html` iniettato nel binario Rust (via `include_str!`), formattato con Tailwind CSS. Tema scuro ultra-elegante (`bg-gray-950`, bordi `border-white/10`).
- **Dashboard Sensoriale**: Una landing page con i numeri enormi dei soldi risparmiati e le ore guadagnate. Grafici a linee morbidi (via Chart.js CDN) sulle statistiche di utilizzo.
- **Configurazione Reattiva**: Aggiungere un profilo genera una chiamata `POST` via fetch API al backend Axum in Rust, che riscrive il `TOML` e fa broadcast tramite hook per aggiornare le chiavi sulla tastiera. Il tutto *senza mai ricaricare la pagina*.

### C. Feedback Sonoro da Studio
Via il generatore di sinusoidi crude di `cpal`. Inseriremo tre piccoli file audio WAV/FLAC microscopici (es. 10KB l'uno) encodati nel binario con `include_bytes!`. 
- **Start**: Un pop morbido e soddisfacente (stile ricarica Airpods).
- **Procesing/Stop**: Un click delicato e grave.
- **Errore**: Un bump leggermente sordo.
Codice Arte = l'utente *sente* la qualità del prodotto senza guardare lo schermo.

---

## 🧨 Memento Operativi Antirex

1. **Nessun Cestino Temporaneo**: Se una libreria costa un casino o espande pesantemente i tempi di build per un beneficio effimero di 10 righe risparmiate, si fa senza e ci si codano brutalmente le 10 righe usando le API Rust Vanilla.
2. **Errori Non Mascherati**: `unwrap()` è morte. Si usa `?` oppure `.context()`. Se cade il microfono, si chiude graziosamente la pipeline, si suona il bump di errore elegante e si torna in `Idle`.
3. **Rust comanda, Web ubbidisce**: Manteniamo il footprint minuscolo garantendo che tutta la logica vera stia in Rust. JS, HTML e CSS servono *esclusivamente* per intercettare l'occhio dell'utente. Nessuna Single Page App React da compilare con Node.js. È Vanilla JS pompato da TailwindCSS.
4. **Il Codice Meno Complesso, Non Il Codice Meno Roso**: Un Match grezzo di 20 righe super esplicito è sempre meglio di una Factory Macro Invertita con closure di 350 righe "in caso scalasse in futuro" che confonde il debug. Scriviamo stupidi se la logica è stupida.
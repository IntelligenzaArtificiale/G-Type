# G-Type

**Dettatura vocale globale push-to-talk basata su Google Gemini.**

G-Type gira localmente in background, registra solo mentre tieni premuta una hotkey, invia l'audio a Gemini per la trascrizione e inserisce il testo nell'applicazione attiva.

L'obiettivo è semplice: **installi una volta, configuri dal browser e poi detti ovunque con il minimo attrito possibile.**

[English](README.md)

## Funzionalità principali

- Dettatura push-to-talk globale.
- Profili multipli con hotkey, modello Gemini, timeout e prompt opzionale.
- Dashboard locale su `http://127.0.0.1:9741/`.
- Cronologia, ricerca, statistiche e tracciamento costi.
- Costi visualizzabili in USD o EUR.
- Recupero locale dei WAV quando una trascrizione fallisce.
- Fallback automatico su modelli Flash-Lite stabili per errori temporanei.
- Template pronti per email, messaggi, riunioni, checklist, brainstorming, prompt AI e bug report.
- Onboarding web con verifica reale della Gemini API key prima del salvataggio.
- Controllo non bloccante delle nuove release.
- Self-update con rollback tramite `g-type upgrade`.
- Configurazione crash-safe con backup locale.
- Autoavvio opzionale configurabile dalla dashboard.

G-Type non richiede account G-Type, database cloud, backend remoto o estensioni browser.

## Piattaforme precompilate

| Piattaforma | Architettura |
|---|---:|
| Linux | x86_64 |
| Windows | x86_64 |
| macOS | Intel x86_64 |
| macOS | Apple Silicon arm64 |

## Installazione

### Linux e macOS

```bash
curl -fsSL https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.sh | bash
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.ps1 | iex
```

Gli installer scaricano l'ultima GitHub Release compatibile, installano G-Type nell'ambiente dell'utente e lo avviano.

**L'autoavvio non viene abilitato automaticamente dall'installer.** Se lo desideri, attivalo esplicitamente da **Dashboard → Impostazioni → Sistema → Avvia con il computer**. La scelta è per-utente e non richiede privilegi amministrativi.

## Primo avvio

Al primo avvio G-Type apre il setup locale:

```text
http://127.0.0.1:9741/setup
```

L'onboarding richiede tre passaggi:

1. **Gemini API key** — viene verificata direttamente con Gemini prima del salvataggio.
2. **Modello Gemini** — scegli un modello compatibile; quello consigliato è già selezionato.
3. **Hotkey globale** — mantieni `Ctrl+Shift+Space` oppure scegli un'altra combinazione.

Al termine viene salvata la configurazione locale e si apre la dashboard.

## Uso quotidiano

Avvia G-Type con:

```bash
g-type
```

Tieni premuta la hotkey del profilo, parla e rilasciala quando hai finito. G-Type registra soltanto durante la pressione, trascrive e inserisce il risultato nell'app attiva.

Dashboard:

```text
http://127.0.0.1:9741/
```

## Dashboard

### Cronologia

- Trascrizioni recenti con ricerca.
- Copia rapida del testo.
- Durata, parole, modello e costo per elemento.
- Ricostruzione dei costi storici quando i dati salvati lo consentono.

### Statistiche

- Trascrizioni, parole e audio totale.
- Tempo risparmiato stimato.
- Velocità media del parlato.
- Costi complessivi e costo per 1.000 parole.
- Attività degli ultimi 14 giorni e utilizzo per modello.

### Impostazioni

Puoi modificare:

- Lingua e valuta.
- Microfono.
- Suoni di feedback.
- Icona tray.
- **Avvio automatico con il computer.**
- Gemini API key, verificata prima della sostituzione.
- Profili, hotkey, modelli, timeout e prompt.
- Stato della versione e degli aggiornamenti.

La maggior parte delle modifiche viene applicata senza riavvio. Le opzioni legate all'integrazione grafica possono richiederlo e la dashboard lo segnala.

### Recupero

Se una richiesta Gemini fallisce, G-Type può conservare localmente il WAV invece di perdere la dettatura. Dalla sezione Recupero puoi ritentare lo stesso audio, scegliere un altro modello, aprire il WAV o eliminarlo.

## Aggiornamenti

G-Type controlla in background la disponibilità di nuove release senza entrare nel percorso critico della dettatura. Se GitHub non è raggiungibile, la dettatura continua normalmente.

Aggiorna con:

```bash
g-type upgrade
```

Verifica la versione con:

```bash
g-type version
```

L'updater valida il download, conserva temporaneamente un backup e ripristina il binario precedente se la sostituzione fallisce.

## Comandi utili

```text
g-type                 Avvia il demone
g-type setup           Apre il setup web
g-type stats           Mostra statistiche e costi
g-type upgrade         Aggiorna all'ultima release
g-type version         Mostra la versione installata
g-type config          Mostra il percorso della configurazione
g-type set-key <KEY>   Sostituisce la Gemini API key
g-type test-audio      Esegue un breve test microfono
g-type list-devices    Elenca i dispositivi di input
g-type help            Mostra l'help CLI
```

## Dati e privacy

- La dashboard ascolta solo su `127.0.0.1`.
- Configurazione, cronologia e recovery restano nelle directory locali dell'utente.
- La Gemini API key non viene restituita in chiaro dall'API della dashboard.
- L'audio necessario alla trascrizione viene inviato all'API Gemini configurata.
- G-Type non richiede un proprio account cloud o database remoto.

## Compilazione dal sorgente

```bash
git clone https://github.com/IntelligenzaArtificiale/G-Type.git
cd G-Type
cargo build --release
```

Controlli prima di contribuire:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Le release ufficiali vengono compilate da GitHub Actions per tutti i target supportati.

## Licenza

MIT. Vedi [LICENSE](LICENSE).

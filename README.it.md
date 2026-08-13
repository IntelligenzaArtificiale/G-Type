# G-Type

**Dettatura vocale globale push-to-talk basata su Google Gemini.**

G-Type gira localmente in background, registra solo mentre tieni premuta una hotkey configurata, invia l'audio catturato a Gemini per la trascrizione e inserisce il testo nell'applicazione che stai usando.

L'obiettivo è semplice: **installi una volta, configuri dal browser e poi detti ovunque con il minimo attrito possibile.**

[English](README.md)

## Cosa include G-Type

- Dettatura push-to-talk globale sulle piattaforme desktop supportate.
- Profili multipli, ognuno con hotkey, modello Gemini, timeout e prompt opzionale.
- Dashboard locale su `http://127.0.0.1:9741/`.
- Cronologia locale, ricerca, statistiche d'uso e tracciamento dei costi.
- Visualizzazione dei costi in USD o EUR, incluso lo storico.
- Recupero delle trascrizioni fallite: se una richiesta non va a buon fine, il WAV resta locale e può essere ritentato con un altro modello oppure eliminato.
- Fallback automatico su modelli stabili ed economici per gli errori temporanei di Gemini.
- Template pronti per email, trascrizione pulita, riunioni, brainstorming, checklist, bug report e altri flussi di lavoro comuni.
- Controllo automatico e non bloccante delle nuove release.
- Aggiornamento integrato con rollback tramite `g-type upgrade`.
- Onboarding web al primo avvio con verifica della Gemini API key prima del salvataggio.
- Scrittura della configurazione crash-safe con backup locale recuperabile.

G-Type **non** richiede un account G-Type, un database cloud, un backend remoto G-Type o un'estensione del browser.

## Piattaforme precompilate supportate

| Piattaforma | Architettura | Release pronta |
|---|---:|---:|
| Linux | x86_64 | Sì |
| Windows | x86_64 | Sì |
| macOS | Intel x86_64 | Sì |
| macOS | Apple Silicon arm64 | Sì |

Gli altri target possono essere compilati dal sorgente, ma gli installer a comando singolo qui sotto sono pensati per le piattaforme precompilate elencate sopra.

## Installazione con un solo comando

### Linux e macOS

```bash
curl -fsSL https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.sh | bash
```

L'installer scarica l'ultima GitHub Release compatibile, installa `g-type` nell'ambiente dell'utente e lo avvia. Al primo avvio G-Type apre automaticamente la configurazione nel browser.

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.ps1 | iex
```

Il comando è pensato per una normale finestra PowerShell. Per l'installazione standard per utente non sono necessari privilegi amministrativi.

## Onboarding iniziale

Non c'è più un questionario di configurazione da completare nel terminale.

Al primo avvio G-Type avvia il server locale e apre:

```text
http://127.0.0.1:9741/setup
```

L'onboarding è volutamente breve:

1. **Gemini API key** — inserisci la chiave e G-Type la verifica direttamente con l'API Gemini prima di salvarla.
2. **Modello Gemini** — scegli dal catalogo corrente di modelli compatibili con audio → testo; il modello consigliato è già selezionato.
3. **Hotkey globale** — mantieni `Ctrl+Shift+Space` oppure registra un'altra combinazione.

Alla fine G-Type salva la configurazione locale e apre la dashboard.

La Gemini API key può essere creata da Google AI Studio.

## Utilizzo quotidiano

Avvia G-Type:

```bash
g-type
```

Poi tieni premuta la hotkey del profilo, parla e rilasciala quando hai finito. G-Type registra solo durante la pressione, trascrive l'audio e inserisce il risultato nell'app attiva.

Il profilo predefinito parte con:

```text
Ctrl + Shift + Space
```

La dashboard è disponibile su:

```text
http://127.0.0.1:9741/
```

## Dashboard

La dashboard viene esposta soltanto sull'interfaccia locale loopback.

### Cronologia

- Trascrizioni recenti con cinque elementi per pagina.
- Ricerca per testo o modello.
- Espansione e riduzione dei testi lunghi.
- Copia rapida della trascrizione.
- Durata, numero di parole, modello e costo per elemento.
- Ricostruzione visuale dei costi storici quando modello e token salvati permettono di ricalcolarli.

### Statistiche

- Numero totale di trascrizioni e parole.
- Tempo audio e stima del tempo risparmiato rispetto alla digitazione.
- Velocità media del parlato.
- Costi complessivi e costo per 1.000 parole.
- Attività degli ultimi 14 giorni.
- Utilizzo e costo suddivisi per modello.

### Impostazioni

Dalla dashboard puoi gestire le impostazioni operative principali:

- Lingua o rilevamento automatico.
- Valuta visualizzata: USD o EUR.
- Microfono di input.
- Suoni di feedback.
- Icona tray.
- Gemini API key, verificata prima della sostituzione.
- Profili e template.
- Hotkey, modelli, timeout e prompt personalizzati dei profili.
- Versione installata e stato degli aggiornamenti.

La maggior parte delle modifiche a profili e impostazioni globali viene applicata senza riavviare G-Type. Le impostazioni dipendenti dall'integrazione grafica desktop possono richiedere un riavvio; la dashboard lo segnala quando necessario.

### Recupero

Se una richiesta verso Gemini fallisce, G-Type conserva localmente il WAV invece di perdere silenziosamente la dettatura.

Da **Recupero** puoi:

- Ritentare esattamente lo stesso audio.
- Scegliere un altro modello Gemini compatibile.
- Aprire il WAV locale.
- Eliminare definitivamente la registrazione fallita.

Quando il recupero riesce, la trascrizione viene salvata nella cronologia normale e l'elemento viene rimosso dalla coda di recupero.

## Profili

Un profilo è un comportamento di dettatura riutilizzabile associato a una hotkey.

Ogni profilo può definire:

- Nome.
- Hotkey globale.
- Modello Gemini.
- Timeout della richiesta.
- Prompt opzionale per trasformare il parlato in uno specifico output di lavoro.

La dashboard include template pronti come:

- Trascrizione pulita.
- Email professionale.
- Messaggio rapido.
- Brainstorming → piano.
- Note riunione.
- Task e checklist.
- Prompt per AI.
- Aggiornamento stato.
- Testo formale.
- Bug report.

Dopo la creazione, i template sono normali profili: possono essere modificati o eliminati in qualsiasi momento.

## Aggiornamenti

Quando parte la dashboard locale, G-Type esegue in background un controllo leggero e in sola lettura sull'ultima release disponibile. Se GitHub non è raggiungibile o il controllo fallisce, la dettatura continua normalmente.

La dashboard mostra anche quando è disponibile una versione più recente.

Per aggiornare G-Type su tutte le piattaforme supportate si usa lo stesso comando:

```bash
g-type upgrade
```

Poi riavvia il processo in esecuzione per utilizzare il nuovo binario.

Per verificare la versione installata:

```bash
g-type version
```

L'updater scarica il nuovo asset accanto all'eseguibile corrente, valida il download prima della sostituzione, conserva temporaneamente un backup e ripristina la versione precedente se l'installazione del nuovo binario fallisce.

## Comandi utili

```text
g-type                 Avvia il demone di dettatura
g-type setup           Avvia G-Type se necessario e apre il setup web
g-type stats           Mostra statistiche e costi nel terminale
g-type upgrade         Aggiorna all'ultima GitHub Release compatibile
g-type version         Mostra la versione installata
g-type config          Mostra il percorso esatto del file di configurazione
g-type set-key <KEY>   Sostituisce la Gemini API key da terminale
g-type test-audio      Esegue un breve test del microfono
g-type list-devices    Elenca i dispositivi di input disponibili
g-type help            Mostra l'help CLI
```

Per l'uso normale è preferibile configurare G-Type dalla dashboard invece di modificare manualmente i file.

## Dati e privacy

G-Type è progettato in modo local-first:

- La dashboard è in ascolto su `127.0.0.1`, non su un'interfaccia di rete pubblica.
- Configurazione e cronologia d'uso vengono memorizzate nelle directory locali dell'utente corrente.
- La Gemini API key viene salvata nella configurazione locale e non viene restituita dall'API della dashboard; all'interfaccia arriva solo una versione mascherata.
- L'audio necessario alla trascrizione viene inviato all'API Gemini configurata.
- Una trascrizione fallita può lasciare un WAV locale nella coda di recupero per permettere un nuovo tentativo.
- G-Type non richiede un account cloud o un database remoto proprio.

Usa:

```bash
g-type config
```

per ottenere il percorso esatto della configurazione sul sistema operativo in uso.

## Robustezza

Il demone resta volutamente compatto, ma i percorsi critici sono difensivi:

- La configurazione viene scritta tramite file temporaneo più backup.
- Una configurazione primaria corrotta può essere recuperata dall'ultimo backup valido.
- Gli errori Gemini vengono classificati per distinguere problemi temporanei di rete/sovraccarico da errori di autenticazione o richieste non valide.
- Gli errori temporanei possono usare fallback Flash-Lite stabili invece di ripetere più volte la stessa richiesta fallita.
- Le registrazioni non trascritte vengono preservate localmente per il recupero manuale.
- Il self-update usa un asset temporaneo e rollback invece di sovrascrivere alla cieca l'eseguibile corrente.
- Il controllo delle nuove release è best-effort e non entra nel percorso critico di registrazione.
- La dashboard locale non dipende da un servizio cloud di G-Type.

## Risoluzione problemi

### La dashboard non si apre

Assicurati che G-Type sia in esecuzione:

```bash
g-type
```

Poi apri:

```text
http://127.0.0.1:9741/
```

Se G-Type segnala che un'altra istanza è già in esecuzione, la dashboard di quell'istanza dovrebbe essere già disponibile sulla porta `9741`.

### Test microfono

```bash
g-type test-audio
```

Per elencare i dispositivi:

```bash
g-type list-devices
```

Poi puoi selezionare il microfono desiderato da Dashboard → Impostazioni.

### Problemi con la API key

Riapri il setup:

```bash
g-type setup
```

oppure sostituisci la chiave da Dashboard → Impostazioni. La nuova chiave viene verificata prima di essere salvata.

### Una trascrizione è fallita

Apri Dashboard → Recupero. Se il WAV è stato preservato, puoi ritentarlo con un altro modello senza ripetere la dettatura.

### Overlay Linux

Il demone di dettatura, la tray e la dashboard locale possono funzionare anche con l'overlay visivo disabilitato. Su Linux l'overlay è volutamente conservativo perché gli ambienti misti Wayland/XWayland/GTK possono essere instabili.

## Compilazione dal sorgente

G-Type è scritto in Rust.

```bash
git clone https://github.com/IntelligenzaArtificiale/G-Type.git
cd G-Type
cargo build --release
```

Su Linux la compilazione richiede le librerie native audio, X11/GTK/WebKit usate dall'integrazione desktop. Il workflow CI contiene l'elenco esatto dei pacchetti Ubuntu usati per le build ufficiali.

Prima di contribuire:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Release

Le release ufficiali vengono compilate da GitHub Actions per tutti i target supportati. Il workflow esegue i controlli di qualità prima di creare i binari e pubblicare la GitHub Release.

`g-type upgrade` risolve sempre l'ultima release pubblicata e seleziona l'asset compatibile con sistema operativo e architettura correnti.

## Licenza

MIT. Vedi [LICENSE](LICENSE).

# G-Type

**Input vocale locale, contestuale e globale basato su Google Gemini.**

G-Type gira in background, registra soltanto quando lo invochi, usa la tua Gemini API key e inserisce il risultato nell'applicazione attiva. La versione 1.5 aggiunge Context Awareness, Modalità, associazioni applicazione→Modalità, snippet vocali, Hands-Free e Voice Edit senza introdurre account G-Type, backend ospitati o database cloud.

[English](README.md)

## Funzionalità principali

- **Dettatura push-to-talk globale** con hotkey configurabili.
- **Context Awareness**: rileva in modo best-effort l'applicazione in primo piano all'avvio della registrazione, usa il contesto per migliorare la comprensione e lo conserva nella cronologia locale.
- **Modalità**: ogni Modalità può avere hotkey, modello Gemini, timeout e istruzioni dedicate.
- **Associazioni applicazione → Modalità**: un contesto già osservato può essere collegato a una Modalità. Una hotkey esplicita di una Modalità non predefinita prevale sempre sull'associazione automatica.
- **Snippet vocali**: associa una frase pronunciata a testo esatto, URL, email, numeri o firme.
- **Backtrack**: gestisce correzioni vocali esplicite come “alle quattro, anzi alle cinque” mantenendo la versione finale corretta.
- **Hands-Free**: premi una volta per iniziare e una seconda volta per terminare. Default: `Ctrl+Shift+H`.
- **Voice Edit**: seleziona un testo, tieni premuta la hotkey, detta l'istruzione di modifica e rilascia. Default: `Ctrl+Shift+E`.
- **Cronologia, statistiche e costi locali** con Modalità, applicazione e tipo di operazione.
- **Recovery locale** dei WAV prima delle richieste di rete, così un errore Gemini o di rete non distrugge l'audio già registrato.
- **Fallback Gemini** su modelli compatibili in caso di errori temporanei.
- **Controllo aggiornamenti in background** e self-update con rollback.
- **Autoavvio opzionale** gestibile dalla dashboard.

G-Type non richiede un account G-Type, un'estensione browser o servizi cloud proprietari.

## Compatibilità

Le release precompilate vengono prodotte per:

| Piattaforma | Architettura |
|---|---|
| Linux | x86_64 |
| Windows | x86_64 |
| macOS | Intel x86_64 |
| macOS | Apple Silicon arm64 |

Il rilevamento del contesto è deliberatamente best-effort. Su Windows e macOS utilizza primitive di sistema; su Linux usa le informazioni EWMH disponibili in X11/XWayland. Un compositor Wayland nativo può non esporre l'applicazione attiva: in quel caso G-Type continua normalmente senza contesto.

## Installazione

### Linux e macOS

```bash
curl -fsSL https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.sh | bash
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.ps1 | iex
```

Gli installer scaricano l'ultima GitHub Release compatibile, installano G-Type per l'utente corrente e lo avviano. L'autoavvio non viene imposto durante l'installazione: puoi abilitarlo successivamente da **Dashboard → Impostazioni → Sistema**.

## Primo avvio

Al primo avvio si apre il setup locale:

```text
http://127.0.0.1:9741/setup
```

Il setup verifica la Gemini API key, permette di scegliere un modello compatibile e configura la prima hotkey push-to-talk.

## Uso quotidiano

Avvia G-Type:

```bash
g-type
```

Dashboard:

```text
http://127.0.0.1:9741/
```

Controlli predefiniti:

```text
Ctrl+Shift+Space   Modalità standard push-to-talk
Ctrl+Shift+H       Hands-Free: avvia / termina
Ctrl+Shift+E       Voice Edit: tieni premuto mentre parli
```

Tutte le hotkey sono modificabili dalla dashboard. G-Type rifiuta collisioni tra hotkey delle Modalità, Hands-Free e Voice Edit.

## Modalità e associazioni applicative

La UI della v1.5 unifica i precedenti Profili/Template nel concetto di **Modalità**. La configurazione resta semplice e retrocompatibile.

Una Modalità può definire:

- hotkey globale;
- modello Gemini;
- timeout della richiesta;
- istruzioni personalizzate.

La dashboard include anche preset per trascrizione pulita, email professionali, note riunione, brainstorming, checklist e bug report.

### Collegare un'app a una Modalità

G-Type non esegue scansioni dell'intero computer. Mostra soltanto applicazioni o contesti già osservati durante l'uso.

1. Apri l'applicazione interessata.
2. Esegui almeno una normale dettatura al suo interno.
3. Vai in **Impostazioni → Applicazioni**.
4. Associa il contesto osservato a una Modalità.

La risoluzione è deterministica:

```text
Hotkey esplicita di Modalità non predefinita → quella Modalità prevale sempre
Modalità predefinita / Hands-Free           → usa il binding dell'app se presente
Nessun binding                              → usa la Modalità predefinita
```

Non viene usato un classificatore AI per indovinare automaticamente la Modalità.

## Snippet vocali

Da **Impostazioni → Snippet** puoi creare scorciatoie come:

```text
Trigger: link calendario
Valore:  https://example.com/calendario
```

oppure:

```text
Trigger: firma lavoro
Valore:  Nome Cognome
         Azienda
```

Gli snippet abilitati vengono forniti a Gemini come contesto e, quando possibile, viene applicata anche una sostituzione deterministica post-trascrizione. I limiti sono volutamente contenuti: massimo 100 snippet, trigger fino a 100 caratteri e valore fino a 4.000 caratteri.

## Hands-Free

Premi la hotkey Hands-Free una volta per iniziare a registrare e una seconda volta per terminare. La registrazione passa nello stesso pipeline sicuro della dettatura normale, compresi Recovery, fallback Gemini, cronologia e cost tracking.

## Voice Edit

1. Seleziona del testo modificabile nell'applicazione corrente.
2. Tieni premuta la hotkey Voice Edit.
3. Pronuncia un'istruzione, per esempio: `rendilo più corto e professionale`.
4. Rilascia la hotkey.

G-Type acquisisce la selezione dopo il rilascio della combinazione, invia testo selezionato + istruzione vocale a Gemini in una singola operazione e sostituisce la selezione con il risultato.

Per sicurezza, l'applicazione attiva viene confrontata prima dell'inserimento finale. Se nel frattempo il focus è passato a un'altra applicazione, il risultato resta disponibile in Cronologia ma non viene inserito nella finestra sbagliata.

## Recovery

Prima della richiesta Gemini G-Type conserva localmente un WAV temporaneo con i metadati necessari a ricostruire l'operazione. Se Gemini, la rete, il salvataggio o il post-processing falliscono, la registrazione rimane disponibile in:

```text
http://127.0.0.1:9741/recovery
```

Il Recovery conserva Modalità, contesto applicativo e tipo di operazione; per Voice Edit conserva anche il testo selezionato necessario a rigenerare la modifica. Un recupero manuale salva il risultato in Cronologia ma non lo inietta automaticamente nell'app che potrebbe trovarsi in primo piano molto tempo dopo.

## Dashboard

- **Cronologia** — trascrizioni recenti con ricerca, applicazione/contesto, Modalità, operazione, durata, modello e costo.
- **Statistiche** — utilizzo, parole, tempo audio, tempo risparmiato stimato, modelli, token e costi.
- **Impostazioni → Generali** — lingua, valuta, microfono, Modalità predefinita, Hands-Free, Voice Edit, suoni e tray.
- **Impostazioni → Modalità** — creazione e modifica delle Modalità e preset pronti.
- **Impostazioni → Applicazioni** — contesti osservati e associazioni opzionali alle Modalità.
- **Impostazioni → Snippet** — editor degli snippet vocali.
- **Impostazioni → API** — gestione della Gemini API key.
- **Impostazioni → Sistema** — autoavvio, aggiornamenti e informazioni runtime.

## Aggiornamenti

G-Type verifica le nuove release in background senza bloccare la dettatura.

Aggiorna con:

```bash
g-type upgrade
```

Controlla la versione con:

```bash
g-type version
```

L'updater valida il download, sostituisce il binario soltanto dopo un download riuscito e mantiene un percorso di rollback in caso di errore durante la sostituzione.

## Comandi utili

```text
g-type                 Avvia il demone
g-type setup           Apre il setup web
g-type stats           Mostra statistiche e costi
g-type upgrade         Aggiorna all'ultima release
g-type version         Mostra la versione installata
g-type config          Mostra il percorso della configurazione
g-type set-key <KEY>   Sostituisce la Gemini API key
g-type test-audio      Esegue un test del microfono
g-type list-devices    Elenca i dispositivi di input
g-type help            Mostra l'help CLI
```

## Dati e privacy

- La dashboard ascolta soltanto su `127.0.0.1`.
- Configurazione, cronologia e Recovery rimangono nelle directory locali dell'utente.
- La Gemini API key non viene restituita in chiaro dall'API della dashboard.
- L'audio viene inviato alla Gemini API configurata per trascrizione o modifica.
- Nelle operazioni contestuali possono essere inclusi nel prompt e nella cronologia locale nome applicazione e, quando disponibile in modo sicuro, un titolo/contesto breve della finestra.
- G-Type non dispone di un proprio account cloud o database remoto.

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

# G-Type

<p align="center">
  <strong>Input vocale locale, contestuale e globale basato su Google Gemini.</strong>
</p>

<p align="center">
  <a href="https://github.com/IntelligenzaArtificiale/G-Type/releases/latest"><img alt="Ultima release" src="https://img.shields.io/github/v/release/IntelligenzaArtificiale/G-Type?display_name=tag&sort=semver"></a>
  <a href="https://github.com/IntelligenzaArtificiale/G-Type/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/IntelligenzaArtificiale/G-Type/ci.yml?branch=main&label=CI"></a>
  <a href="LICENSE"><img alt="Licenza MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="Dashboard locale" src="https://img.shields.io/badge/dashboard-127.0.0.1%3A9741-6f42c1">
</p>

<p align="center">
  <a href="README.md">English</a> •
  <a href="README.it.md"><b>Italiano</b></a> •
  <a href="README.es.md">Español</a> •
  <a href="README.pt-BR.md">Português (BR)</a> •
  <a href="README.hi.md">हिन्दी</a>
</p>

G-Type gira in background, registra soltanto quando lo invochi, usa la tua Gemini API key e inserisce il risultato nell'applicazione attiva. **v1.5.0** aggiunge Context Awareness, Modalità, associazioni applicazione→Modalità, snippet vocali, Hands-Free e Voice Edit senza introdurre account G-Type, backend ospitati o database cloud.

<p align="center">
  <img src="docs/assets/g-type-v1.5-flow.svg" alt="Flusso di elaborazione G-Type v1.5" width="100%">
</p>

## Avvio rapido

### 1. Installa

Linux e macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.sh | bash
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.ps1 | iex
```

### 2. Completa il primo setup

G-Type apre la pagina locale:

```text
http://127.0.0.1:9741/setup
```

Inserisci la Gemini API key, scegli un modello compatibile e configura la prima hotkey push-to-talk.

### 3. Inizia a dettare

```bash
g-type
```

Tieni premuta la hotkey configurata, parla e rilasciala.

Dashboard:

```text
http://127.0.0.1:9741/
```

### 4. Aggiorna in seguito

```bash
g-type upgrade
g-type version
```

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

I binari ufficiali sono pubblicati nelle [GitHub Releases](https://github.com/IntelligenzaArtificiale/G-Type/releases).

## Installazione

Gli installer scaricano l'ultima GitHub Release compatibile, installano G-Type per l'utente corrente e lo avviano. L'autoavvio non viene imposto durante l'installazione: puoi abilitarlo successivamente da **Dashboard → Impostazioni → Sistema**.

## Primo avvio

Al primo avvio si apre il setup locale:

```text
http://127.0.0.1:9741/setup
```

Il setup verifica la Gemini API key, permette di scegliere un modello compatibile e configura la prima hotkey push-to-talk.

Se il setup non si apre automaticamente:

```bash
g-type setup
```

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

Se G-Type è avviato in primo piano, fermalo con `Ctrl+C` e riavvialo con `g-type`. Se hai abilitato l'autoavvio, gestiscilo da **Dashboard → Impostazioni → Sistema** invece di creare una seconda voce di avvio manuale.

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

**Non cancellare manualmente la cartella Recovery se contiene registrazioni che ti servono ancora.** Prima prova il recupero dalla pagina dedicata.

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
g-type version
```

L'updater valida il download, sostituisce il binario soltanto dopo un download riuscito e mantiene un percorso di rollback in caso di errore durante la sostituzione.

Se G-Type sta girando in primo piano, il flusso più sicuro è: `Ctrl+C`, `g-type upgrade`, `g-type version`, quindi `g-type` per riavviarlo.

## Comandi utili

```text
g-type                 Avvia G-Type
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

## Controlli rapidi

Verifica la versione installata:

```bash
g-type version
```

Testa il microfono senza fare una normale dettatura:

```bash
g-type test-audio
```

Elenca i dispositivi di input:

```bash
g-type list-devices
```

Trova il percorso della configurazione attiva:

```bash
g-type config
```

Se una registrazione completata fallisce durante l'elaborazione Gemini/rete, apri Recovery prima di cancellare file locali:

```text
http://127.0.0.1:9741/recovery
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
cargo test --all-features
```

Le release ufficiali vengono compilate da GitHub Actions per tutti i target supportati.

## Changelog e release

- Changelog completo: [CHANGELOG.md](CHANGELOG.md)
- Binari più recenti e release notes: [GitHub Releases](https://github.com/IntelligenzaArtificiale/G-Type/releases/latest)

## Screenshot

Il README usa per ora un visual tecnico nativo del repository invece di screenshot simulati. Gli screenshot reali della dashboard è meglio catturarli da una build realmente in esecuzione, così la documentazione non mostra mai un mockup diverso dall'interfaccia effettiva.

Gli screenshot possono essere aggiunti in `docs/assets/`. Prima di inserirli nel repository vanno rimossi API key, cronologia privata, email, titoli di finestre sensibili e snippet personali.

## Licenza

MIT. Vedi [LICENSE](LICENSE).

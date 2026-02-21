# G-Type

<p align="center">
  <a href="README.md">English</a> •
  <a href="README.it.md">Italiano</a> •
  <a href="README.es.md">Español</a> •
  <a href="README.pt-BR.md"><b>Português (BR)</b></a> •
  <a href="README.hi.md">हिन्दी</a>
</p>

> 🔄 Traduzido de [`README.md`](README.md) — última sincronização: commit `de04abd` (21 fev 2026)

**Daemon global de ditado por voz.** Segure uma tecla em qualquer lugar do sistema, fale, solte — suas palavras aparecem como texto digitado.

A entrada por voz é [**~3× mais rápida**](BENCHMARK.md) que digitar em cenários de entrada de texto ([Stanford/UW/Baidu, 2016](https://news.stanford.edu/stories/2016/08/stanford-study-speech-recognition-faster-texting)). G-Type remove o atrito: uma tecla, zero interface, funciona em qualquer app.

Baseado na Google Gemini REST API. Binário estático único. ~5 MB.

---

## Como funciona

1. **Idle:** O daemon aguarda sua hotkey. Uso mínimo de recursos.
2. **Gravação:** O microfone captura áudio → converte para PCM mono 16kHz → armazena em memória.
3. **Processamento:** Ao soltar a tecla, o áudio é codificado como WAV, enviado à API REST Gemini, transcrição retornada.
4. **Injeção:** O texto é digitado via emulação de teclado. Fallback para clipboard para textos >500 caracteres.

## Instalação

### Instalação rápida (Linux e macOS)

```bash
curl -sSf https://raw.githubusercontent.com/IntelligenzaArtificiale/g-type/main/install.sh | bash
```

### Instalação rápida (Windows)

Abra o PowerShell e execute:

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/g-type/main/install.ps1 | iex
```

Ambos os instaladores automaticamente:
- Detectam seu SO e arquitetura
- Instalam dependências do sistema necessárias (Linux)
- Baixam o último binário pré-compilado
- Adicionam ao PATH
- Executam o assistente de configuração interativo no primeiro uso

### Binários pré-compilados

Baixe em [Releases](https://github.com/IntelligenzaArtificiale/g-type/releases).

### Do código-fonte (todas as plataformas)

```bash
# Pré-requisitos: toolchain Rust + bibliotecas de áudio/input do sistema
# Linux: sudo apt install libasound2-dev libx11-dev libxtst-dev libxdo-dev libevdev-dev
cargo install --path .
```

## Primeiro uso

Na primeira execução, G-Type inicia um assistente de configuração interativo:

```
╔══════════════════════════════════════════════╗
║       G-Type — Configuração Inicial          ║
╚══════════════════════════════════════════════╝

  G-Type precisa de uma API key do Google Gemini.
  Obtenha uma grátis em: https://aistudio.google.com/apikey

? 🔑 Gemini API Key: ****************************************
⠋ Verificando API key...
✔ API key válida!

? 🤖 Selecionar Modelo Gemini:
  > models/gemini-2.0-flash
    ...

? 🌍 Idioma de transcrição:
  > Auto-detect  (auto)
    Português  (pt)
    English  (en)
    ...

? 🔊 Habilitar feedback sonoro?
  > Sim — beeps ao iniciar/parar gravação
    Não — modo silencioso
```

Execute novamente quando quiser com `g-type setup`.

## Uso

```bash
g-type                # Iniciar o daemon (setup automático no primeiro uso)
g-type setup          # Reexecutar o assistente
g-type set-key KEY    # Atualizar a API key
g-type config         # Mostrar caminho do arquivo de configuração
g-type test-audio     # Testar microfone (3 segundos)
g-type list-devices   # Listar dispositivos de áudio
```

Em **qualquer** aplicação:
1. Segure sua hotkey (padrão: `CTRL+SHIFT+ESPAÇO`) e fale
2. Solte a tecla
3. O texto aparece na posição do cursor

## Configuração

| Chave            | Padrão                    | Descrição                      |
|------------------|---------------------------|--------------------------------|
| `api_key`        | —                         | API key do Google Gemini (obrigatória) |
| `model`          | `models/gemini-2.0-flash` | Identificador do modelo Gemini |
| `hotkey`         | `ctrl+shift+space`        | Combinação de teclas           |
| `language`       | `auto`                    | Idioma de transcrição          |
| `sound_enabled`  | `true`                    | Beeps ao iniciar/parar         |
| `timeout_secs`   | `10`                      | Timeout de requisição HTTP (seg) |

## Requisitos

- API key do Google Gemini ([obtenha uma grátis](https://aistudio.google.com/apikey))
- Microfone funcionando
- **Linux:** ALSA, X11, XTest libs (`libasound2-dev libx11-dev libxtst-dev libxdo-dev libevdev-dev`)
- **macOS:** Permissões de acessibilidade para injeção de teclado
- **Windows:** Sem requisitos adicionais

## Contribuir

Veja [CONTRIBUTING.md](CONTRIBUTING.md) (em inglês).

## Segurança

Veja [SECURITY.md](SECURITY.md) (em inglês).

## Licença

[MIT](LICENSE)

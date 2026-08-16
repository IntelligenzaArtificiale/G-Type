# G-Type

<p align="center">
  <strong>Entrada de voz local, contextual e global com Google Gemini.</strong>
</p>

<p align="center">
  <a href="https://github.com/IntelligenzaArtificiale/G-Type/releases/latest"><img alt="Última versão" src="https://img.shields.io/github/v/release/IntelligenzaArtificiale/G-Type?display_name=tag&sort=semver"></a>
  <a href="https://github.com/IntelligenzaArtificiale/G-Type/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/IntelligenzaArtificiale/G-Type/ci.yml?branch=main&label=CI"></a>
  <a href="LICENSE"><img alt="Licença MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
</p>

<p align="center">
  <a href="README.md">English</a> •
  <a href="README.it.md">Italiano</a> •
  <a href="README.es.md">Español</a> •
  <a href="README.pt-BR.md"><b>Português (BR)</b></a> •
  <a href="README.hi.md">हिन्दी</a>
</p>

> 🔄 Sincronizado com `README.md` para **G-Type v1.5.0**.

G-Type roda em segundo plano, grava somente quando você o aciona, usa a sua própria chave da API Google Gemini e insere o resultado no aplicativo ativo. **v1.5.0** adiciona Context Awareness, Modos, associações aplicativo→Modo, snippets de voz, Hands-Free e Voice Edit sem exigir conta G-Type, backend hospedado ou banco de dados cloud próprio.

<p align="center">
  <img src="docs/assets/g-type-v1.5-flow.svg" alt="Fluxo do G-Type v1.5" width="100%">
</p>

## Início rápido

### 1. Instalar

Linux e macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.sh | bash
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.ps1 | iex
```

### 2. Primeiro setup

```text
http://127.0.0.1:9741/setup
```

Adicione sua Gemini API key, escolha um modelo compatível e configure a hotkey push-to-talk inicial.

### 3. Começar a ditar

```bash
g-type
```

Dashboard local:

```text
http://127.0.0.1:9741/
```

### 4. Atualizar depois

```bash
g-type upgrade
g-type version
```

## Principais recursos

- **Ditado push-to-talk global** com hotkeys configuráveis.
- **Context Awareness**: detecta de forma best-effort o aplicativo em primeiro plano no início da gravação e usa esse contexto para melhorar a compreensão.
- **Modos**: cada Modo pode ter hotkey, modelo Gemini, timeout e instruções próprias.
- **Associações aplicativo → Modo**: um contexto já observado pode ser ligado a um Modo. Uma hotkey explícita de um Modo não padrão sempre tem prioridade.
- **Snippets de voz**: mapeie um gatilho falado para texto exato, URL, email, números ou assinaturas.
- **Backtrack**: lida com correções faladas como “às quatro, na verdade às cinco” preservando a versão final corrigida.
- **Hands-Free**: pressione uma vez para iniciar e outra para parar. Padrão: `Ctrl+Shift+H`.
- **Voice Edit**: selecione um texto, mantenha a hotkey pressionada, dite uma instrução e solte. Padrão: `Ctrl+Shift+E`.
- **Histórico, estatísticas e custos locais** com Modo, aplicativo e tipo de operação.
- **Recovery local**: WAVs concluídos são salvos antes da requisição de rede, evitando perda de áudio em falhas do Gemini ou da rede.
- **Fallback Gemini** em erros transitórios.
- **Verificação de atualizações em segundo plano** e self-update com rollback.
- **Inicialização automática opcional** pelo dashboard.

## Compatibilidade

| Plataforma | Arquitetura |
|---|---|
| Linux | x86_64 |
| Windows | x86_64 |
| macOS | Intel x86_64 |
| macOS | Apple Silicon arm64 |

A detecção de contexto é best-effort. No Linux, G-Type aproveita as informações disponíveis em X11/XWayland; um compositor Wayland nativo pode não expor o aplicativo ativo. Nesse caso, o G-Type continua funcionando normalmente sem contexto.

Os binários oficiais ficam em [GitHub Releases](https://github.com/IntelligenzaArtificiale/G-Type/releases).

## Uso diário

Controles padrão:

```text
Ctrl+Shift+Space   Modo padrão push-to-talk
Ctrl+Shift+H       Hands-Free: iniciar / parar
Ctrl+Shift+E       Voice Edit: manter enquanto fala
```

Todas as hotkeys podem ser alteradas no dashboard. O G-Type bloqueia colisões entre hotkeys de Modos, Hands-Free e Voice Edit.

Se estiver rodando em primeiro plano, pare com `Ctrl+C` e inicie novamente com `g-type`.

## Modos e associações de aplicativos

Os **Modos** substituem na interface a antiga separação entre Profiles/Templates, mantendo a configuração simples e retrocompatível.

Um Modo pode definir:

- hotkey global;
- modelo Gemini;
- timeout da requisição;
- instruções personalizadas.

Para associar um aplicativo a um Modo:

1. Abra o aplicativo.
2. Faça pelo menos um ditado normal dentro dele.
3. Abra **Settings → Applications**.
4. Associe o contexto observado a um Modo.

Resolução:

```text
Hotkey explícita de Modo não padrão → esse Modo sempre vence
Modo padrão / Hands-Free            → binding do app, se existir
Sem binding                         → Modo padrão
```

Não há classificador de IA para adivinhar automaticamente o Modo.

## Snippets de voz

Em **Settings → Snippets**, você pode criar entradas como:

```text
Gatilho: link calendário
Valor:   https://example.com/calendario
```

Snippets ativados são fornecidos ao Gemini como contexto e, quando seguro, o G-Type também aplica substituição determinística pós-transcrição. Limites: até 100 snippets, 100 caracteres por gatilho e 4.000 por valor.

## Hands-Free

Pressione a hotkey Hands-Free uma vez para iniciar a gravação e novamente para finalizar. A gravação usa a mesma pipeline de Recovery, fallback, histórico e rastreamento de custos do ditado padrão.

## Voice Edit

1. Selecione um texto editável.
2. Mantenha a hotkey Voice Edit pressionada.
3. Diga uma instrução, por exemplo `deixe mais curto e profissional`.
4. Solte a hotkey.

O G-Type captura a seleção depois que a combinação é liberada, envia texto selecionado + instrução falada ao Gemini em uma única operação e substitui a seleção pelo resultado.

Se o foco mudar para outro aplicativo antes da inserção final, o resultado permanece no Histórico, mas não é inserido na janela errada.

## Recovery

Antes de cada requisição de rede, o G-Type salva localmente um WAV temporário e os metadados necessários. Se Gemini, rede ou pós-processamento falharem, o item continua disponível em:

```text
http://127.0.0.1:9741/recovery
```

Recovery mantém Modo, contexto do aplicativo e tipo de operação. Em Voice Edit, também conserva o texto fonte selecionado.

**Não apague manualmente a pasta Recovery se ela ainda contiver gravações importantes.**

## Dashboard

- **History** — transcrições recentes com busca, aplicativo, Modo, operação, duração, modelo e custo.
- **Statistics** — uso, palavras, áudio, tempo estimado economizado, modelos, tokens e custos.
- **Settings → General** — idioma, moeda, microfone, Modo padrão, Hands-Free, Voice Edit, sons e tray.
- **Settings → Modes** — gerenciamento de Modos e presets.
- **Settings → Applications** — contextos observados e bindings.
- **Settings → Snippets** — editor de snippets.
- **Settings → API** — Gemini API key.
- **Settings → System** — auto-start, atualizações e informações de runtime.

## Atualizações

```bash
g-type upgrade
g-type version
```

Se o G-Type estiver rodando em primeiro plano: `Ctrl+C`, `g-type upgrade`, `g-type version` e depois `g-type`.

## Comandos úteis

```text
g-type                 Iniciar o G-Type
g-type setup           Abrir o setup web
g-type stats           Mostrar estatísticas e custos
g-type upgrade         Atualizar para a última release
g-type version         Mostrar a versão instalada
g-type config          Mostrar o caminho da configuração
g-type set-key <KEY>   Alterar a Gemini API key
g-type test-audio      Testar o microfone
g-type list-devices    Listar dispositivos de entrada
g-type help            Mostrar a ajuda CLI
```

## Dados e privacidade

- O dashboard escuta somente em `127.0.0.1`.
- Configuração, histórico e Recovery ficam em diretórios locais do usuário.
- A Gemini API key não é devolvida em texto claro pela API do dashboard.
- O áudio é enviado à Gemini API configurada para transcrição ou edição.
- O contexto do aplicativo pode ser incluído no prompt e salvo no Histórico local quando disponível com segurança.
- O G-Type não possui sistema próprio de contas cloud nem banco de dados remoto.

## Compilar a partir do código-fonte

```bash
git clone https://github.com/IntelligenzaArtificiale/G-Type.git
cd G-Type
cargo build --release
```

Antes de contribuir:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Changelog e releases

- [CHANGELOG.md](CHANGELOG.md)
- [GitHub Releases](https://github.com/IntelligenzaArtificiale/G-Type/releases/latest)

## Capturas de tela

Por enquanto o README usa um visual técnico real do repositório, e não mockups da interface. Capturas do dashboard devem ser feitas a partir de uma build real em execução e sem API keys, histórico privado, emails, títulos de janelas sensíveis ou snippets pessoais.

## Licença

MIT. Consulte [LICENSE](LICENSE).

# G-Type

<p align="center">
  <strong>Entrada de voz local, contextual y global impulsada por Google Gemini.</strong>
</p>

<p align="center">
  <a href="https://github.com/IntelligenzaArtificiale/G-Type/releases/latest"><img alt="Última versión" src="https://img.shields.io/github/v/release/IntelligenzaArtificiale/G-Type?display_name=tag&sort=semver"></a>
  <a href="https://github.com/IntelligenzaArtificiale/G-Type/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/IntelligenzaArtificiale/G-Type/ci.yml?branch=main&label=CI"></a>
  <a href="LICENSE"><img alt="Licencia MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
</p>

<p align="center">
  <a href="README.md">English</a> •
  <a href="README.it.md">Italiano</a> •
  <a href="README.es.md"><b>Español</b></a> •
  <a href="README.pt-BR.md">Português (BR)</a> •
  <a href="README.hi.md">हिन्दी</a>
</p>

> 🔄 Sincronizado con `README.md` para **G-Type v1.5.0**.

G-Type se ejecuta en segundo plano, graba únicamente cuando lo invocas, utiliza tu propia API key de Google Gemini e inserta el resultado en la aplicación activa. **v1.5.0** añade Context Awareness, Modos, asociaciones aplicación→Modo, snippets de voz, Hands-Free y Voice Edit sin requerir una cuenta G-Type, backend alojado ni base de datos cloud propia.

<p align="center">
  <img src="docs/assets/g-type-v1.5-flow.svg" alt="Flujo de G-Type v1.5" width="100%">
</p>

## Inicio rápido

### 1. Instalar

Linux y macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.sh | bash
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.ps1 | iex
```

### 2. Primer setup

```text
http://127.0.0.1:9741/setup
```

Añade tu Gemini API key, selecciona un modelo compatible y configura la hotkey push-to-talk inicial.

### 3. Empezar a dictar

```bash
g-type
```

Dashboard local:

```text
http://127.0.0.1:9741/
```

### 4. Actualizar

```bash
g-type upgrade
g-type version
```

## Funciones principales

- **Dictado push-to-talk global** con hotkeys configurables.
- **Context Awareness**: detecta de forma best-effort la aplicación en primer plano al iniciar la grabación y usa ese contexto para mejorar la comprensión.
- **Modos**: cada Modo puede tener hotkey, modelo Gemini, timeout e instrucciones propias.
- **Asociaciones aplicación → Modo**: un contexto ya observado puede vincularse a un Modo. Una hotkey explícita de un Modo no predeterminado siempre tiene prioridad.
- **Snippets de voz**: convierte un trigger hablado en texto exacto, URL, email, números o firmas.
- **Backtrack**: gestiona correcciones explícitas como “a las cuatro, mejor a las cinco” conservando la versión final corregida.
- **Hands-Free**: pulsa una vez para iniciar y otra para terminar. Predeterminado: `Ctrl+Shift+H`.
- **Voice Edit**: selecciona texto, mantén la hotkey, dicta una instrucción de edición y suelta. Predeterminado: `Ctrl+Shift+E`.
- **Historial, estadísticas y costes locales** con Modo, aplicación y tipo de operación.
- **Recovery local**: los WAV completados se guardan antes de la petición de red para no perder el audio si Gemini o la red fallan.
- **Fallback de Gemini** ante errores transitorios.
- **Comprobación de actualizaciones en segundo plano** y self-update con rollback.
- **Inicio automático opcional** desde el dashboard.

## Compatibilidad

| Plataforma | Arquitectura |
|---|---|
| Linux | x86_64 |
| Windows | x86_64 |
| macOS | Intel x86_64 |
| macOS | Apple Silicon arm64 |

El contexto es best-effort. En Linux se aprovecha la información disponible en X11/XWayland; un compositor Wayland nativo puede no exponer la aplicación activa. En ese caso G-Type sigue funcionando sin contexto.

Los binarios oficiales están disponibles en [GitHub Releases](https://github.com/IntelligenzaArtificiale/G-Type/releases).

## Uso diario

Controles predeterminados:

```text
Ctrl+Shift+Space   Modo estándar push-to-talk
Ctrl+Shift+H       Hands-Free: iniciar / terminar
Ctrl+Shift+E       Voice Edit: mantener mientras hablas
```

Las hotkeys se pueden cambiar desde el dashboard y G-Type evita colisiones entre Modos, Hands-Free y Voice Edit.

Si G-Type se ejecuta en primer plano, puedes detenerlo con `Ctrl+C` y volver a iniciarlo con `g-type`.

## Modos y asociaciones de aplicaciones

Los **Modos** sustituyen en la interfaz la antigua separación entre Profiles/Templates manteniendo una configuración simple y retrocompatible.

Un Modo puede definir:

- hotkey global;
- modelo Gemini;
- timeout de la petición;
- instrucciones personalizadas.

Para asociar una aplicación a un Modo:

1. Abre la aplicación.
2. Haz al menos un dictado dentro de ella.
3. Abre **Settings → Applications**.
4. Vincula el contexto observado a un Modo.

Resolución:

```text
Hotkey explícita de Modo no predeterminado → ese Modo siempre gana
Modo predeterminado / Hands-Free           → binding de app si existe
Sin binding                                → Modo predeterminado
```

No existe un clasificador AI que adivine automáticamente el Modo.

## Snippets de voz

Desde **Settings → Snippets** puedes crear entradas como:

```text
Trigger: enlace calendario
Valor:   https://example.com/calendario
```

Los snippets habilitados se pasan a Gemini como contexto y, cuando es seguro, G-Type aplica también una sustitución determinista post-transcripción. Límites: hasta 100 snippets, 100 caracteres por trigger y 4.000 por valor.

## Hands-Free

Pulsa la hotkey Hands-Free una vez para empezar a grabar y una segunda vez para detener. La grabación utiliza la misma pipeline de Recovery, fallback, historial y seguimiento de costes que el dictado normal.

## Voice Edit

1. Selecciona texto editable.
2. Mantén pulsada la hotkey Voice Edit.
3. Di una instrucción, por ejemplo `hazlo más corto y profesional`.
4. Suelta la hotkey.

G-Type captura la selección después de soltar la combinación, envía texto seleccionado + instrucción de voz a Gemini en una sola operación y reemplaza la selección con el resultado.

Si el foco cambia a otra aplicación antes de la inserción final, el resultado se conserva en el Historial pero no se inserta en la ventana equivocada.

## Recovery

Antes de cada petición de red, G-Type guarda localmente un WAV temporal y los metadatos necesarios. Si falla Gemini, la red o el post-procesado, el elemento sigue disponible en:

```text
http://127.0.0.1:9741/recovery
```

Recovery conserva el Modo, contexto de aplicación y tipo de operación. En Voice Edit conserva también el texto fuente seleccionado.

**No borres manualmente la carpeta Recovery si contiene grabaciones que todavía necesitas.**

## Dashboard

- **History** — transcripciones recientes con búsqueda, aplicación, Modo, operación, duración, modelo y coste.
- **Statistics** — uso, palabras, audio, tiempo estimado ahorrado, modelos, tokens y costes.
- **Settings → General** — idioma, moneda, micrófono, Modo predeterminado, Hands-Free, Voice Edit, sonidos y tray.
- **Settings → Modes** — gestión de Modos y presets.
- **Settings → Applications** — contextos observados y bindings.
- **Settings → Snippets** — editor de snippets.
- **Settings → API** — Gemini API key.
- **Settings → System** — autoarranque, actualizaciones e información runtime.

## Actualizaciones

```bash
g-type upgrade
g-type version
```

Si G-Type está ejecutándose en primer plano: `Ctrl+C`, `g-type upgrade`, `g-type version` y después `g-type`.

## Comandos útiles

```text
g-type                 Iniciar G-Type
g-type setup           Abrir el setup web
g-type stats           Mostrar estadísticas y costes
g-type upgrade         Actualizar a la última release
g-type version         Mostrar la versión instalada
g-type config          Mostrar la ruta de configuración
g-type set-key <KEY>   Cambiar la Gemini API key
g-type test-audio      Probar el micrófono
g-type list-devices    Listar dispositivos de entrada
g-type help            Mostrar la ayuda CLI
```

## Datos y privacidad

- El dashboard escucha únicamente en `127.0.0.1`.
- Configuración, historial y Recovery permanecen en directorios locales del usuario.
- La Gemini API key no se devuelve en texto claro desde la API del dashboard.
- El audio se envía a la Gemini API configurada para transcripción o edición.
- El contexto de aplicación puede incluirse en el prompt y guardarse en el Historial local cuando está disponible de forma segura.
- G-Type no tiene un sistema propio de cuentas cloud ni base de datos remota.

## Compilar desde el código fuente

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

## Changelog y releases

- [CHANGELOG.md](CHANGELOG.md)
- [GitHub Releases](https://github.com/IntelligenzaArtificiale/G-Type/releases/latest)

## Capturas de pantalla

El README usa de momento un visual técnico real del repositorio, no mockups de la interfaz. Las capturas del dashboard deberían tomarse desde una build real en ejecución y sin API keys, historial privado, emails, títulos sensibles o snippets personales.

## Licencia

MIT. Consulta [LICENSE](LICENSE).

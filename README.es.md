# G-Type

<p align="center">
  <a href="README.md">English</a> •
  <a href="README.it.md">Italiano</a> •
  <a href="README.es.md"><b>Español</b></a> •
  <a href="README.pt-BR.md">Português (BR)</a> •
  <a href="README.hi.md">हिन्दी</a>
</p>

> 🔄 Traducido de [`README.md`](README.md) — última sincronización: commit `de04abd` (21 feb 2026)

**Daemon global de dictado por voz.** Mantén pulsada una tecla en cualquier parte del sistema, habla, suelta — tus palabras aparecen como texto escrito.

La entrada por voz es [**~3× más rápida**](BENCHMARK.md) que escribir en escenarios de entrada de texto ([Stanford/UW/Baidu, 2016](https://news.stanford.edu/stories/2016/08/stanford-study-speech-recognition-faster-texting)). G-Type elimina la fricción: una sola tecla, cero interfaz, funciona en cualquier app.

Basado en Google Gemini REST API. Binario estático único. ~5 MB.

---

## Cómo funciona

1. **Idle:** El daemon espera tu hotkey. Uso mínimo de recursos.
2. **Grabación:** El micrófono captura audio → convierte a PCM mono 16kHz → almacena en memoria.
3. **Procesamiento:** Al soltar la tecla, el audio se codifica como WAV, se envía a la API REST Gemini, se devuelve la transcripción.
4. **Inyección:** El texto se escribe mediante emulación de teclado. Usa portapapeles para textos >500 caracteres.

## Instalación

### Instalación rápida (Linux y macOS)

```bash
curl -sSf https://raw.githubusercontent.com/IntelligenzaArtificiale/g-type/main/install.sh | bash
```

### Instalación rápida (Windows)

Abre PowerShell y ejecuta:

```powershell
irm https://raw.githubusercontent.com/IntelligenzaArtificiale/g-type/main/install.ps1 | iex
```

Ambos instaladores automáticamente:
- Detectan tu SO y arquitectura
- Instalan dependencias del sistema necesarias (Linux)
- Descargan el último binario pre-compilado
- Lo agregan al PATH
- Ejecutan el asistente de configuración interactivo en el primer uso

### Binarios pre-compilados

Descarga desde [Releases](https://github.com/IntelligenzaArtificiale/g-type/releases).

### Desde el código fuente (todas las plataformas)

```bash
# Prerrequisitos: toolchain de Rust + bibliotecas de audio/input del sistema
# Linux: sudo apt install libasound2-dev libx11-dev libxtst-dev libxdo-dev libevdev-dev
cargo install --path .
```

## Primer uso

En el primer inicio, G-Type ejecuta un asistente de configuración interactivo:

```
╔══════════════════════════════════════════════╗
║       G-Type — Configuración Inicial         ║
╚══════════════════════════════════════════════╝

  G-Type necesita una API key de Google Gemini.
  Obtén una gratis en: https://aistudio.google.com/apikey

? 🔑 Gemini API Key: ****************************************
⠋ Verificando API key...
✔ ¡API key válida!

? 🤖 Seleccionar Modelo Gemini:
  > models/gemini-2.0-flash
    ...

? 🌍 Idioma de transcripción:
  > Auto-detect  (auto)
    Italiano  (it)
    English  (en)
    Español  (es)
    ...

? 🔊 ¿Habilitar retroalimentación sonora?
  > Sí — beeps al iniciar/parar grabación
    No — modo silencioso
```

Vuelve a ejecutar cuando quieras con `g-type setup`.

## Uso

```bash
g-type                # Iniciar el daemon (configuración automática en primer uso)
g-type setup          # Volver a ejecutar el asistente
g-type set-key KEY    # Actualizar la API key
g-type config         # Mostrar ruta del archivo de configuración
g-type test-audio     # Probar micrófono (3 segundos)
g-type list-devices   # Listar dispositivos de audio
```

En **cualquier** aplicación:
1. Mantén pulsado tu hotkey (por defecto: `CTRL+SHIFT+ESPACIO`) y habla
2. Suelta la tecla
3. El texto aparece en la posición del cursor

## Configuración

| Clave            | Por defecto               | Descripción                    |
|------------------|---------------------------|--------------------------------|
| `api_key`        | —                         | API key de Google Gemini (obligatoria) |
| `model`          | `models/gemini-2.0-flash` | Identificador del modelo Gemini |
| `hotkey`         | `ctrl+shift+space`        | Combinación de teclas          |
| `language`       | `auto`                    | Idioma de transcripción        |
| `sound_enabled`  | `true`                    | Beeps al iniciar/parar         |
| `timeout_secs`   | `10`                      | Timeout de petición HTTP (seg) |

## Requisitos

- API key de Google Gemini ([obtén una gratis](https://aistudio.google.com/apikey))
- Micrófono funcionando
- **Linux:** ALSA, X11, XTest libs (`libasound2-dev libx11-dev libxtst-dev libxdo-dev libevdev-dev`)
- **macOS:** Permisos de accesibilidad para inyección de teclado
- **Windows:** Sin requisitos adicionales

## Contribuir

Ver [CONTRIBUTING.md](CONTRIBUTING.md) (en inglés).

## Seguridad

Ver [SECURITY.md](SECURITY.md) (en inglés).

## Licencia

[MIT](LICENSE)

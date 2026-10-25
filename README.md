# AIMeetings

Graba clases, llamadas y reuniones; obtén la transcripción con **quién dijo qué** y deja que la IA
escriba resúmenes, apuntes de estudio, conceptos clave, preguntas de repaso o actas. La
transcripción ocurre **en tu equipo**.

Plataformas: **macOS (Apple Silicon)**, **Windows x64** y **Windows ARM64**.

## Qué hace

- **Grabar**: llamada (micrófono + audio del equipo: Teams, Meet, Zoom…) o presencial (micrófono),
  con texto en vivo, pausa y detección automática de llamadas.
- **Transcribir en local**: Whisper (whisper.cpp) o Parakeet/Canary (sherpa-onnx), o un Whisper en
  la nube compatible con OpenAI si lo prefieres.
- **Saber quién habla**: separación de hablantes (CAM++ + pyannote) y voces guardadas que se
  reconocen en las siguientes grabaciones.
- **Resumir con IA**: con una herramienta CLI que ya tengas (Claude Code, Codex, Gemini, OpenCode,
  Cline, Grok), con **cualquier CLI propio** (plantillas para Ollama, llm, aichat) o con un **modelo
  local** (llama.cpp integrado, Ollama o servidor compatible con OpenAI). Las grabaciones o
  carpetas marcadas como privadas siempre usan el modelo local.
- **Biblioteca**: carpetas, búsqueda en títulos, notas y transcripciones, notas propias en Markdown
  por grabación, exportar transcripción (md/txt/srt) y audio.
- **IA y modelos**: cada modelo muestra lo que ocupa, la RAM/VRAM que necesita, la velocidad
  estimada (o medida) en tu equipo y cuál se recomienda para tu máquina.
- **Actualizaciones** desde la propia app (firmadas).

## Desarrollo

```bash
pnpm install
pnpm tauri dev
```

Tests: `pnpm test` y `cd src-tauri && cargo test --lib`.

Los motores (whisper.cpp, sherpa-onnx) y los modelos se descargan desde la app, verificados por
SHA-256. `llama-server` se empaqueta con `pnpm llama:runtime`.

## Publicar una versión

1. Una vez: crea el repositorio `sorodriguezz/AIMeetings` en GitHub y añade el secreto
   `TAURI_SIGNING_PRIVATE_KEY` con el contenido de `~/.tauri/aimeetings.key` (la clave no tiene
   contraseña; deja `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` vacío). Sin esa clave las apps instaladas
   no podrán actualizarse.
2. `python3 scripts/bump_version.py 0.2.0`
3. `git commit -am "Release v0.2.0" && git tag v0.2.0 && git push --follow-tags`

El workflow `release` construye los tres instaladores en un borrador, reconstruye `latest.json` y
publica la versión; las apps instaladas la detectan solas.

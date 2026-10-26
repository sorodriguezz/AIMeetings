# Desarrollo

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

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
3. `git add -A`
4. `git commit -m "Release v0.2.0"`
5. `git push origin main`


Al hacer push a `main`, el workflow comprueba que la versión coincida en la app, Tauri, el motor
(si existe) y los archivos de bloqueo. Si `vX.Y.Z` todavía no existe, crea el tag anotado en ese
mismo commit y construye su release. Un push sin cambio de versión no vuelve a publicar.
Los tags y las releases pertenecen únicamente a este repositorio.

La compilación y publicación continúan en el mismo workflow: el tag creado con `GITHUB_TOKEN`
no necesita disparar otro workflow ni requiere un token personal. La release se publica sólo
cuando están disponibles los instaladores y las actualizaciones firmadas de las tres plataformas.

Si falla una publicación, usa **Actions → release → Re-run all jobs** en la ejecución original.
También puedes usar **Run workflow** seleccionando el tag de esa versión. Se reutiliza el borrador;
una release ya publicada no se sobrescribe y un tag existente nunca se mueve a otro commit.
Si necesitas corregir código, incrementa la versión y sube un commit nuevo.

Comprobar la automatización localmente:

```bash
python3 scripts/bump_version.py --check
python3 -m unittest discover -s scripts -p test_release.py -v
```

El workflow `release` construye los tres instaladores en un borrador, reconstruye `latest.json` y
publica la versión; las apps instaladas la detectan solas.

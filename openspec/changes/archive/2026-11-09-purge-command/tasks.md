# Tasks

## 1. Parsers (RED → GREEN)

- [x] 1.1 Unit test: `parse_secret_names` — sin `,`, con `,type=env,target=VAR`, varias
      líneas en un fichero, ausente → vacío, comentario inline.
- [x] 1.2 Implementar `parse_secret_names` (multi-línea; nombre = texto antes de la primera `,`).
- [x] 1.3 Unit tests: `parse_volume_binds` — `%h`/`%t` resueltos, socket `%t/podman/podman.sock`
      ignorado, ruta fuera de `$HOME` ignorada, volumen nombrado (`<x>.volume`) excluido,
      detección de `User=` (uid mapeado).
- [x] 1.4 Implementar `parse_volume_binds` y la resolución de sufijos systemd.

## 2. Guarda cross-stack (RED → GREEN)

- [x] 2.1 Unit/integration test: `scan_stack_resources` recoge secretos e imágenes por stack.
- [x] 2.2 Implementar `scan_stack_resources` reutilizando `stack_quadlet_dirs` y `QUADLET_EXTENSIONS`.
- [x] 2.3 Test: secreto compartido → `skip:`; con `--force` → se borra. Igual para imagen.

## 3. Comando `purge` (RED → GREEN)

- [x] 3.1 Añadir `Commands::Purge { volumes, secrets, images, data, force, yes }` y su dispatch.
- [x] 3.2 `async fn purge` base: `uninstall` + `clean_volumes` (sin flags = base).
- [x] 3.3 `--secrets`: borrar secretos declarados, saltando compartidos salvo `--force`;
      ausentes → `warn!` y continuar.
- [x] 3.4 `--images`: borrar imágenes declaradas con la misma guarda.
- [x] 3.5 `--data`: borrar binds elegibles (`rm -rf` / `podman unshare rm -rf` según uid mapeado).
- [x] 3.6 Confirmación única (`confirm_purge`) listando recursos y rutas; `--yes` la omite.
- [x] 3.7 `--dry-run`: imprimir plan completo, no ejecutar.

## 4. Integration tests (`tests/cli.rs`)

- [x] 4.1 `purge` sin flags = `uninstall` + `clean-volumes` y nada más.
- [x] 4.2 `--secrets`/`--images` con shims: borra declarados, omite compartidos, `--force` borra.
- [x] 4.3 `--data` con shims: borra binds bajo `$HOME`, ignora socket/`%t`/sistema,
      usa `podman unshare` con uid mapeado.
- [x] 4.4 `--dry-run` no invoca podman; `--yes` no pide confirmación; recurso ausente no aborta.

## 5. Verificación y cierre

- [x] 5.1 `cargo test` (unit + integración) verde.
- [x] 5.2 `cargo fmt --all -- --check` y `cargo clippy --all-targets --all-features -- -D warnings`.
- [x] 5.3 `openspec validate purge-command --strict` verde.
- [x] 5.4 Auditoría `@rust-reviewer`; corregir hallazgos.
- [x] 5.5 `docs/purge.md`; actualizar `README.md`; PR a `develop` (gitmoji); `openspec archive`.

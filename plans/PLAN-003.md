# PLAN-003 — Comando `qctl purge`

- **Issue:** #29 (feat: nuevo comando `qctl purge`) — asignado a atareao
- **Cambio OpenSpec:** `openspec/changes/purge-command/` (propuesta, design, spec delta `specs/purge/spec.md`, tasks) — validado con `openspec validate purge-command --strict`
- **Estado:** COMPLETADO — auditoría @rust-reviewer APTO; pendiente PR y archive
- **Rama:** `feature/purge-command` desde `develop`

## Objetivo

Nuevo subcomando `qctl purge [--volumes] [--secrets] [--images] [--data] [--force] [--yes]` que desinstala un stack y, con opt-in explícito por categoría, borra sus secretos, imágenes y datos de bind mounts host-side, respetando los recursos compartidos entre stacks.

## Alcance

- Base (sin flags): `uninstall` + `clean-volumes`.
- `--secrets`: `podman secret rm` de cada `Secret=<name>[,opts]` de los `.container`.
- `--images`: `podman rmi` de cada `Image=`.
- `--data`: borra binds host-side bajo `$HOME` (resuelve `%h`/`%t`), ignora sockets/`%t`/rutas de sistema, usa `podman unshare rm -rf` con uid mapeado.
- Guarda cross-stack: skip de recursos usados por otra stack (salvo `--force`).
- `--dry-run` respetado; `--yes` omite confirmación.

## Fuera de alcance

- No borra quadlets de fuentes ni de `available/`. No gestiona pods ni `.kube`.

## Tareas (TDD, detalle en openspec/changes/purge-command/tasks.md)

1. Parsers: `parse_secret_names` (multi-línea) y `parse_volume_binds` — unit tests RED→GREEN.
2. Guarda cross-stack: `scan_stack_resources` (secretos/imágenes por stack).
3. Comando `purge`: `enum Commands` + dispatch + `async fn purge` (base, secrets, images, data, confirmación única, dry-run).
4. Integration tests en `tests/cli.rs` con shims de podman/systemctl.
5. Verificación: `cargo test`, `cargo fmt --check`, `cargo clippy -D warnings`, `openspec validate --strict`, auditoría `@rust-reviewer`, `docs/purge.md`, PR a `develop`, `openspec archive`.

## Criterios de aceptación

Los 7 del issue #29 (base = uninstall+clean-volumes; secrets/images con guarda; data seguro; dry-run; yes; ausencias no abortan).

## Puntos de integración (src/main.rs)

`enum Commands` (L56), dispatch (L170), `uninstall` (L360), `clean_volumes` (L1063), `confirm_clean_volumes` (L1117), `parse_volume_name` (L1804), `parse_image_name` (L1812), `discover_source_dirs` (L1545), `collect_quadlets` (L1630), `extension_of_path` (L1744), `run_command` (L1901), `scan_stacks_root` (L1249), `stack_quadlet_dirs` (L1311), `current_stack_name` (L1394).
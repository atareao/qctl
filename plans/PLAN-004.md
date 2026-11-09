# PLAN-004 · fix(purge): `--data` resuelve symlinks antes de borrar

- **Issue**: #31 — `fix(purge): --data debe resolver symlinks antes de borrar (escape de $HOME)`
- **Change OpenSpec**: `openspec/changes/fix-purge-data-symlinks` (spec aprobada)
- **Rama**: `feature/fix-purge-data-symlinks` → PR a `develop`
- **Release prevista**: patch (0.7.x)

## Objetivo

Cerrar el escape de `$HOME` en `qctl purge --data`: antes de borrar cada bind, resolver la
ruta real con `fs::canonicalize` y exigir que sea descendiente **estricto** del `$HOME`
canónico. Verificación idéntica para `rm -rf` y `podman unshare rm -rf`.

## Contexto de código (`src/main.rs`)

- `resolve_bind_source` (≈L2322): resuelve sufijo systemd + normaliza léxicamente, sin FS.
- `parse_volume_binds`/`resolve_volume_binds` (≈L2285) → `Vec<VolumeMount>`.
- `remove_binds` (≈L1527): comprueba `metadata().is_dir()` y ejecuta `rm`/`podman unshare rm`.
- `run_command` respeta `ctx.dry_run`.

## Tareas (TDD)

- [ ] 1. RED: unit del helper de canonicalización + integración con symlinks.
- [ ] 2. GREEN: resolver `$HOME` canónico y verificar contención en `remove_binds`.
- [ ] 3. REFACTOR: fmt/clippy, docstrings, `docs/purge.md`.
- [ ] 4. Verificación: `cargo test`, `openspec validate --strict`, auditoría `@rust-reviewer`, PR.

## Criterios de aceptación

1. Bind cuyo path real escapa de `$HOME` (symlink intermedio o hoja) → **no se borra** + `warn!`.
2. Bind real bajo `$HOME` (symlinks internos que no escapan) → se borra con normalidad.
3. `canonicalize` que falla → `warn!` + skip, sin abortar el purge.
4. `--dry-run` sigue sin ejecutar nada.
5. Regresión: los casos del #29 siguen verdes.

## Notas

- Sin dependencias nuevas. Linux-only.
- Barrera léxica de `resolve_bind_source` se conserva como primera barrera.

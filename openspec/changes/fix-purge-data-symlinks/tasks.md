# Tasks

## 1. Tests primero (RED)

- [x] 1.1 Unit: caso de symlink intermedio que escapa `$HOME` → no elegible/skip.
- [x] 1.2 Unit/integración: symlinks internos que **no** escapan `$HOME` → se borran.
- [x] 1.3 Integración (shims `rm`/`podman`): symlink intermedio que escapa → NO se invoca
      `rm -rf`/`podman unshare rm -rf` sobre la ruta, y se emite `warn!`.
- [x] 1.4 Integración: symlink hoja que apunta fuera de `$HOME` → no se sigue (skip/warn).
- [x] 1.5 Regresión: los casos del #29 (rutas reales bajo `$HOME`) siguen funcionando.
- [x] 1.6 Regresión: `--dry-run` sigue sin ejecutar nada.

## 2. Implementación (GREEN)

- [x] 2.1 Calcular `home_canon = fs::canonicalize($HOME)` en el camino de `--data`.
- [x] 2.2 En `remove_binds`: `real = fs::canonicalize(&mount.source)`; si `Err` → `warn!` +
      `skip`; si `real` no es descendiente estricto de `home_canon` → `warn!` nombrando
      ruta declarada y real + `skip`.
- [x] 2.3 Aplicar la verificación tanto a `rm -rf` como a `podman unshare rm -rf`.
- [x] 2.4 Mantener la normalización léxica de `resolve_bind_source` como primera barrera.

## 3. Refactor y documentación

- [x] 3.1 `cargo fmt --all -- --check` y `cargo clippy --all-targets --all-features -- -D warnings` limpios.
- [x] 3.2 Docstrings de `remove_binds` (y helper si procede) explicando la doble barrera.
- [x] 3.3 `docs/purge.md`: nota de seguridad sobre resolución de symlinks y contención.

## 4. Verificación y cierre

- [x] 4.1 `cargo test` (unit + integración) verde.
- [x] 4.2 `openspec validate fix-purge-data-symlinks --strict` verde.
- [x] 4.3 Auditoría `@rust-reviewer`; corregir hallazgos.
- [ ] 4.4 PR a `develop` (gitmoji) referenciando #31.

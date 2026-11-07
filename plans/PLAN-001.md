# PLAN — Migrar a comfy-table 8 (issue #20)

## Objetivo

Resolver el issue #20 de forma definitiva: subir `comfy-table` de "7" a "8" y adaptar el código a la API v8. Deshace el pin provisional de la release 0.5.0 (`e849ca3`) y recupera la intención del PR #16.

## Arquitectura

Migración de dependencia y adaptación de código a la API v8 de `comfy-table`, manteniendo la salida visual byte-idéntica a la versión 7.

## Tareas

### Tarea 1: Migrar a comfy-table 8

**Archivos:**
- Crear: `/data/rust/qctl/openspec/changes/migrate-comfy-table-8/proposal.md`
- Crear: `/data/rust/qctl/openspec/changes/migrate-comfy-table-8/specs/status-output/spec.md`
- Crear: `/data/rust/qctl/openspec/changes/migrate-comfy-table-8/tasks.md`

- [ ] **Paso 1:** Actualizar `Cargo.toml` a `comfy-table = "8"`
      ```toml
      [dependencies]
      comfy-table = "8"
      ```
- [ ] **Paso 2:** Migrar los call sites a `load_style(UTF8_FULL.with_rounded_corners())`
      ```rust
      let table = Table::load_style(presets::UTF8_FULL.with_rounded_corners());
      ```
- [ ] **Paso 3:** Eliminar import de `comfy_table::modifiers`
- [ ] **Paso 4:** Validar compilación con `cargo build` y tests con `cargo test` (49 verdes)
- [ ] **Paso 5:** Formatear y lintear con `cargo fmt --all -- --check` y `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] **Paso 6:** Validar build limpio sin `Cargo.lock` con `rm -rf target Cargo.lock && cargo build && cargo test`
- [ ] **Paso 7:** Archivar change con `openspec archive migrate-comfy-table-8`
- [ ] **Paso 8:** Commits gitmoji convencionales + PR a `development`
- [ ] **Paso 9:** Cerrar issue #20 via PR
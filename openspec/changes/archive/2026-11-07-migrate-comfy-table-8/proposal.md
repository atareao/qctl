# Migrar a comfy-table 8 (issue #20)

## Why

El PR #16 («upgrade crates») subió `comfy-table` de 7 a 8 en `Cargo.toml` sin adaptar el código: `src/main.rs` seguía usando `comfy_table::modifiers::UTF8_ROUND_CORNERS` y `Table::load_preset`, API que la versión 8.0.1 eliminó. Como `Cargo.lock` está gitignored, la CI resuelve dependencias de cero en cada build y un build limpio de `main` no compila (`E0432` unresolved import, `E0599` no method `load_preset`). El fix provisional de la release 0.5.0 (`e849ca3`) fijó `comfy-table = "7"` para que `main` volviera a compilar. Este change decide el estado definitivo que el issue dejaba abierto: **migrar a 8**, recuperando la intención original del PR #16.

## What Changes

- **`Cargo.toml`**: `comfy-table = "7"` → `comfy-table = "8"`, sin comentarios de pin permanente.
- **`src/main.rs`**: los dos call sites de render — tabla humana de `status` en `status()` y tabla de acciones de `menu` en `print_menu_table()` — migran de `load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS)` a `load_style(UTF8_FULL.with_rounded_corners())`. Se elimina el import de `comfy_table::modifiers`; `presets::UTF8_FULL` ya estaba importado. La salida visual SHALL quedar byte-idéntica a la anterior.

## Capabilities

### Modified Capabilities

- `status-output`: el contrato de render de la tabla humana de `status` (y de la tabla de acciones de `menu`) queda anclado a la API v8 — mismos bordes UTF8 con esquinas redondeadas, sin cambio visual.

## Impact

- **Specs**: delta de `status-output`.
- **Código**: `src/main.rs` (import + 2 call sites) y `Cargo.toml` (bump de dependencia).
- **Tests**: los 49 existentes (6 unit + 43 integración) actúan de caracterización; no se añaden tests de comportamiento porque el contrato visual no cambia. Se valida además un build limpio sin `Cargo.lock` (equivalente a CI).
- **Docs**: sin impacto manual en este repo (no hay `CHANGELOG.md` ni `cliff.toml`).
- **CI**: el build sin lock vuelve a compilar y `cargo test` pasa.

## Out of Scope

- Fijar el pin a 7 permanentemente (opción descartada del issue).
- Cualquier rediseño visual de las tablas.
- Otros upgrades de dependencias incluidos en el PR #16 (ese PR se ciñe a `comfy-table`).
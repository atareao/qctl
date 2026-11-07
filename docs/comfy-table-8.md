# Migración a comfy-table 8

## Qué es

Resolución definitiva del issue #20. El PR #16 subió `comfy-table` de 7 a 8 sin adaptar el código, rompiendo el build limpio de `main` (CI sin `Cargo.lock`: `E0432` `unresolved import comfy_table::modifiers`, `E0599` `no method load_preset`). La release 0.5.0 lo parcheó fijando `comfy-table = "7"` (`e849ca3`). Este cambio decide el estado definitivo: migrar a la API v8, no fijar el pin.

## Ficheros implicados

- `Cargo.toml:10` — `comfy-table = "8"` (antes `"7"`) sin comentarios de pin.
- `src/main.rs:11` — import: eliminado `modifiers::UTF8_ROUND_CORNERS`.
- `src/main.rs:739-740` — tabla de `status()`: `load_preset(UTF8_FULL).apply_modifier(UTF8_ROUND_CORNERS)` → `load_style(UTF8_FULL.with_rounded_corners())`.
- `src/main.rs:954-955` — tabla de acciones de `menu()` (`print_menu_table`): misma sustitución.
- `openspec/specs/status-output/spec.md` — nuevo requisito: table rendering SHALL be stable across the comfy-table 8 migration.

## Cómo funciona

`load_preset` y el módulo `modifiers` desaparecieron en 8.0.1. `presets::UTF8_FULL` es ahora un `TableStyle` const con `.with_rounded_corners()`; `Table::load_style(TableStyle)` es el reemplazo de `load_preset`. La salida visual queda byte-idéntica (bordes UTF8 con esquinas redondeadas), verificada por los 67 tests (7 unit + 60 integración) y por la spec `status-output`.

## Variables/config

Ninguna. El comportamiento no cambia; solo la dependencia y su API de render.

## Cómo habilitar/desplegar

Ningún paso especial: el build limpio sin `Cargo.lock` (equivalente CI) compila y pasa los tests. No hay `CHANGELOG.md`/`cliff.toml` en el repo.

## Decisiones

- Migrar a v8 (opción 1 del issue) en lugar de fijar `"7"` permanentemente: recupera la intención del PR #16, evita quedar clavado en una versión antigua y la migración eran 2 call sites.
- `design.md` del change no se creó: artefacto opcional del schema `spec-driven`.

## Follow-ups

- `rust-writer` y `plan-writer` fallaron en la edición de ficheros durante este ciclo (saludo sin editar / alucinación de contenido); se usó `backend-dev` como escritor fiable.
- AGENTS.md actualizado con regla IX (plans/ y docs/) durante este ciclo; `plans/PLAN-001.md` es el plan de esta feature.

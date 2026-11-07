# PLAN-002 — Corregir unidades systemd de quadlets con stems punteados

## Objetivo

Cerrar los hallazgos de la auditoría del change `fix-systemd-unit-dots`: unidades systemd de stems con puntos (`voicebox.prod.container` → `voicebox.prod.service`). Documentar la limitación de round-trip, ampliar el spec, endurecer `recent_logs`, crear los artefactos de proceso y verificar en verde.

## Arquitectura

Dos helpers nuevos en `src/main.rs`: `container_unit(stem)` (siempre añade `.service`) y `strip_quadlet_extension(name)` (recorta solo extensiones de quadlet conocidas). `systemd_unit` (heurística "ya tiene punto") queda eliminado y todos los call-sites se re-enrutan.

## Tareas

- [x] 1.1 Unit: `container_unit_always_appends_service_suffix`.
- [x] 1.2 Unit: `strip_quadlet_extension_only_strips_known_extensions`.
- [x] 1.3–1.7 Integración: start/stop/restart, dry-run, logs, status y argumento con nombre completo.
- [x] 2.1–2.5 Implementación GREEN y eliminación de `systemd_unit`.
- [x] 3.1–3.4 Refactor, docstrings de contrato y spec ampliado.
- [x] 4.1 Test de argumento completo y robustez de `recent_logs`.
- [x] 4.2 `openspec validate fix-systemd-unit-dots --strict` en verde.
- [x] 4.3 `cargo test` completo en verde.
- [x] 4.4 Auditoría con `@rust-reviewer`.
- [x] 4.5 PR a `develop`, merge y `openspec archive fix-systemd-unit-dots`.

## Documentación de la auditoría

- [x] Hallazgo 1 (ALTO): limitación de round-trip documentada en el spec (`## KNOWN LIMITATIONS`) y en `docs/fix-systemd-unit-dots.md`.
- [x] Hallazgo 2 (MEDIO): contrato de `logs` con nombres no-container documentado en el spec.
- [x] Hallazgo 3 (MEDIO): `recent_logs` normaliza con `strip_quadlet_extension`.
- [x] Hallazgo 4 (MEDIO): `tasks.md` del change creado.
- [x] Hallazgo 5 (MEDIO): tests de argumento completo y dry-run de restart añadidos.
- [x] Hallazgo 6 (BAJO): docstrings de contrato en `is_active`, `systemctl_user`, `container_unit` y `strip_quadlet_extension`.
- [x] Hallazgo 7 (NIT): `docs/AUDIT.md` actualizado (mención a `systemd_unit`).
- [x] Hallazgo 9 (NIT): `plans/PLAN-002.md` y `docs/fix-systemd-unit-dots.md` creados.

## Estado

**COMPLETADO** — `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` (9 unit + 84 integración) y `openspec validate fix-systemd-unit-dots --strict` en verde.

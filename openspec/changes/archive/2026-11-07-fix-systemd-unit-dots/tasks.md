# Tasks

## 1. Tests primero (RED)

- [x] 1.1 Unit: `container_unit_always_appends_service_suffix` (`voicebox`, `voicebox.prod`, `a.b.c`).
- [x] 1.2 Unit: `strip_quadlet_extension_only_strips_known_extensions`.
- [x] 1.3 Integración: start/stop/restart con stem punteado forman `<stem>.service` y no abortan con `no such quadlet`.
- [x] 1.4 Integración: `--dry-run start` imprime la unidad formada.
- [x] 1.5 Integración: `logs` usa `-u <stem>.service`.
- [x] 1.6 Integración: `status` consulta `is-active <stem>.service` y reporta running (nunca el stem pelado).
- [x] 1.7 Integración: argumento con nombre de fichero completo aceptado (este paso).

## 2. Implementación (GREEN)

- [x] 2.1 `container_unit(stem)` — siempre añade `.service`.
- [x] 2.2 `strip_quadlet_extension(name)` — recorta solo extensiones de quadlet conocidas (`QUADLET_EXTENSIONS`).
- [x] 2.3 Re-enrutado de call-sites: start/stop/restart (+dry-run), logs, recent_logs, status/menu (is_active), ensure_service_known, resolve_targets, update.
- [x] 2.4 Eliminación de `systemd_unit` y su test heurístico.
- [x] 2.5 `systemctl_user`/`is_active` dejan de re-transformar la unidad.

## 3. Refactor y documentación

- [x] 3.1 `cargo fmt --all -- --check` y `cargo clippy --all-targets --all-features -- -D warnings` limpios.
- [x] 3.2 Spec ampliado: resolución del argumento (`strip_quadlet_extension`) y status-output; limitaciones conocidas documentadas.
- [x] 3.3 Docstrings de `is_active`/`systemctl_user` indicando que reciben la unidad ya formada.
- [x] 3.4 Docstrings de `container_unit`/`strip_quadlet_extension`.

## 4. Verificación y cierre

- [x] 4.1 Tests de argumento completo (`voicebox.prod.container`) y robustez recent_logs.
- [x] 4.2 `openspec validate fix-systemd-unit-dots --strict` en verde.
- [x] 4.3 `cargo test` completo en verde.
- [x] 4.4 Auditoría con `@rust-reviewer`.
- [x] 4.5 PR a `develop`, merge y `openspec archive fix-systemd-unit-dots`.

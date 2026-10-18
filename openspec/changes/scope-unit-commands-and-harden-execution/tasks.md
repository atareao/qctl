# Tasks

## 1. Tests primero (RED)

- [x] 1.1 `stop_without_arguments_does_not_stop_other_project_units`: proyectos A y B enlazados en el mismo `HOME`; `stop` desde A registra en el log del `systemctl` falso solo las unidades de A.
- [x] 1.2 `restart_without_arguments_does_not_restart_other_project_units`: mismo layout; solo las de A.
- [x] 1.3 `start_without_arguments_does_not_start_other_project_units`: mismo layout; solo las de A.
- [x] 1.4 `stop_table_lists_only_the_current_project`: mismo layout; la tabla impresa por `stop` no menciona las unidades de B.
- [x] 1.5 `status_reports_units_from_every_project`: mismo layout; la tabla incluye las de A y las de B.
- [x] 1.6 `stop_with_explicit_name_still_resolves_target_dir`: `svc.container` enlazado y con el origen borrado; `stop svc` se acepta (guarda contra la regresión de `installed_unit_survives_missing_source`).
- [x] 1.7 Endurecer `logs_uses_plain_journalctl_flags` y `logsf_uses_extra_journalctl_flags` para exigir `--no-pager` en los argumentos registrados.
- [x] 1.8 `systemctl_timeout_fails_with_unit_and_action`: shim `systemctl` que duerme; `--timeout 1 stop voicebox` sale != 0 dentro del margen y el error nombra `voicebox`.
- [x] 1.9 `timeout_zero_disables_the_limit`: mismo shim; `--timeout 0` termina con éxito.
- [x] 1.10 `podman_pull_is_not_bounded_by_the_timeout`: shim `podman` que duerme más que el timeout; `update` con `--timeout 1` termina con éxito.
- [x] 1.11 `logs_is_not_bounded_by_the_timeout`: shim `journalctl` que duerme; `--timeout 1 logs voicebox` no se aborta.
- [x] 1.12 `negative_timeout_is_rejected`: `--timeout -1` sale != 0 con error de uso de clap.

## 2. Implementación (GREEN)

- [x] 2.1 `Cli`: opción global `--timeout <SEGUNDOS>` (default 60; `0` → sin límite). `AppContext`: campo `timeout: Option<Duration>`.
- [x] 2.2 `resolve_targets`: separar los dos modos — local (solo orígenes del proyecto) y amplio (orígenes + target dir). `start`/`stop`/`restart` usan el local; `status` y `service_rows` el amplio.
- [x] 2.3 La tabla final de `start`/`stop`/`restart` sigue el modo local; la de la consulta `status` y la de `menu` siguen el amplio.
- [x] 2.4 `run_command`: parámetro de timeout; `None` = sin límite. Envolver la espera en `tokio::time::timeout` y activar `kill_on_drop` en el `Command` para no dejar procesos huérfanos al expirar.
- [x] 2.5 `is_active`: aplicar el mismo timeout (hoy invoca `Command::new` directamente, sin pasar por `run_command`).
- [x] 2.6 `systemctl_user` y `daemon_reload`: acotadas.
- [x] 2.7 `podman volume rm` y `/usr/lib/podman/quadlet`: acotadas.
- [x] 2.8 `podman pull`: invocación explícitamente sin timeout.
- [x] 2.9 `logs`/`logsf`: añadir `--no-pager` y dejarlas sin timeout.
- [x] 2.10 Mensaje del timeout: nombra el comando y la unidad, reutilizando `format_command`.

## 3. Refactor y documentación

- [x] 3.1 `cargo fmt --all -- --check` y `cargo clippy --all-targets --all-features -- -D warnings` sin avisos.
- [x] 3.2 `README.md`: alcance de `start`/`stop`/`restart` sin argumentos, la opción `--timeout` y `--no-pager`.
- [x] 3.3 `docs/AUDIT.md`: nueva ronda con los dos hallazgos (#5 y #6).
- [x] 3.4 Rellenar el `Purpose` de `openspec/specs/services/spec.md` y `openspec/specs/install/spec.md`, que siguen en `TBD`.
- [x] 3.5 Actualizar el recuento de tests en `AGENTS.md`.

## 4. Verificación y cierre

- [ ] 4.1 `openspec validate scope-unit-commands-and-harden-execution --strict` en verde.
- [ ] 4.2 `cargo test` completo en verde.
- [ ] 4.3 Auditoría con `@rust-reviewer`: cerrar bloqueantes, mayores y menores hasta LISTO PARA MERGE.
- [ ] 4.4 PR contra `develop`, CI verde, merge y `openspec archive scope-unit-commands-and-harden-execution`.
- [ ] 4.5 `release/0.4.0` con el ritual de `CONTRIBUTING.md`: bump, PR a `main`, tag `v0.4.0`, GitHub Release y verificación del binario estático y su checksum.
- [ ] 4.6 Comprobar que no quedan PRs abiertos ni tareas sin marcar.

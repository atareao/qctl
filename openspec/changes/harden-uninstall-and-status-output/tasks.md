# Tasks

## 1. Tests primero (RED)

- [ ] 1.1 `uninstall_stops_generated_volume_unit`: `stack.container` y `stack-x.volume` enlazados en el mismo `HOME`; `uninstall` registra en el log del `systemctl` falso `stop stack.service` **y** `stop stack-x-volume.service`, y borra ambos symlinks.
- [ ] 1.2 `uninstall_stops_generated_network_unit`: mismo layout con `stack.network`; registra `stop stack-network.service` y borra el symlink.
- [ ] 1.3 `uninstall_still_stops_container_unit`: guarda de no regresión; `voicebox.container` sigue produciendo `stop voicebox.service`.
- [ ] 1.4 `uninstall_does_not_stop_regular_files`: fichero regular `one.container`; ningún `stop one.service` en el log y el fichero queda intacto.
- [ ] 1.5 `uninstall_keeps_going_when_stop_fails`: shim `systemctl` que sale != 0 para la unidad de volumen; `uninstall` sale 0, avisa y aun así borra el symlink.
- [ ] 1.6 `compact_output_has_no_emoji`: `status --compact` con unidades enlazadas; stdout no contiene `✅`, `❌`, `🟢`, `🟡` ni `⚫`.
- [ ] 1.7 `compact_output_is_tab_separated_plain_tokens`: unidad enlazada y activa → línea exacta `voicebox\tlinked\trunning`.
- [ ] 1.8 `compact_missing_unit_reports_dash_state`: unidad sin symlink → `voicebox\tmissing\t-`.
- [ ] 1.9 `compact_prints_no_summary`: `status --compact` no imprime `Summary:` ni los contadores.
- [ ] 1.10 `status_project_lists_only_current_project`: proyectos A y B enlazados; `status --project` desde A no menciona las unidades de B.
- [ ] 1.11 `status_without_flag_reports_whole_target_dir`: guarda de la spec vigente; `status` sin flag lista A y B.
- [ ] 1.12 `status_project_with_explicit_service`: `status --project voicebox` acota al proyecto y muestra `voicebox`.

## 2. Implementación (GREEN)

- [ ] 2.1 Helper para el nombre de unidad generado por tipo de quadlet: `.container` → `<stem>.service`, `.volume` → `<stem>-volume.service`, `.network` → `<stem>-network.service`; reutilizar `systemd_unit` donde encaje.
- [ ] 2.2 `uninstall`: parar la unidad por **extensión** (no solo `if ext == \"container\"`), manteniendo el `warn!` y la continuación ante fallo, y sin tocar ficheros regulares.
- [ ] 2.3 `status` rama `compact`: reescribir el `println!` a TSV `<unit>\t<link>\t<state>` con tokens `linked`/`missing` y `running`/`stopped`/`-`; sin emojis ni colores; salir antes del bloque `Summary:`.
- [ ] 2.4 `Commands::Status`: añadir el flag `--project` (bool) y pasar `TargetScope::Project` cuando esté presente; `Wide` en caso contrario.
- [ ] 2.5 Actualizar el texto de ayuda de `--compact` para describir el contrato TSV (y el de `--project`).

## 3. Refactor y documentación

- [ ] 3.1 `cargo fmt --all -- --check` y `cargo clippy --all-targets --all-features -- -D warnings` sin avisos.
- [ ] 3.2 `README.md`: formato de `status --compact`, flag `status --project` y mención de que `uninstall` para también volúmenes y redes.
- [ ] 3.3 `docs/AUDIT.md`: nueva ronda con los tres hallazgos (unidad fantasma de volumen, `--compact` con emojis, ausencia de vista local en `status`).
- [ ] 3.4 Rellenar cualquier `Purpose` que quede en `TBD` en las specs tocadas.

## 4. Verificación y cierre

- [ ] 4.1 `openspec validate harden-uninstall-and-status-output --strict` en verde.
- [ ] 4.2 `cargo test` completo en verde.
- [ ] 4.3 Auditoría con `@rust-reviewer`: cerrar bloqueantes, mayores y menores hasta LISTO PARA MERGE.
- [ ] 4.4 PR contra `develop`, CI verde, merge y `openspec archive harden-uninstall-and-status-output`.
- [ ] 4.5 Añadir los eventos a `CHANGELOG.md`/release según `CONTRIBUTING.md` y comprobar que no quedan PRs abiertos ni tareas sin marcar.

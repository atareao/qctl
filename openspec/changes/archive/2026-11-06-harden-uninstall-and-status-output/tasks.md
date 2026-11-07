# Tasks

## 1. Tests primero (RED)

- [x] 1.1 `uninstall_stops_generated_volume_unit`: `stack.container` y `stack-x.volume` enlazados en el mismo `HOME`; `uninstall` registra en el log del `systemctl` falso `stop stack.service` **y** `stop stack-x-volume.service`, y borra ambos symlinks.
- [x] 1.2 `uninstall_stops_generated_network_unit`: mismo layout con `stack.network`; registra `stop stack-network.service` y borra el symlink.
- [x] 1.3 `uninstall_still_stops_container_unit`: guarda de no regresión; `voicebox.container` sigue produciendo `stop voicebox.service`.
- [x] 1.4 `uninstall_does_not_stop_regular_files`: fichero regular `one.container`; ningún `stop one.service` en el log y el fichero queda intacto.
- [x] 1.5 `uninstall_keeps_going_when_stop_fails`: shim `systemctl` que sale != 0 para la unidad de volumen; `uninstall` sale 0, avisa y aun así borra el symlink.
- [x] 1.6 `compact_output_has_no_emoji`: `status --compact` con unidades enlazadas; stdout no contiene `✅`, `❌`, `🟢`, `🟡` ni `⚫`.
- [x] 1.7 `compact_output_is_tab_separated_plain_tokens`: unidad enlazada y activa → línea exacta `voicebox\tlinked\trunning`.
- [x] 1.8 `compact_missing_unit_reports_dash_state`: unidad sin symlink → `voicebox\tmissing\t-`.
- [x] 1.9 `compact_prints_no_summary`: `status --compact` no imprime `Summary:` ni los contadores.
- [x] 1.10 `status_project_lists_only_current_project`: proyectos A y B enlazados; `status --project` desde A no menciona las unidades de B.
- [x] 1.11 `status_without_flag_reports_whole_target_dir`: guarda de la spec vigente; `status` sin flag lista A y B.
- [x] 1.12 `status_project_with_explicit_service`: `status --project voicebox` acota al proyecto y muestra `voicebox`.

## 2. Implementación (GREEN)

- [x] 2.1 Helper para el nombre de unidad generado por tipo de quadlet: `.container` → `<stem>.service`, `.volume` → `<stem>-volume.service`, `.network` → `<stem>-network.service`; reutilizar `systemd_unit` donde encaje.
- [x] 2.2 `uninstall`: parar la unidad por **extensión** (no solo `if ext == \"container\"`), manteniendo el `warn!` y la continuación ante fallo, y sin tocar ficheros regulares.
- [x] 2.3 `status` rama `compact`: reescribir el `println!` a TSV `<unit>\t<link>\t<state>` con tokens `linked`/`missing` y `running`/`stopped`/`-`; sin emojis ni colores; salir antes del bloque `Summary:`.
- [x] 2.4 `Commands::Status`: añadir el flag `--project` (bool) y pasar `TargetScope::Project` cuando esté presente; `Wide` en caso contrario.
- [x] 2.5 Actualizar el texto de ayuda de `--compact` para describir el contrato TSV (y el de `--project`).

## 3. Refactor y documentación

- [x] 3.1 `cargo fmt --all -- --check` y `cargo clippy --all-targets --all-features -- -D warnings` sin avisos.
- [x] 3.2 `README.md`: formato de `status --compact`, flag `status --project` y mención de que `uninstall` para también volúmenes y redes.
- [x] 3.3 `docs/AUDIT.md`: nueva ronda con los tres hallazgos (unidad fantasma de volumen, `--compact` con emojis, ausencia de vista local en `status`).
- [x] 3.4 Rellenar cualquier `Purpose` que quede en `TBD` en las specs tocadas.

## 4. Verificación y cierre

- [x] 4.1 `openspec validate harden-uninstall-and-status-output --strict` en verde.
- [x] 4.2 `cargo test` completo en verde.
- [x] 4.3 Auditoría con `@rust-reviewer`: cerrar bloqueantes, mayores y menores hasta LISTO PARA MERGE.
- [x] 4.4 PR contra `develop`, CI verde, merge y `openspec archive harden-uninstall-and-status-output`.
- [x] 4.5 Añadir los eventos a `CHANGELOG.md`/release según `CONTRIBUTING.md` y comprobar que no quedan PRs abiertos ni tareas sin marcar.

## 5. Ronda de correcciones post-auditoría (A–D)

- [x] A (mayor) `generated_unit` con mapeo explícito, sin depender del heurístico de `systemd_unit`: stems con puntos (`voicebox.prod.container`) deben generar `voicebox.prod.service`; tipos con sufijo del generador (`.volume`, `.network`, `.pod`, `.kube`, `.image`, `.build`, `.artifact`) generan `<stem>-<ext>.service`; test unitario ampliado y `uninstall_stops_generated_unit_for_dotted_container_name`.
- [x] B (menor) Tests de `--project` discriminantes: `status_project_with_no_units_exits_successfully` con proyecto A vacío + `bravo` de B enlazado (con guarda de contraste sin flag); `status_project_with_explicit_service` renombrado a `status_project_with_explicit_service_resolves_the_name`.
- [x] C (menor) `--compact` con cero unidades no imprime prosa (stdout vacío) + test `compact_with_no_units_prints_nothing`.
- [x] D (menor) Redacción de spec: escenario de `Summary:` aclarado (sin líneas de contadores, tokens TSV legítimos); escenario nuevo de compact sin unidades; escenario de servicio explícito documenta que se resuelve con o sin `--project`.

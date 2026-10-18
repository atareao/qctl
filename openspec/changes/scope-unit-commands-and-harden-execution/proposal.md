# Acotar los comandos de unidad y endurecer la ejecución externa

## Why

Dos defectos detectados durante la revisión del PR #2 siguen abiertos y atacan el mismo flanco: `qctl` hace más de lo que el usuario espera cuando habla con systemd.

- **#5** — `qctl stop` y `qctl restart` sin argumentos actúan sobre **todas** las unidades enlazadas en `$HOME/.config/containers/systemd`, incluidas las de otros proyectos. `resolve_targets` ensancha el conjunto con el contenido del target dir para que `menu` y `status` muestren lo instalado desde cualquier directorio, pero ese ensanchado alcanza también a los comandos destructivos.
- **#6** — Las invocaciones externas no llevan `--no-pager` ni timeout. `journalctl` puede heredar el paginador interactivo del usuario, y `systemctl` puede bloquearse sin que el proceso termine. En una CLI operativa, esa es la diferencia entre "se colgó" y "timeout claro".

## What Changes

- **`start`, `stop` y `restart` sin argumentos pasan a ser de alcance local**: actúan solo sobre los quadlets del proyecto actual. **BREAKING** para quien contara con que `stop` en el directorio del proyecto A parase también las unidades del proyecto B.
- `status` y `menu` conservan la vista amplia del target dir: siguen mostrando todas las unidades enlazadas en el `HOME`, vengan del proyecto que vengan.
- La tabla que los comandos de unidad imprimen al terminar refleja el mismo conjunto sobre el que actuaron, no el del `HOME` entero.
- Un servicio explícito (`stop <nombre>`) sigue resolviéndose contra el proyecto y contra el target dir, así que una unidad instalada cuyo origen desapareció se sigue pudiendo parar por nombre.
- Nueva opción global `--timeout <SEGUNDOS>`: límite para las invocaciones externas acotadas, 60 por defecto, `0` para desactivarlo.
- `journalctl` recibe `--no-pager` en todas sus invocaciones.
- El timeout **no** se aplica a `podman pull` (depende de la red y puede tardar legítimamente minutos) ni a `logs`/`logsf`, que son modo follow (`-f`) y deben correr hasta que el usuario interrumpa.

## Capabilities

### New Capabilities

- `command-execution`: cómo `qctl` invoca herramientas externas — supresión del paginador interactivo y límite de tiempo configurable para las llamadas acotadas.

### Modified Capabilities

- `services`: qué unidades alcanzan `start`, `stop` y `restart` cuando no reciben un servicio; `status` y `menu` mantienen el alcance amplio del target dir.

## Impact

- **Specs**: nueva `command-execution`; delta de `services`.
- **Código**: `src/main.rs` — `resolve_targets` (dos modos de resolución), `start`, `stop`, `restart` (modo local), `status`, `service_rows` y la tabla final de los comandos de unidad (modo local para el seguimiento de la acción, amplio para la consulta), `run_command`, `run_command_streaming`, `is_active`, `systemctl_user`, `daemon_reload`, `logs`, `check_quadlet`, `clean_volumes`, `AppContext` (campo `timeout`) y el enum `Cli` (`--timeout`).
- **Tests**: `tests/cli.rs` — escenarios nuevos de alcance local, de `--no-pager` y de timeout (con un shim que duerme y el helper `wait_with_timeout` que ya existe); endurecer `logs_uses_plain_journalctl_flags` y `logsf_uses_extra_journalctl_flags` para exigir `--no-pager` en vez de conformarse con un substring.
- **Docs**: `README.md` (alcance de `stop`/`restart`/`start`, `--timeout` y `--no-pager`) y `docs/AUDIT.md` (nueva ronda con los dos hallazgos).
- **Compatibilidad**: cambio de comportamiento en `start`/`stop`/`restart` sin argumentos. El contrato de `--dry-run` no cambia.
- **Menor**: rellenar el `Purpose` que las specs vivas `services` e `install` dejaron en `TBD` al archivarse el change anterior.

## Out of Scope

- Soporte de `ServiceName=` para direccionar unidades renombradas: limitación conocida, ya documentada en `README.md` y `docs/AUDIT.md`.
- Un flag `--all`/`--global` para recuperar el comportamiento transversal: no se introduce en este change.
- Descubrimiento recursivo de quadlets (#12, cerrado como `not planned`).
- Aislar por completo el target dir de `qctl`: fuera de alcance.

# Fix: quadlets dentro del target dir y resolución de servicios

## Why

Dos bugs detectados durante la revisión del PR #2 dejan a `qctl` en estados de los
que no se recupera solo, o con efectos secundarios silenciosos:

- **#3** — Si el directorio de trabajo es el propio target dir (`$HOME/.config/containers/systemd`)
  y contiene un subdirectorio con quadlets, `install` crea un symlink dentro del
  target dir apuntando a un fichero que ya está dentro del target dir. La segunda
  ejecución falla con `duplicate quadlet file name` y, a partir de ahí, `install`,
  `reinstall` y `uninstall` quedan inservibles hasta limpiar a mano.
- **#4** — `qctl start <servicio inexistente>` instala **todos** los quadlets,
  ejecuta `daemon-reload` y sale con código 0. Un typo tiene el mismo efecto que
  `qctl install`.

## What Changes

- La detección de "ya está en el target dir" pasa de comparar el padre directo a
  comprobar pertenencia al árbol (ancestro, no igualdad).
- `collect_quadlets` tolera entradas redundantes que resuelven al mismo fichero
  (symlinks creados por versiones anteriores), pero sigue rechazando duplicados
  reales.
- `qctl` deja de borrar o reemplazar ficheros regulares del target dir: solo
  elimina symlinks.
- Los comandos que actúan sobre unidades (`start`, `stop`, `restart`, `update`)
  validan el nombre del servicio y fallan con `no such quadlet: <name>` (exit != 0)
  **antes** de crear enlaces, ejecutar `daemon-reload` o invocar `systemctl`/`podman`.
- `start <servicio>` instala **solo** la unidad pedida, no el proyecto completo.
- `start` falla con exit != 0 si alguna unidad no se pudo arrancar o no quedó
  enlazada.

## Impact

- **Specs afectadas**: `install` (nueva), `services` (nueva).
- **Código**: `src/main.rs` — `install`, `uninstall`, `source_is_in_target_dir`,
  `collect_quadlets`, `link_or_replace`, `resolve_targets`, `start`,
  `ensure_targets_installed`, `install_missing_targets`, `update`.
- **Tests**: `tests/cli.rs` — 8 escenarios nuevos y ajuste de los existentes que
  asuman el comportamiento antiguo.
- **Docs**: `README.md` (contrato de `start`), `docs/AUDIT.md`.
- **Compatibilidad**: cambio de comportamiento en la CLI. `start` sin argumentos
  no cambia. `update <nombre-inexistente>` pasa de salir con 0 a fallar.

## Out of Scope

- `status`, `check`, `logs` y `logsf` no validan nombres: son consultas de solo
  lectura y seguirán reportando lo que exista (o su error de `systemctl`).
- Flags de `--no-pager`/timeouts (#6) y el alcance transversal de `stop`/`restart`
  sobre unidades de otros proyectos (#5).
- No se introduce `--force`: si `qctl` no puede reemplazar algo, lo dice y sigue.

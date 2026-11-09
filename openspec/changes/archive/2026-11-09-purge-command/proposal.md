# Proposal: comando `qctl purge`

## Why

`qctl uninstall` sólo **detiene y desenlaza** el stack, y `qctl clean-volumes` sólo
borra los **volúmenes nombrados** declarados en los `.volume`. Todo lo demás que un
stack deja al ejecutarse —secretos (`podman secret create`), imágenes (`Image=`) y datos
en bind mounts host-side (`%h/.config/<stack>/...`, a menudo con uid mapeado)— sigue vivo
y hoy hay que limpiarlo a mano con varios comandos `podman`. Un caso real de
"desinstalar" cuatro stacks exigió un `uninstall`, seis `volume rm`, dos `secret rm`, un
`podman unshare rm -rf` y cuatro `rmi`.

## What Changes

- **Nuevo subcomando `qctl purge`** con flags opt-in:
  `[--volumes] [--secrets] [--images] [--data] [--force] [--yes]`.
- **Base (sin flags)**: `uninstall` (stop + desenlaza) + `clean-volumes` (volúmenes
  nombrados declarados). `--volumes` es explícito pero ya incluido en la base.
- **`--secrets`**: `podman secret rm` de cada `Secret=<name>[,type=...]` de los
  `.container` del stack (un fichero puede declarar varios).
- **`--images`**: `podman rmi` de cada `Image=` de los `.container` del stack.
- **`--data`**: borra los **bind mounts** host-side declarados en `Volume=`, resolviendo
  sufijos systemd (`%h`→`$HOME`, `%t`→`XDG_RUNTIME_DIR` o `/run/user/<uid>`), sólo bajo
  `$HOME`, ignorando sockets/ficheros y rutas de sistema; usa `podman unshare rm -rf`
  cuando el uid está mapeado (`User=<uid>`).
- **Guarda de recursos compartidos**: antes de borrar un secreto o una imagen, escanear
  todo el stacks root y saltar el recurso si **otra** stack lo sigue declarando (mensaje
  `skip: ...`). `--force` lo ignora.
- **`--dry-run` global respetado**: imprime las acciones (`DRY-RUN podman ...`) sin
  ejecutarlas. **`--yes`** omite la confirmación previa (que lista lo que se borrará).

## Capabilities

### New Capabilities

- `purge`: comando `qctl purge` — desinstalación destructiva selectiva de un stack
  (volúmenes, secretos, imágenes y datos de bind mounts) con guarda de recursos
  compartidos, `--dry-run` y confirmación.

### Modified Capabilities

<!-- Ninguna: purge es una capacidad nueva; no cambia requisitos de specs existentes. -->

## Impact

- **Código**: `src/main.rs` — `enum Commands` (+`Purge`), dispatch del `match`, nueva
  `async fn purge`, nuevo `parse_secret_name` y parser de binds; reutiliza `uninstall`,
  `clean_volumes`, `confirm_clean_volumes`, `parse_volume_name`, `parse_image_name`,
  `collect_quadlets`, `discover_source_dirs`, `extension_of_path`, `run_command`,
  `scan_stacks_root`/`stack_quadlet_dirs`.
- **Tests**: `src/main.rs` (`#[cfg(test)]`) para parsers y resolución de binds;
  `tests/cli.rs` para la guarda cross-stack, `--dry-run` y `--data`.
- **Docs**: `README.md` (tabla de comandos) y, en `self-hosted-podman`,
  `docs/gestion-stacks-qctl.md` (remitir a `qctl purge`).
- **Sin cambios de dependencias**; Linux-only igual que el resto.

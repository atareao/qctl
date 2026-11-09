# Design

## Context

Ver `proposal.md - Why`. Estado actual relevante en `src/main.rs` (single crate, single
binary):

- `uninstall(&ctx)` (≈L360) itera `collect_quadlets(&ctx.source_dirs)`, para cada
  symlink en `ctx.target_dir` hace `systemctl --user stop` y `fs::remove_file`, y termina
  con `daemon_reload`. Respeta `ctx.dry_run`.
- `clean_volumes(&ctx, yes)` (≈L1063) recoge `VolumeName=` de los `.volume` con
  `parse_volume_name`, y borra con `podman volume rm -f`. Tiene su propio
  `confirm_clean_volumes` (≈L1117).
- `parse_ini_value` (≈L1796) usa `find_map`: **sólo devuelve la primera** línea de la clave.
  `parse_image_name` (≈L1812) lo reaprovecha. **No sirve** para múltiples `Secret=` por
  fichero.
- `scan_stacks_root` (≈L1249) + `stack_quadlet_dirs` (≈L1311) recorren cada stack del
  stacks root y sus `quadlets/`, pero sólo recogen `basename` y `ContainerName=`.
- `current_stack_name(ctx)` (≈L1394) resuelve la stack actual dentro de `ctx.stacks_root`.
- `run_command` (≈L1901) ejecuta con timeout; `ctx.dry_run` desvía a prints.

## Goals / Non-Goals

**Goals:**

- Un subcomando declarativo que sustituya la limpieza manual multi-`podman`.
- Guarda de recursos compartidos reutilizando el recorrido del stacks root.
- `--data` seguro: sólo binds, sólo bajo `$HOME`, con `podman unshare` para uid mapeado.

**Non-Goals:**

- No borra los quadlets de las fuentes ni de `available/` (igual que `uninstall`).
- No gestiona pods ni `.kube`.
- No introduce dependencias nuevas ni cambia las specs existentes.

## Decisions

### D1. Nueva capacidad `purge`, comandos base reutilizados

`purge` compone: `uninstall(&ctx)` + `clean_volumes(&ctx, yes)` para la base, y funciones
nuevas para `--secrets`/`--images`/`--data`. Alternativa considerada: extender
`uninstall`/`clean_volumes` con flags; rechazada porque mezcla responsabilidades y rompe
specs archivadas (`install`/`services`). Se prefiere una capacidad nueva aislada.

### D2. Parser multi-valor para `Secret=` (nuevo)

`parse_ini_value` devuelve sólo la primera ocurrencia, así que `Secret=` necesita un parser
propio. Decisión: `parse_secret_names(content: &str) -> Vec<String>` que recorre **todas**
las líneas, tolera indentación y comentario inline, y toma el nombre como el texto anterior
a la primera `,` (`Secret=name,type=env,target=VAR` → `name`). `--images` reutiliza
`parse_image_name` por fichero (una imagen por `.container`, iterando los ficheros).

Alternativa: generalizar `parse_ini_value` a multi-valor; rechazada por tocar rutas de
`check`/`clean_volumes` ya estables y auditadas.

### D3. Guarda cross-stack con un escaneo de recursos dedicado

Se añade `scan_stack_resources(root) -> HashMap<stack, StackResources{secrets, images}>`
que reutiliza `stack_quadlet_dirs` y `QUADLET_EXTENSIONS`, y aplica los parsers de D2. Antes
de borrar cada secreto/imagen, se consulta ese mapa: si aparece en una stack **distinta** de
la actual, se salta con `skip: ...` (salvo `--force`). Alternativa: ampliar `StackUnit` de
`scan_stacks_root`; rechazada para no alterar el camino de `check`/`ensure_no_global_collision`.

### D4. `--data`: parseo de `Volume=` de binds y resolución de sufijos

Nuevo parser `parse_volume_binds(content) -> Vec<VolumeMount{source, uid_mapped}>`. Por
cada `Volume=` cuyo source no acabe en `.volume` (no es volumen nombrado) y sea ruta:

- `%h` → `$HOME`, `%t` → `XDG_RUNTIME_DIR` (o `/run/user/<uid>`).
- Elegible **sólo** si la ruta resuelta cae bajo `$HOME`.
- Se descartan sockets/ficheros y todo lo que resida bajo `%t` (en particular
  `%t/podman/podman.sock`).
- `uid_mapped` = existe `User=` en el quadlet → se usa `podman unshare rm -rf`; si no,
  `rm -rf`.
- **Normalización y descendencia estricta**: antes de decidir la elegibilidad se normalizan
  léxicamente `.`/`..` (sobre `Path::components`) y se exige descendiente **estricto** de
  `$HOME` (la igualdad con `$HOME` se rechaza). Sin esto, `Path::starts_with` acepta `$HOME`
  mismo y rutas con `..` que escapan del home. Se aceptan `%h/`, `~/`, `$HOME/`, `${HOME}/`;
  las rutas relativas (`./`) se rechazan (base ambigua).

Alternativa considerada: delegar en `podman` para resolver mounts vía inspección; rechazada
porque el stack ya está desinstalado en ese punto y no habría contenedor que inspeccionar.

### D5. Orden y confirmación única

Orden: base (`uninstall` + `clean-volumes`) → `--secrets` → `--images` → `--data`. La
confirmación (`--yes` o `confirm_purge`) lista **primero** todas las categorías activas —base
(symlinks + volúmenes nombrados) incluida— y se pide **una sola vez** ANTES de ejecutar
`uninstall`/`clean_volumes`; así no se borra nada si el usuario declina. Tras confirmar, se
ejecuta la base (`clean_volumes` con `yes=true` para no re-promptear) y luego los extras. `--dry-run` tiene
precedencia: imprime el plan completo y no ejecuta.

## Risks / Trade-offs

- **Borrado de datos host-side irreversible** → sólo rutas resueltas bajo `$HOME`, listado
  previo y confirmación por defecto; `%t` y rutas de sistema excluidas por diseño.
- **Falso negativo en la guarda** (recurso compartido no detectado si otro stack lo declara
  de forma no estándar) → mismo criterio de parseo que el resto de la herramienta; `--force`
  da la salida manual.
- **`parse_image_name` devuelve la primera imagen de un fichero** → suficiente para
  `.container` (una imagen por unidad), documentado en D2.
- **Stack actual no resuelto** (`current_stack_name` → `None`, p. ej. invocación fuera del
  stacks root) → la guarda no puede distinguir la propia stack de las demás. Decisión
  **fail-closed**: se registra un `warn!` y todo recurso declarado por cualquier stack se
trata como compartido (se salta), nunca se borra un recurso potencialmente compartido.

## Migration Plan

- Aditivo: la base sin flags replica `uninstall`+`clean-volumes`. No hay migración de datos
  ni de configuración; rollback = no usar el subcomando nuevo.
- Al fusionar: actualizar `README.md` (tabla de comandos) y, en `self-hosted-podman`,
  `docs/gestion-stacks-qctl.md`.

## Open Questions

<!-- Ninguna: alcance y reglas cerrados en el issue #29 y en la delta de spec. -->

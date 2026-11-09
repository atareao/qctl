# `qctl purge`

## Qué es

Limpieza destructiva declarativa de un stack: un único comando que sustituye la cadena
manual de `qctl uninstall` + varios `podman volume rm` / `podman secret rm` / `podman rmi` /
`podman unshare rm -rf`. Se ejecuta **desde el directorio del stack**, igual que el resto de
comandos de `qctl`.

Origina en el issue #29: al "desinstalar" cuatro stacks hubo que ejecutar a mano un
`uninstall`, seis `volume rm`, dos `secret rm`, un `podman unshare rm -rf` y cuatro `rmi`.

## Uso

```bash
qctl purge [--volumes] [--secrets] [--images] [--data] [--force] [--yes]
```

| Flag | Acción |
|---|---|
| (sin flags) | `uninstall` (stop + desenlaza) + `clean-volumes` (volúmenes nombrados declarados) |
| `--volumes` | explícito; ya incluido en la base |
| `--secrets` | `podman secret rm` de cada `Secret=<nombre>[,opciones]` de los `.container` |
| `--images` | `podman rmi` de cada `Image=` de los `.container` |
| `--data` | borra los **bind mounts** host-side declarados en `Volume=` |
| `--force` | ignora la guarda de recursos compartidos |
| `--yes` | sin confirmación |

Respeta el flag global `--dry-run` (imprime `DRY-RUN ...` sin ejecutar).

## Ficheros implicados

- `src/main.rs` — todo el comando (crate única y binario único).
  - `enum Commands::Purge { .. }` + dispatch del `match`.
  - `purge`, `build_purge_plan`, `confirm_purge`, `print_dry_run_plan`.
  - `remove_secrets`, `remove_images`, `remove_binds`, `collect_base_links`.
  - Parsers: `parse_secret_names` (multi-línea, nombre = texto antes de la primera `,`),
    `parse_volume_binds` / `resolve_volume_binds` / `resolve_bind_source` / `normalize_path`.
  - Guarda: `StackResources`, `scan_stack_resources`, `ResourceKind`, `other_stack_declaring`,
    `classify_guard` (`Guard::{Removable, Shared, Unresolved}`).
- `tests/cli.rs` — integración con shims de `podman`/`systemctl`.

## Cómo funciona

1. **Plan (solo lectura)**: `build_purge_plan` recopila
   - base: symlinks a desenlazar (`collect_base_links`) y volúmenes nombrados (`VolumeName=`);
   - extras: secretos, imágenes y rutas de bind resueltas.
   Un `.container`/`.volume` ilegible degrada a `warn!` y continúa (no aborta el plan).
2. **Confirmación única**: `confirm_purge` lista **todo** (links, volúmenes, secretos, imágenes
   y rutas) y pregunta **una sola vez, antes de tocar nada**. `--yes` la omite;
   `--dry-run` imprime el plan y retorna sin prompt ni ejecución.
3. **Ejecución**: base (`uninstall` + `clean_volumes(ctx, true)`, sin segundo prompt) y
   después los extras.
4. **Recursos ausentes**: cualquier fallo de `podman secret rm`/`rmi`/`rm` se avisa (`warn!`)
   y el bucle continúa; el comando sale 0.

### Guarda de recursos compartidos

Antes de borrar un secreto o una imagen, `scan_stack_resources` recorre el *stacks root*
(`stack_quadlet_dirs` + `QUADLET_EXTENSIONS`) y `other_stack_declaring` comprueba si **otra**
stack lo declara:

```
skip: secreto 'openobserve_root_password' todavía lo usa la stack 'vector'
skip: imagen 'docker.io/library/redis:alpine' todavía la usa la stack 'searxng'
```

- `--force` borra igualmente.
- **Fail-closed**: si la stack actual no se puede resolver (`current_stack_name == None`), se
  avisa y todo recurso declarado por cualquier stack se trata como compartido (no se borra).
- Si el stacks root no es legible, se avisa (`warn!`) y el mapa queda vacío.

### `--data`: reglas estrictas

Solo son candidatos los `Volume=` cuyo *source* sea una **ruta** host (no un volumen nombrado
`<x>.volume`) y que resuelva **bajo `$HOME`**:

- Prefijos aceptados: `%h/` → `$HOME/`, `~/`, `$HOME/`, `${HOME}/`.
- Se normalizan léxicamente `.`/`..` (`normalize_path`) **antes** de comparar y se exige un
  descendiente **estricto** de `$HOME`: nunca `$HOME` mismo, nunca un escape por `..`.
- Se ignoran sockets y ficheros (solo directorios), todo lo que cuelgue de `%t` (en especial
  `%t/podman/podman.sock`), las rutas de sistema fuera de `$HOME` y las relativas (`./…`).
- Antes de borrar, se resuelve el **path real en disco** con `fs::canonicalize` y se
  re-comprueba la contención contra el `$HOME` **canónico**: solo se borra si el path real
  es descendiente **estricto** de `$HOME`. Se borra el **path real resuelto** (canónico), no
  la ruta original declarada. La normalización léxica anterior se conserva como
  primera barrera. Si el path real escapa de `$HOME` (symlink intermedio o en la hoja) o la
  resolución falla, se avisa con `warn!` y se salta la entrada sin borrar nada.
- Si el quadlet declara `User=<uid>` (uid mapeado) se borra con `podman unshare rm -rf <ruta>`;
  si no, con `rm -rf <ruta>`.

Ejemplos:

| `Volume=` | Resultado |
|---|---|
| `%h/.config/navidrome/data:/data:Z` | borra `$HOME/.config/navidrome/data` |
| `%h/.config/navidrome/music:/music:ro` | borra `$HOME/.config/navidrome/music` |
| `%t/podman/podman.sock:/var/run/docker.sock:ro` | **ignorado** (socket, `%t`) |
| `%h:/x`, `$HOME/../../etc:/x` | **ignorados** (igual a `$HOME` / escape por `..`) |
| `%h/.config/navidrome/data:/data:Z` con `~/.config/navidrome -> /mnt/ssd/navidrome` | **ignorado** (el path real `/mnt/ssd/navidrome/data` escapa de `$HOME`) |
| `memos-memos.volume:/var/opt/memos:Z` | no es data (volumen nombrado → `clean-volumes`) |

## Variables / configuración

Sin variables propias. Usa el `stacks_root` global (o su default), `$HOME` y, por herencia,
`--dry-run` / `--timeout`.

## Cómo habilitar / desplegar

Es parte del binario: `cargo build --release` y sustituir `~/.local/bin/qctl`. Sin pasos
extra. La tabla de comandos del `README.md` ya lo documenta.

## Decisiones

- **Capacidad nueva** (`purge`) en vez de flags sobre `uninstall`/`clean-volumes`: aísla la
  responsabilidad y no rompe specs archivadas.
- **Parser propio para `Secret=`**: `parse_ini_value` solo devuelve la primera ocurrencia y
  un `.container` puede declarar varios secretos.
- **Confirmación única incluyendo la base**: se confirma antes de ejecutar `uninstall`, para
  que declinar no deje nada borrado.
- **Fail-closed** cuando la stack actual no se resuelve.
- **Doble barrera** en `--data`: primero normalización léxica (`resolve_bind_source`, rechaza
  `..` y `$HOME` mismo sin tocar el FS) y después resolución canónica (`resolve_real_bind`:
  `fs::canonicalize` + contención estricta contra el `$HOME` canónico). Cierra el escape por
  symlink intermedio (issue #31). Borra el **path real resuelto** (canónico), no la ruta
  original, para no re-seguir symlinks (cierra el TOCTOU).

## Follow-ups / limitación conocida

- Un **symlink hoja dentro de `$HOME`** que apunte fuera de `$HOME` no se sigue: se avisa y se
  salta (issue #31). Un symlink hoja que apunta **dentro** de `$HOME` se resuelve a su destino
  real y se borra el destino (no el enlace).
- Pendiente en `self-hosted-podman`: enlazar `docs/gestion-stacks-qctl.md` a `qctl purge`.

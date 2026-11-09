# Design

## Context

Ver `proposal.md - Why`. Estado actual relevante en `src/main.rs` (single crate, single
binary):

- `resolve_bind_source(source, home)` (≈L2322) resuelve el sufijo systemd y normaliza la
  ruta léxicamente con `normalize_path` (≈L2360), **sin tocar el FS**. Devuelve la ruta
  normalizada si es descendiente estricta de `$HOME`.
- `parse_volume_binds`/`resolve_volume_binds` (≈L2285) producen `Vec<VolumeMount>` con
  `source: PathBuf` y `uid_mapped: bool` (de la presencia de `User=`).
- `remove_binds(ctx, mounts)` (≈L1527) comprueba `fs::metadata(...).is_dir()` (sigue
  enlaces) y ejecuta `podman unshare rm -rf <path>` si `uid_mapped`, si no `rm -rf <path>`.

El hueco: entre la guarda léxica (en el plan) y el borrado (en `remove_binds`) no hay
resolución real de symlinks.

## Goals / Non-Goals

**Goals:**

- Impedir que `--data` borre fuera de `$HOME` por culpa de un symlink (intermedio o hoja).
- Mantener intacta la semántica del #29 (mismos flags, mismos casos felices).
- Verificación uniforme para `rm` y `podman unshare rm`.

**Non-Goals:**

- No cambia el contrato del #29 ni los flags.
- No introduce dependencias nuevas.
- No añade resolución de symlinks a otras rutas (sólo a los binds de `--data`).

## Decisions

### D1. Verificar en el punto de borrado, con `$HOME` canónico

Se calcula `home_canon = fs::canonicalize($HOME)`. Para cada mount, `real =
fs::canonicalize(&mount.source)`:
- `Err` → `warn!` (la ruta no existe o no se pudo resolver) y `skip`.
- `Ok(real)` → se borra **`real`, el path real resuelto**, y sólo si
  `real != home_canon && real.starts_with(&home_canon)`; si no, `warn!` nombrando
  `mount.source` y `real`, y `skip`.

`canonicalize` requiere que la ruta exista, lo cual es aceptable en el camino de borrado
(si no existe, no hay nada que borrar). Se borra `real` (no la ruta original declarada) para
que `rm -rf` no vuelva a seguir symlinks: si un enlace se cambia entre la verificación y el
borrado, el borrado sigue apuntando al path ya verificado, cerrando el TOCTOU.

### D2. La barrera léxica se conserva

`resolve_bind_source` sigue rechazando `..` y `$HOME` mismo antes de tocar el FS: es barato
y evita canonicalizar rutas manifiestamente inválidas. La verificación de D1 es la segunda
barrera, ya contra el FS real.

### D3. Symlink hoja no se sigue

Al canonicalizar, un symlink hoja que apunta fuera de `$HOME` produce un `real` fuera de
`home_canon` → `skip` + `warn`. Un symlink hoja que apunta dentro de `$HOME` se resuelve a
su destino real y se borra el destino (no el enlace). Se documenta.

## Alternatives considered

- **Borrar siempre `real` en vez de la ruta original**: adoptado. Se borra el path real
  resuelto (`real`), no la ruta original declarada, para no re-seguir symlinks en el `rm` y
  cerrar el TOCTOU entre la verificación y el borrado.
- **Sólo rechazar si el symlink es el último componente**: insuficiente; no cubre el caso
  de symlink intermedio, que es el grave.

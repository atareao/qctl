# Proposal: `qctl purge --data` debe resolver symlinks antes de borrar

## Why

`qctl purge --data` (issue #31) decide la elegibilidad de un bind **sobre el texto** de la
ruta: `resolve_bind_source` + `normalize_path` en `src/main.rs` normalizan `.`/`..`
léxicamente pero **no resuelven symlinks**. Después, `remove_binds` ejecuta `rm -rf <ruta>`
(o `podman unshare rm -rf <ruta>`) tal cual.

Consecuencia: si un componente **intermedio** del path es un symlink que apunta fuera de
`$HOME`, la cadena supera la guarda (parece estar bajo `$HOME`) pero `rm` sigue el enlace y
**borra fuera de `$HOME`**. Es un escape de la contención y un borrado destructivo no
intencionado.

```
$HOME/.config/navidrome -> /mnt/ssd/navidrome   (symlink)
Volume=%h/.config/navidrome/data:/data:Z
# qctl purge --data cree borrar $HOME/.config/navidrome/data
# pero rm -rf borra en realidad /mnt/ssd/navidrome/data
```

Caso "hoja": si el último componente es el symlink, `fs::metadata` (que sigue enlaces) hace
que `is_dir` sea `true` y `rm -rf <symlink>` borra **sólo el enlace**, no el destino (menos
grave, pero inesperado).

## What Changes

- **Endurecimiento interno de `--data`** (no cambia contrato ni flags del #29):
  antes de borrar cada bind, resolver la ruta real en disco con `fs::canonicalize` y
  re-comprobar la contención contra el `$HOME` **canónico**.
- La normalización léxica actual se **mantiene como primera barrera** (rechaza `..` y
  `$HOME` mismo antes de tocar el FS).
- La nueva verificación se aplica **por igual** a `rm -rf` y a `podman unshare rm -rf`.
- Ruta real fuera de `$HOME`, o `canonicalize` que falla (la ruta no existe / se desvaneció)
  → `warn!` nombrando la ruta y el destino real, y `skip` sin borrar.

## Capabilities

### New Capabilities

<!-- Ninguna. -->

### Modified Capabilities

- `purge`: se endurece el requisito de `--data` para que resuelva symlinks y re-compruebe
  la contención contra el `$HOME` real antes de borrar.

## Impact

- **Código**: `src/main.rs` — `remove_binds` (nueva verificación de canonicalización y
  contención contra `$HOME` canónico antes de invocar `rm`/`podman unshare`); posible
  helper puro reutilizable en `resolve_bind_source`.
- **Tests**: `src/main.rs` (`#[cfg(test)]`) y `tests/cli.rs` con symlinks temporales.
- **Docs**: `docs/purge.md` (nota de seguridad sobre la resolución de symlinks).
- **Sin cambios de dependencias**; Linux-only igual que el resto.

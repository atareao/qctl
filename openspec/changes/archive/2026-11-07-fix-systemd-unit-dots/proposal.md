# Fix: `systemd_unit` no maneja nombres de unidad con stems con puntos

## Why

`systemd_unit()` (`src/main.rs`) usa un heurístico: si el nombre contiene un punto, lo
devuelve tal cual; si no, le añade `.service`. Esto funciona para stems sin punto
(`voicebox` → `voicebox.service`) pero falla con quadlets cuyo stem contiene puntos,
p. ej. `voicebox.prod.container`:

- `strip_dot_extension("voicebox.prod.container")` → `voicebox.prod`
- `systemd_unit("voicebox.prod")` → `voicebox.prod` (**sin `.service`**)

## What Changes

- **Nuevo helper `container_unit(stem)`** que SIEMPRE añade `.service` (`format!("{stem}.service")`),
  sin heurístico de puntos.
- **`systemctl_user` y `is_active` dejan de re-aplicar heurísticos**: reciben el nombre de
  unidad ya formado y lo pasan tal cual a `systemctl --user`.
- **Enrutado de call-sites por `container_unit`**: `start`/`stop`/`restart` (incluidos sus
  `--dry-run`), `logs`, `recent_logs` y los call-sites de `is_active` en `status`/`menu`
  forman la unidad con `container_unit(stem)` antes de llamar a systemd.
- **`generated_unit` se mantiene intacto**: es el mapeo explícito stem+ext → unidad y ya es
  correcto (usado por `uninstall`); su salida pasa a `systemctl_user` sin transformación.
- **`systemd_unit` queda sin uso y se elimina** junto con su test heurístico.

## Impact

- `qctl start voicebox.prod` / `stop` / `restart` (y `--dry-run`) operan sobre
  `voicebox.prod.service` en vez de `voicebox.prod` (unidad inexistente).
- `status` / `menu` reportan `voicebox.prod` como running cuando está activo.
- `qctl logs voicebox.prod` consulta `journalctl -u voicebox.prod.service`.
- Sin cambio de comportamiento para stems sin puntos (no regresión).

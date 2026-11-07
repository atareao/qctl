# fix-systemd-unit-dots

## Qué es

Corrección de un bug por el que `qctl` formaba mal la unidad systemd de un quadlet
cuyo *stem* contiene puntos. Un contenedor `voicebox.prod.container` debe actuar sobre
`voicebox.prod.service`; antes, una heurística trataba cualquier nombre con punto como
unidad ya formada y pasaba `voicebox.prod` (sin `.service`) a `systemctl`, que abortaba
con `no such quadlet`.

## Ficheros implicados

- `src/main.rs` — helpers y re-enrutado de call-sites; command tests unitarios.
- `tests/cli.rs` — tests de integración con shims falsos de `systemctl`/`journalctl`
  (`DottedStemLayout`).
- `openspec/changes/fix-systemd-unit-dots/` — `proposal.md`, `tasks.md` y el delta de
  spec `specs/services/spec.md`.

## Cómo funciona

Dos helpers sustituyen a la heurística eliminada (`systemd_unit`):

- `container_unit(stem)` — **siempre** añade `.service`: `voicebox.prod` →
  `voicebox.prod.service`. Nunca re-transforma una unidad ya formada (no produce
  `.service.service`).
- `strip_quadlet_extension(name)` — recorta **solo** extensiones de quadlet conocidas
  (`QUADLET_EXTENSIONS`: `.container`, `.volume`, `.network`, `.pod`, `.kube`, `.image`,
  `.build`, `.artifact`, `.service`, `.socket`, `.mount`, `.timer`, `.path`). Deja
  intacto un stem punteado: `voicebox.prod` → `voicebox.prod`; `voicebox.prod.container`
  → `voicebox.prod`.

Re-enrutado: `start`/`stop`/`restart` (y su `--dry-run`), `logs`, `recent_logs`,
`status`/`menu` (`is_active`), `ensure_service_known`, `resolve_targets` y `update`
forman la unidad con `container_unit(strip_quadlet_extension(arg))`. `systemctl_user` e
`is_active` reciben la unidad **ya formada** y no aplican sufijos. `generated_unit`
(mapping explícito para `install`/`uninstall`) queda intacto.

## Limitación conocida

Un quadlet cuyo stem termine a su vez en una extensión de quadlet conocida —p. ej.
`app.service.container` (stem `app.service`)— se muestra como `app.service` en
`status`/`menu`, pero reintroducirlo como argumento (`qctl start app.service`) recorta
el `.service` y falla con `no such quadlet: app`. Es indistinguible de un stem que
simplemente acaba así; documentado como limitación, fuera de alcance del fix.

## Decisiones

- **`logs` siempre forma unidad de contenedor.** `qctl logs foo.mount` invoca
  `journalctl --user -u foo.service`: qctl es una herramienta de quadlets de
  contenedores, y el comportamiento es coherente con el fix.
- **`recent_logs` normaliza igual que `logs`** (`strip_quadlet_extension` +
  `container_unit`), por robustez ante futuros callers aunque hoy solo recibe stems.
- **`generated_unit` intacto**: el mapping explícito de `install`/`uninstall` ya era
  correcto para stems punteados y no depende de la heurística eliminada.

# Endurecer `uninstall` y hacer explícita la salida de `status`

## Why

Tres fricciones detectadas usando `qctl` sobre el stack self-hosted real. Las tres atacan el mismo flanco: `qctl` hace algo distinto de lo que su ayuda promete.

- **`uninstall` deja unidades fantasma.** `uninstall()` solo para la unidad generada cuando la extensión es `container` (`src/main.rs:343`, `if ext == "container"`). Los symlinks de `.volume`/`.network` sí se borran, pero la unidad que el generador de quadlets derivó se queda activa: en real quedó `microbin-microbin-volume.service` en `not-found / active / exited` tras un `uninstall`, y hubo que pararla a mano.
- **`--compact` no es parseable.** La ayuda dice *"Compact output format for scripts"* (`src/main.rs:75-76`), pero la implementación imprime `unit<TAB>✅<TAB>🟢` (`src/main.rs:776-777`). Un script no puede consumir eso sin limpiar emojis. El contrato y el comportamiento no coinciden.
- **No hay vista de la stack actual en `status`.** El `status` amplio (todas las unidades del target dir, de cualquier proyecto) es un **requisito deliberado y ya especificado** (`openspec/specs/services/spec.md`), no un descuido. El problema no es el default, es que **no existe forma de pedir la vista local**: desde dentro de una stack no se puede obtener el estado de *esa* stack sin filtrar a mano.

## What Changes

- **`uninstall` para también las unidades generadas de `.volume` y `.network`**, no solo las de `.container`. El nombre del unit se deriva del tipo: `<stem>.service`, `<stem>-volume.service`, `<stem>-network.service`. Los ficheros regulares (no symlink) siguen intactos y nunca se paran.
- **`--compact` pasa a emitir TSV sin decoración**: `<unit>\t<linked|missing>\t<running|stopped|->`, un renglón por unidad, sin emojis ni ANSI y sin el bloque `Summary:`. **BREAKING** para quien parseara (mal) el formato con emojis; el nuevo formato es el que la ayuda ya prometía.
- **`status` gana `--project` (opt-in)**: restringe el informe a los quadlets del proyecto actual, igual que el alcance de `start`/`stop`/`restart` sin argumentos. El default **sigue siendo la vista amplia**; `menu` no cambia. No se rompe la spec vigente.

## Capabilities

### New Capabilities

- `status-output`: el contrato de salida legible por máquina de `qctl status --compact` — separadores, tokens y ausencia de decoración.

### Modified Capabilities

- `install`: `uninstall` deja de dejar unidades generadas huérfanas para quadlets que no son `.container`.
- `services`: `status` admite un alcance de proyecto opt-in (`--project`) además del alcance amplio por defecto.

## Impact

- **Specs**: nueva `status-output`; deltas de `install` y `services`.
- **Código**: `src/main.rs` — `uninstall` (parar por extensión, no solo `container`), un helper para el nombre de unidad generado, `status` (rama `compact` reescrita a TSV) y `Commands::Status` (flag `--project` que selecciona `TargetScope::Project` frente al `Wide` actual).
- **Tests**: `tests/cli.rs` — escenarios nuevos de parada de `.volume`/`.network` en `uninstall`, de formato de `--compact` (sin emojis, TSV) y de alcance `--project` en `status`; guardas de no regresión para el `uninstall` de `.container` y para el `status` amplio por defecto.
- **Docs**: `README.md` (formato de `--compact`, `status --project`, limpieza de volúmenes/redes en `uninstall`) y `docs/AUDIT.md` (nueva ronda con los tres hallazgos).

## Out of Scope

- `uninstall <service>` y la limpieza de symlinks huérfanos cuyo origen desapareció (punto aparte, no entra).
- Guardas de colisión global en `install` (paridad con `qenable`/`qcheck`): fuera de este change.
- `uninstall --path <dir>` y descubrimiento recursivo: fuera de alcance.
- Recuento de `.volume`/`.network` en el bloque `Summary:` y columna `Tipo` en la tabla humana: no entra; el foco es la correctitud de `uninstall` y el contrato de salida.
- Un `--json` completo para `status`/`menu`: no en este change.

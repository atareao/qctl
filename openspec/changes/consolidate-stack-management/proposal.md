# Consolidar la gestión de stacks en `qctl` (guardas de colisión y `check`)

## Why

Las funciones fish `qenable`/`qdisable`/`qlist`/`qcheck` son anteriores a `qctl` y hoy
duplican lo que ya hace el binario (`install`/`uninstall`/`status`), saltándose medidas de
seguridad que el binario no tiene. Se quieren retirar para tener una única fuente de verdad.
Para hacerlo sin perder capacidades, `qctl` debe cubrir los dos huecos que solo viven en los
scripts:

- **`install` no tiene guarda de colisión global.** `qenable` hace un pre-scan atómico
  (basename del unit y `ContainerName=`) contra TODOS los stacks y aborta, sin enlazar nada,
  si hay choque. `qctl install` enlaza sin comprobar: puede colisionar con units de otro stack
  (el directorio `~/.config/containers/systemd` es plano).
- **No hay auditoría de colisiones.** `qcheck` detecta colisiones duras (basename/`ContainerName`
  entre stacks) y avisa de units fuera de la convención de nombres. `qctl check` hoy solo valida
  **un** fichero.

## What Changes

- **`install` gana una guarda de colisión global atómica**: antes de crear ningún enlace,
  recorre todos los stacks del *stacks root* buscando (a) el mismo basename de unit en otro
  stack y (b) el mismo valor de `ContainerName=` en otro quadlet. Si hay choque, aborta con
  código ≠ 0 informando de los nombres y stacks implicados, **sin crear ningún symlink**.
- **`check` se amplía**: `qctl check <QUADLET>` mantiene la validación de un fichero con el
  generador de Podman; **`qctl check` sin argumento** audita el stacks root completo — colisiones
  duras (basename/`ContainerName`) como errores y unidades fuera de la convención
  (`<stack>.<ext>` o `<stack>-<rol>`) como avisos — y sale con código ≠ 0 solo si hay colisión dura.
- **`--stacks-root <dir>`**: apunta la guarda y la auditoría a un directorio distinto del
  default `$HOME/.config/containers/available`.

## Capabilities

### New Capabilities

- `check`: contrato de la auditoría de stacks (colisiones duras y avisos de convención) y de la
  validación de un único fichero.

### Modified Capabilities

- `install`: pre-scan de colisión global atómica antes de enlazar.

## Impact

- **Specs**: nueva `check`; delta de `install`.
- **Código**: `src/main.rs` — `check` con argumento opcional + rama de auditoría; pre-scan en
  `install`; helper de descubrimiento del stacks root.
- **Tests**: `tests/cli.rs` — escenarios de aborto atómico por colisión (basename y `ContainerName`),
  de auditoría limpia/sucia y de aviso de convención sin fallo.
- **Docs**: `README.md` de `qctl`; y en el repo `self-hosted-podman` la retirada de los scripts fish.

## Out of Scope

- Descubrimiento recursivo (sigue siendo de un nivel).
- Operar "por nombre de stack desde cualquier directorio" (el flujo pasa a `cd <stack> && qctl ...`).
- `--json` para `check`.

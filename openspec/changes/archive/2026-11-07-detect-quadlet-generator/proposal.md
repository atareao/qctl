# Fix: detectar el generador `quadlet` en varios paths de distro

## Why

`qctl check <QUADLET>` invoca el generador de Podman con un path hardcodeado en
`src/main.rs` (`check_quadlet`): `/usr/lib/podman/quadlet`, salvo override con la variable
de entorno `QCTL_QUADLET_BIN`. Ese default es correcto en Arch Linux, Debian y Ubuntu, pero
Fedora, openSUSE y otros sistemas con `usrmerge` instalan el generador en
`/usr/libexec/podman/quadlet`. En esas distros, `qctl check` falla porque el binario no
existe en el default.

## What Changes

- **`check_quadlet` deja de usar un path hardcodeado**: localiza el generador con una
  función `find_quadlet_bin(candidates, env_override)` que devuelve el primer candidato que
  existe y es ejecutable, en este orden:
  1. `QCTL_QUADLET_BIN` si está definida y apunta a un binario existente/ejecutable;
  2. los candidatos de distro en orden: `/usr/libexec/podman/quadlet`, `/usr/lib/podman/quadlet`.
- **Si ninguno existe, error claro**: un mensaje que lista los paths probados y recuerda el
  override `QCTL_QUADLET_BIN`, en vez del fallo opaco de `Command::new` ("No such file").
- **La invocación no cambia**: se sigue llamando con `["-dryrun", "-user", <uid>, <quadlet>]`.
- **Tests**: unit tests de `find_quadlet_bin` (enum candidatos con ficheros temporales: env
  override gana, primer candidato existente gana, ninguno → error con paths listados);
  regresión del flujo `check` vía stub con `QCTL_QUADLET_BIN` (ya existe) y vía PATH con un
  stub en uno de los paths candidatos.

## Impact

- `qctl check` funciona en Fedora/openSUSE sin configuración adicional.
- En Arch/Debian/Ubuntu el comportamiento no cambia (el default sigue encontrando el binario).
- `QCTL_QUADLET_BIN` sigue siendo el mecanismo de override para pruebas y hosts raros.

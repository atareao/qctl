# detect-quadlet-generator

## Qué es

`qctl check <QUADLET>` valida un quadlet con el generador de Podman invocándolo como
`<bin> -dryrun -user <uid> <quadlet>`. Antes de este cambio el binario estaba *hardcodeado*
a `/usr/lib/podman/quadlet`, lo que funcionaba en Arch/Debian/Ubuntu pero fallaba en
sistemas *usrmerge* (Fedora, openSUSE) donde el generador vive en
`/usr/libexec/podman/quadlet`. Ahora `qctl` autodetecta el generador entre varios paths de
distro y conserva `QCTL_QUADLET_BIN` como override.

## Ficheros implicados

- `src/main.rs` — `is_executable_file`, `find_quadlet_bin`, `quadlet_candidates`,
  `QUADLET_BIN_CANDIDATES` y `check_quadlet`; unit tests dentro de `#[cfg(test)]`.
- `tests/cli.rs` — tests de integración de `check` (override por env y resolución real).
- `README.md` — requisitos y menciones al generador.

## Cómo funciona la resolución

`check_quadlet` construye la lista de candidatos con `quadlet_candidates()` (derivada de la
constante `QUADLET_BIN_CANDIDATES`) y llama a `find_quadlet_bin(candidates, override)`:

1. Si `QCTL_QUADLET_BIN` está definida (`std::env::var_os`, admite paths no-UTF8) y apunta a
   un fichero ejecutable, gana.
2. Si no, devuelve el primer candidato que existe **y es un fichero regular con algún bit de
   ejecución** (`mode() & 0o111`). Los directorios y los no-ejecutables se saltan.
3. Si ninguno vale, falla con un error que lista todos los paths probados (incluido el
   override roto, si lo había) y recuerda `QCTL_QUADLET_BIN`.

## Paths por distro

| Orden | Path | Distros |
|-------|------|---------|
| 1 | `/usr/libexec/podman/quadlet` | Fedora, openSUSE y otros con *usrmerge* |
| 2 | `/usr/lib/podman/quadlet` | Arch, Debian, Ubuntu |

El orden es intencional: el layout *usrmerge* se prueba primero, de modo que en un host donde
existan ambos gana el de Fedora. El contrato queda fijado en el unit test
`find_quadlet_bin_probes_the_distro_candidates_in_order`.

`is_executable_file` acepta cualquier bit de ejecución (owner, grupo u otros). Es suficiente
para binarios de sistema *world-executable*; la limitación (podría aceptar un fichero que el
usuario actual no puede ejecutar) está documentada en el propio docstring.

## Cómo probar

```sh
cargo test --bin qctl          # unit tests de find_quadlet_bin
cargo test --test cli check    # integración de `check`
# Forzar un generador concreto:
QCTL_QUADLET_BIN=/ruta/a/quadlet cargo run -- check quadlets/foo.container
```

El test de integración `check_without_override_reports_the_probed_candidate_paths` recorre la
ruta real sin override; si el host ya tiene un generador instalado, se salta (nada que
comprobar), y en CI (sin podman) verifica el error "not found" con ambos candidatos.

## Decisiones

- **Constante + helper en vez de literal embebido**: permite fijar el orden en tests y evita
  duplicar la lista entre `check_quadlet` y su documentación.
- **Override con `var_os`/`PathBuf`**: no descarta en silencio paths no-UTF8.
- **`find_quadlet_bin_from` eliminada**: se fusionó en `find_quadlet_bin`, ya que solo servía
  para componer el mensaje de error; mantenerla era indirección sin valor.
- **Sin inyección por env solo-test**: no se añade complejidad al binario de producción;
  la cobertura de la ruta real se hace con el test de integración condicionado al host.

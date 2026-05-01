# qctl

[![Rust](https://img.shields.io/badge/Rust-1.70%2B-000000?logo=rust)](https://www.rust-lang.org/)
[![Edition](https://img.shields.io/badge/Edition-2021-blue)](https://doc.rust-lang.org/edition-guide/rust-2021/index.html)
[![Platform](https://img.shields.io/badge/Platform-Linux-informational)](https://kernel.org)
[![Podman Quadlet](https://img.shields.io/badge/Podman-Quadlet-892CA0?logo=podman)](https://docs.podman.io/)

CLI en Rust para gestionar quadlets de forma simple y segura: instalación de enlaces simbólicos, control de servicios `systemd --user`, estado visual y utilidades de soporte.

## Características

- Instala y desinstala quadlets encontrados en `./quadlets/` o en el directorio actual.
- Inicia y detiene servicios de contenedor con `systemctl --user`.
- Si ejecutas `qctl start` sin haber instalado antes, instala los quadlets necesarios y luego arranca el servicio.
- Muestra estado en formato visual (tabla) o compacto (ideal para scripts).
- Limpia volúmenes de Podman declarados en archivos `.volume`.
- Ejecuta verificación de quadlets con `/usr/lib/podman/quadlet`.
- Sigue logs por servicio con `journalctl`.

## Requisitos

- Linux
- Rust toolchain (`cargo`)
- `systemctl --user`
- `podman`
- `journalctl`
- `/usr/lib/podman/quadlet`

## Instalación y compilación

Compilación de desarrollo:

```bash
cargo build
```

Compilación optimizada:

```bash
cargo build --release
```

Binario generado:

- `target/debug/qctl` (desarrollo)
- `target/release/qctl` (release)

## Uso rápido

Desde la raíz del proyecto:

```bash
cargo run -- <comando>
```

O usando binario compilado:

```bash
./target/release/qctl <comando>
```

## Comandos

```text
qctl install
qctl uninstall
qctl start [SERVICE]
qctl stop [SERVICE]
qctl restart [SERVICE]
qctl status [SERVICE] [--compact]
qctl menu [SERVICE]
qctl clean-volumes
qctl check <QUADLET>
qctl logs <SERVICE>
qctl logsf <SERVICE>
```

## Ejemplos

```bash
cargo run -- install
cargo run -- start voicebox
cargo run -- status
cargo run -- status --compact
cargo run -- menu
cargo run -- stop voicebox
cargo run -- restart voicebox
cargo run -- clean-volumes
cargo run -- check quadlets/voicebox.container
cargo run -- logs voicebox
```

`qctl start` también sirve como atajo de instalación inicial: si faltan enlaces en `$HOME/.config/containers/systemd`, ejecuta la instalación antes de hacer `start`.

## Menú interactivo

`qctl menu` muestra una tabla interactiva de servicios y permite arrancar, parar, reiniciar o consultar logs sin recordar todos los comandos.

Acciones disponibles:

- `número`: alterna entre arrancar y parar según el estado actual
- `s número` o `número s`: arranca el servicio
- `p número` o `número p`: para el servicio
- `r número` o `número r`: reinicia el servicio
- `l número` o `número l`: muestra logs recientes y vuelve al menú
- `q`: sale del menú

El menú también incluye unidades ya enlazadas en `$HOME/.config/containers/systemd`, aunque se ejecute desde otro directorio.

## Salida de status

En la tabla de `status` se usan emojis para facilitar lectura rápida:

- Link:
  - `✅` enlazado
  - `❌` faltante
- State:
  - `🟢` running
  - `🟡` stopped
  - `⚫` n/a

El resumen final también incluye estos estados con su significado.

## Estructura esperada

`qctl` busca quadlets en estos dos lugares, relativos al directorio desde el que se ejecuta:

- `./quadlets`
- `.`

Si encuentra archivos con el mismo nombre en ambos sitios, falla para evitar enlaces ambiguos. Los enlaces se crean en:

- `$HOME/.config/containers/systemd`

## Ayuda

```bash
qctl --help
qctl status --help
```

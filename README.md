# qctl

[![Rust](https://img.shields.io/badge/Rust-1.70%2B-000000?logo=rust)](https://www.rust-lang.org/)
[![Edition](https://img.shields.io/badge/Edition-2021-blue)](https://doc.rust-lang.org/edition-guide/rust-2021/index.html)
[![Platform](https://img.shields.io/badge/Platform-Linux-informational)](https://kernel.org)
[![Podman Quadlet](https://img.shields.io/badge/Podman-Quadlet-892CA0?logo=podman)](https://docs.podman.io/)

CLI en Rust para gestionar quadlets de forma simple y segura: instalación de enlaces simbólicos, control de servicios `systemd --user`, estado visual y utilidades de soporte.

## Características

- Instala y desinstala quadlets desde el directorio local `quadlets/`.
- Inicia y detiene servicios de contenedor con `systemctl --user`.
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
cargo run -- stop voicebox
cargo run -- restart voicebox
cargo run -- clean-volumes
cargo run -- check quadlets/voicebox.container
cargo run -- logs voicebox
```

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

`qctl` busca quadlets en `./quadlets` (directorio actual) y crea enlaces en:

- `$HOME/.config/containers/systemd`

## Ayuda

```bash
qctl --help
qctl status --help
```

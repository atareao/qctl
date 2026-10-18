# qctl

[![Rust](https://img.shields.io/badge/Rust-1.70%2B-000000?logo=rust)](https://www.rust-lang.org/)
[![Edition](https://img.shields.io/badge/Edition-2021-blue)](https://doc.rust-lang.org/edition-guide/rust-2021/index.html)
[![Platform](https://img.shields.io/badge/Platform-Linux-informational)](https://kernel.org)
[![Podman Quadlet](https://img.shields.io/badge/Podman-Quadlet-892CA0?logo=podman)](https://docs.podman.io/)

CLI en Rust para gestionar quadlets de forma simple y segura: instalación de enlaces simbólicos, control de servicios `systemd --user`, estado visual y utilidades de soporte.

## Características

- Instala y desinstala quadlets encontrados en `./quadlets/` o en el directorio actual.
- Actualiza las imágenes de los contenedores con `podman pull` leyendo el campo `Image=` de cada `.container`.
- Inicia y detiene servicios de contenedor con `systemctl --user`.
- `qctl start <servicio>` instala **solo** esa unidad si faltaba y la arranca; `qctl start` sin argumentos instala los quadlets que falten y arranca todos.
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

### Binario precompilado

Cada release publica un binario estático para Linux (`x86_64-unknown-linux-musl`, sin dependencias dinámicas) junto a su checksum `.sha256`:

```bash
curl -LO https://github.com/atareao/qctl/releases/latest/download/qctl-linux-x86_64
curl -LO https://github.com/atareao/qctl/releases/latest/download/qctl-linux-x86_64.sha256
sha256sum -c qctl-linux-x86_64.sha256
chmod +x qctl-linux-x86_64
sudo install -m 755 qctl-linux-x86_64 /usr/local/bin/qctl
qctl --version
```

### Compilación desde el código

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
qctl reinstall
qctl start [SERVICE]
qctl stop [SERVICE]
qctl restart [SERVICE]
qctl update [SERVICE]
qctl status [SERVICE] [--compact]
qctl menu [SERVICE]
qctl clean-volumes [--yes]
qctl check <QUADLET>
qctl logs <SERVICE>
qctl logsf <SERVICE>
qctl completions <SHELL>
```

`--dry-run` afecta a los comandos que modifican el sistema o invocan herramientas externas: `install`, `uninstall`, `reinstall`, `start`, `stop`, `restart`, `update`, `reload` y `clean-volumes`. Estos imprimen las acciones planificadas como `DRY-RUN ...` sin cambiar archivos ni ejecutar acciones mutadoras; la tabla de estado final sí consulta `systemctl --user is-active`:

```bash
qctl --dry-run install
qctl --dry-run start voicebox
qctl --dry-run clean-volumes
```

Los comandos de solo consulta (`status`, `logs`, `logsf`, `check`, `completions`) siguen consultando el sistema aunque se use `--dry-run`, porque son de lectura y su utilidad desaparecería al simularlos. `qctl menu --dry-run` también sigue consultando para dibujar la tabla, pero las acciones que mutan (`start`, `stop`, `restart`) respetan `--dry-run`.

### Nombres de servicio y seguridad del directorio destino

Los comandos que actúan sobre unidades (`start`, `stop`, `restart`, `update`) validan el nombre antes de tocar nada: si no corresponde a ningún quadlet, fallan con `no such quadlet: <nombre>` y código de salida distinto de cero. Un nombre se acepta si existe `<origen>/<nombre>.container` **o** `<destino>/<nombre>.container`; lo segundo cubre unidades ya instaladas cuyo origen se borró.

`status`, `check`, `logs` y `logsf` no validan nombres: son consultas y se limitan a mostrar lo que exista (o el error de `systemctl`).

`qctl` nunca borra ni reemplaza **ficheros regulares** del directorio destino: solo elimina symlinks. Si encuentra un fichero regular con el mismo nombre, avisa por stderr y lo deja intacto. Esto cubre además ejecutar `qctl` desde dentro del propio `$HOME/.config/containers/systemd` (con un `quadlets/` anidado): ya no crea auto-symlinks y se recupera de los que hubieran dejado versiones anteriores.

## Ejemplos

```bash
cargo run -- install
cargo run -- start voicebox
cargo run -- status
cargo run -- status --compact
cargo run -- menu
cargo run -- stop voicebox
cargo run -- restart voicebox
cargo run -- update
cargo run -- update voicebox
cargo run -- clean-volumes
cargo run -- clean-volumes --yes
cargo run -- check quadlets/voicebox.container
cargo run -- logs voicebox
cargo run -- logsf voicebox
```

`qctl start` también sirve como atajo de instalación inicial. Con un servicio, instala **solo** esa unidad si faltaba en `$HOME/.config/containers/systemd` y la arranca; sin argumentos, instala los quadlets que falten y arranca todos.

## Completions de shell

Genera e instala completions para tu shell:

```bash
# Fish
qctl completions fish > ~/.config/fish/completions/qctl.fish

# Bash
qctl completions bash > ~/.local/share/bash-completion/completions/qctl

# Zsh
qctl completions zsh > "${fpath[1]}/_qctl"
```

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

## Limpieza de volúmenes

`qctl clean-volumes` elimina los volúmenes de Podman declarados con `VolumeName=` en archivos `.volume`. Por defecto pide confirmación antes de borrar. Usa `--yes` para confirmar desde scripts, o `--dry-run` para ver qué volúmenes se borrarían.

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

Con `--compact`, la salida contiene solo una línea por unidad en formato tabulado:

```text
<unit>	<link>	<state>
```

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

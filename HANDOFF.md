# HANDOFF — qctl

## Estado (2026-11-09)

Fecha: 2026-11-09.

- **Release v0.7.1 (patch): PUBLICADO y DESPLEGADO.**
  - Rama `release/0.7.1` desde `develop`; bump `0.7.0 → 0.7.1` (`vampus upgrade --patch`, `.vampus.yml` + `Cargo.toml`). PR **#34** → `main`.
  - **Tag `v0.7.1`** + merge-back a `develop` hechos (`git flow release finish`). GitHub Release `v0.7.1` → `release.yml` verde → assets `qctl-linux-x86_64` + `.sha256`.
  - Contenido: 🐛 fix(purge) `--data` resuelve symlinks antes de borrar (issue #31, PR #33).
  - **Despliegue**: Local `cargo install --path . --force` → `~/.cargo/bin/qctl` **0.7.1**. Producción `new_atareao` (usuario `runner`, vía `rr`): asset descargado, `sha256` verificado, `install -m 0755` → `~/.local/bin/qctl` **0.7.1**; backup `~/qctl.bak-0.7.0`. Verificado `qctl --version` + `purge --data` presente.

## Detalle técnico del cambio (doble barrera en `--data`)

- `resolve_bind_source` (léxica) + `resolve_real_bind` (`fs::canonicalize` + contención estricta contra `$HOME` canónico).
- Devuelve `BindReal::{Accepted, Escapes, Unresolvable}`; se borra el **path real resuelto**.
- Fail-closed si `$HOME` no canonicaliza.

## Wrapper `rr` (acceso a producción)

Para operar el stack en `new_atareao` se usa el wrapper **`rr`** (dos copias cooperantes):

- **Remoto** `~/bin/rr` en `lorenzo` (servidor): `sudo -n -u runner -H env XDG_RUNTIME_DIR=/run/user/1002 PATH=/usr/local/bin:/usr/bin:/home/runner/.local/bin SOPS_AGE_KEY_FILE=/home/runner/.secrets/sops/age/key.txt bash -lc "cd && <cmd>"`. Aporta el entorno completo (systemd user bus + crypta/sops + `~/.local/bin`) y fuerza bash (el login shell de `runner` es fish).
- **Local** `~/bin/rr` en la máquina que inicia la sesión: envía el comando al remoto por **base64** (protege del quoting de fish, local y remoto) y lo ejecuta a través del `rr` remoto.

Uso:

```bash
rr                          # shell interactivo de runner (pide TTY)
rr "podman ps"              # comando como runner, sin TTY
rr "crypta lookup oidc_client_id"   # secretos SOPS+age
```

Reglas: nunca SSH directo como `runner`; siempre `lorenzo` → `rr`. `runner` no tiene sudo; `lorenzo` (grupo sudo) es el humano que eleva. Implementado y documentado (README.md, AGENTS.md, setup_apps.sh) en el repo `self-hosted-podman`.

## Tarea pendiente

**Ninguna en `qctl`.** Cero issues abiertos.

## Reglas / notas

- Gitflow: PR a `develop`; PR a `main` solo desde `release/*`/`hotfix/*` (branch-guard). Commits gitmoji. `Cargo.lock` ignorado.
- **`.vampus.yml` fuente de verdad de versión** (editar ahí + propagar Cargo.toml); `git flow release start/publish/finish`.
- Escritor fiable: **`backend-dev`** (el `plan-writer` no dispone de `write`). NO pasar el parámetro `model` a los subagentes.
- **`host new_atareao` = Ubuntu 26.04 (vmi3651091)**: producción bajo el usuario **`runner`** (uid 1002, antes `podman`), shell fish, yadm + stacks root (`~/.config/containers/available`, **128** stacks) + `systemd` user en `/home/runner`. qctl en `~runner/.local/bin/qctl` (0.7.0).
- Comando de acceso rápido: **`rr "<cmd>"`** (wrapper, ver arriba) — `XDG_RUNTIME_DIR=/run/user/1002`, `SOPS_AGE_KEY_FILE=~/.secrets/sops/age/key.txt`, `PATH` con `~/.local/bin` ya exportados.
- Para instalar ficheros como runner en /tmp ajeno: usar `install -m 0755` (cp), nunca `mv` (sticky bit).

# HANDOFF — qctl

## Estado (2026-11-07)

- **Release v0.6.2 (patch): PUBLICADO.**
  - Rama `release/0.6.2` desde `develop` (PR **#28** → `main`); bump 0.6.1 → 0.6.2 (.vampus.yml + Cargo.toml).
  - Contenido: 🐛 fix(check) detección multi-path del generador quadlet (PR #27).
  - GitHub Release `v0.6.2` → workflow verde → assets `qctl-linux-x86_64` + `.sha256`. Merge-back a `develop` hecho.
- **Cambio `detect-quadlet-generator`: COMPLETADO y ARCHIVADO.** PR **#27** → `develop`; auditado (@rust-reviewer: NO APTO inicial → cerrados, 16 unit + 86 integración); archivado.
- **`new_atareao` ACTUALIZADO — usuario `runner` (uid 1002), ya NO `podman` (PLAN-008 COMPLETADO y ARCHIVADO).**
  - Rename `podman` → `runner` ejecutado: `usermod -l runner podman` + `usermod -d /home/runner -m runner`; UID 1002 constante.
  - subuid/subgid reconfigurados a mano; 7 symlinks de systemd relinkeados (ROTOS=0); `db.sql` de podman migrado a `/home/runner`; linger migrado; 4 volúmenes intactos.
  - **traefik**: reparado (secreto `cf_dns_api_token` del driver file apuntaba a `/home/podman/.../filedriver` en `secrets.json`). Up healthy.
  - **pocketid**: clave de cifrado del `kv` irrecuperable → regeneradas `jwt_private_key.json`/`session_key.json` (backup DB `.PLAN008.bak`). Up healthy. Admin `atareao` re-autenticado con passkey; datos preservados.
  - **errserve**: healthy.
  - Verificado: `qctl check` 0.6.2 exit 0 (2 WARN no bloqueantes de naming); `https://auth.servidorlinux.es/` → 200; los 3 contenedores healthy vía systemd user.
  - yadm operativo (`core.worktree` corregido a `/home/runner`); subuid/subgid/DB/secrets sin rutas viejas.

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

- **Ninguna en `qctl`.** Comando `qctl purge` (issue #29) MERGEADO en `develop` (PR #30, squash). Cambio OpenSpec `purge-command` ARCHIVADO en `openspec/specs/purge/`; `plans/PLAN-003-ARCHIVED.md`.
- **Issue #29 cerrado.**
- Resto: `develop` por delante de `main` (v0.6.2) — `purge` irá en el próximo release.
- **Handoff cruzado `self-hosted-podman`: PLAN-008 CERRADO y docs publicadas.** Queda solo el WARN no bloqueante de naming (`proxy.network` en stack traefik y `security.network` en vector) del `qctl check`, para una sesión futura de limpieza — no es un bloqueo.

## Verificación
`cargo test` (16 unit + 86 integración) · fmt/clippy/validate verdes · release workflow verde.

## Reglas / notas
- Gitflow: PR a `develop`; PR a `main` solo desde `release/*`/`hotfix/*` (branch-guard). Commits gitmoji. `Cargo.lock` ignorado.
- **`.vampus.yml` fuente de verdad de versión** (editar ahí + propagar Cargo.toml); `git flow release start/publish/finish`.
- Escritor fiable: **`backend-dev`** (el `plan-writer` no dispone de `write`). NO pasar el parámetro `model` a los subagentes.
- **`host new_atareao` = Ubuntu 26.04 (vmi3651091)**: producción bajo el usuario **`runner`** (uid 1002, antes `podman`), shell fish, yadm + stacks root (`~/.config/containers/available`, **128** stacks) + `systemd` user en `/home/runner`. qctl en `~runner/.local/bin/qctl` (0.6.2).
- Comando de acceso rápido: **`rr "<cmd>"`** (wrapper, ver arriba) — `XDG_RUNTIME_DIR=/run/user/1002`, `SOPS_AGE_KEY_FILE=~/.secrets/sops/age/key.txt`, `PATH` con `~/.local/bin` ya exportados.
- Para instalar ficheros como runner en /tmp ajeno: usar `install -m 0755` (cp), nunca `mv` (sticky bit).

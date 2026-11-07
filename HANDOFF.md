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
- **Releases 0.6.0/0.6.1**: publicados antes; `main` == `develop` en v0.6.2.

## Tarea pendiente

- **Ninguna en `qctl`.** Cero issues abiertas; `develop` == `main` en v0.6.2.
- **Handoff cruzado → `self-hosted-podman` (siguiente sesión):**
  - Reconciliar el repo yadm del host `new_atareao` (en `ca217bb`) con `origin/main` (en GitHub en
    `9c34c83` + últimos commits de docs/PLAN-008): el repo yadm tiene commits de docs(gestion-stacks)/
    PLAN-003 sin empujar (divergencia pre-existente). Hacer `yadm pull` y resolver SI hay conflicto;
    NO force-push.
  - Recordar: los helpers fish `{qenable,qdisable,qlist,qcheck}.fish` ya fueron retirados en el commit
    `f59e0d4` del repo yadm (local, sin empujar) — verificar coherencia con la rama `main` de GitHub.
  - Revisar `docs/rename-usuario-produccion.md` (nueva doc del PLAN-008) y `AGENTS.md` del repo por
    referencias al usuario `podman`/ruta `/home/podman` caducadas.

## Verificación
`cargo test` (16 unit + 86 integración) · fmt/clippy/validate verdes · release workflow verde.

## Reglas / notas
- Gitflow: PR a `develop`; PR a `main` solo desde `release/*`/`hotfix/*` (branch-guard). Commits gitmoji. `Cargo.lock` ignorado.
- **`.vampus.yml` fuente de verdad de versión** (editar ahí + propagar Cargo.toml); `git flow release start/publish/finish`.
- Escritor fiable: **`backend-dev`** (el `plan-writer` no dispone de `write`). NO pasar el parámetro `model` a los subagentes.
- **`host new_atareao` = Ubuntu 26.04 (vmi3651091)**: producción bajo el usuario **`runner`** (uid 1002, antes `podman`), shell fish, yadm + stacks root (`~/.config/containers/available`, 124 funciones) + `systemd` user en `/home/runner`. qctl en `~runner/.local/bin/qctl` (0.6.2).
- Para la sesión remota como runner: `sudo -n -u runner -H bash -lc "..."` exportando `XDG_RUNTIME_DIR=/run/user/1002`, `SOPS_AGE_KEY_FILE=~/.secrets/sops/age/key.txt` y `PATH=/usr/local/bin:/usr/bin:$HOME/.local/bin`.
- Para instalar ficheros como runner en /tmp ajeno: usar `install -m 0755` (cp), nunca `mv` (sticky bit).

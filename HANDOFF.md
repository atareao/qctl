# HANDOFF — qctl

## Estado (2026-11-07)

- **Release v0.6.1 (patch): PUBLICADO y COMPLETO.**
  - Rama `release/0.6.1` desde `develop` (PR **#26** → `main`, merge commit `8785824`).
  - Contenido: `⬆️ deps` comfy-table 8 (PR #22) + `🐛 fix` unidades con stems con puntos (PR #25).
  - Bump 0.6.0 → 0.6.1 en `.vampus.yml` (fuente de verdad, quedó desincronizado en 0.5.0 desde el release 0.6.0 — **corregido**) y `Cargo.toml`.
  - GitHub Release `v0.6.1` → `release.yml` verde → assets `qctl-linux-x86_64` + `.sha256` subidos.
  - Merge-back a `develop` hecho y pusheado (`develop` == `main` en `8785824`).
  - Rama `release/0.6.0` huérfana local eliminada; `release/0.6.1` borrada tras el merge.
- **Issues #20 y #21: cerradas.** Cero issues abiertas en el repo.
- **Changes archivados:** `consolidate-stack-management`, `migrate-comfy-table-8`, `fix-systemd-unit-dots`.

## Tarea pendiente

- **Ninguna en `qctl`.** `develop` y `main` sincronizadas en v0.6.1.
- **Handoff cruzado → `self-hosted-podman`** (PLAN-003, tareas 5-6): con qctl ≥0.6.0 disponible, borrar los helpers fish `{qenable,qdisable,qlist,qcheck}.fish` y actualizar `AGENTS.md`/`README.md`/`docs/errserve.md`. Requiere **sesión nueva en el repo dotfiles**.

## Verificación
`cargo test` (9 unit + 84 integración) · fmt/clippy/audit verdes (CI en el PR #26) · release workflow verde.

## Reglas / notas
- Gitflow: PR a `develop`; PR a `main` solo desde `release/*`/`hotfix/*` (branch-guard). Commits gitmoji. `Cargo.lock` ignorado.
- **`.vampus.yml` es la fuente de verdad de la versión**: al hacer release, editar `current_version` AHÍ y propagar a `Cargo.toml` (no solo Cargo.toml, como pasó en 0.6.0). Usar `git flow release start/publish/finish`.
- Escritor fiable: **`backend-dev`** (el `plan-writer` no dispone de `write` — no usarlo para escribir ficheros). NO pasar el parámetro `model` a los subagentes.
- `install` con guarda de colisión global atómica; `check` sin args = auditoría del stacks root; `--stacks-root`; `QCTL_QUADLET_BIN`.

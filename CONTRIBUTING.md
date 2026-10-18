# Contribuir a qctl

Guía operativa del repositorio: cómo preparar el entorno, qué modelo de ramas se usa y cómo entrar y salir de cada tipo de rama sin romper el historial.

## Requisitos

- Rust estable (`rustup` recomendado) con `cargo`, `rustfmt` y `clippy`.
- `podman` y `systemctl --user` para probar la herramienta de verdad.
- [`git-flow`](https://github.com/petervanderdoes/gitflow-avh) (AVH). Probado con la versión **1.12.3**.

En un clon nuevo, inicializa git-flow para que use `main` como rama de producción y `develop` como rama de integración:

```bash
git config gitflow.branch.master main
git config gitflow.branch.develop develop
git flow init -f -d
```

`-d` acepta todos los valores por defecto (prefijos `feature/`, `release/`, `hotfix/`, `support/` y tag `v`); `-f` evita que tenga que preguntar.

## Modelo de ramas

| Rama | Propósito | Entra / sale |
| --- | --- | --- |
| `main` | Producción. Solo contiene versiones publicadas. | Solo recibe merges de `release/*` y `hotfix/*`. Nunca se commitea directamente. |
| `develop` | Integración. Refleja el estado del próximo release. | Destino de todos los PRs de `feature/*`. Recibe merge-back de `release/*` y `hotfix/*`. |
| `feature/*` | Trabajo nuevo o cambios que no son urgentes. | Sale de `develop`, vuelve a `develop` vía PR. |
| `release/*` | Preparación de una versión: bump de versión y últimos ajustes. | Sale de `develop`, vuelve a `main` vía PR y deja merge-back a `develop` con `release finish`. |
| `hotfix/*` | Corrección urgente sobre producción. | Sale de `main`, vuelve a `main` vía PR y deja merge-back a `develop` con `hotfix finish`. |

Esta regla no es solo una convención: el workflow [`branch-guard.yml`](.github/workflows/branch-guard.yml) rechaza cualquier PR hacia `main` cuyo origen no sea `release/*` o `hotfix/*`. En gitflow estricto `develop` nunca se mergea directamente a `main`: se abre una rama `release/X.Y.Z`.

## Commits

Se usa [Conventional Commits](https://www.conventionalcommits.org/). El mensaje va en inglés, en imperativo y con el tipo como prefijo:

| Tipo | Cuándo |
| --- | --- |
| `feat` | Funcionalidad nueva. |
| `fix` | Corrección de un fallo. |
| `docs` | Solo documentación. |
| `ci` | Cambios en workflows o automatización. |
| `refactor` | Cambio de código que no altera el comportamiento. |
| `test` | Altas o ajustes de tests. |
| `chore(release)` | Bump de versión en una rama de release o hotfix. |

Ejemplo: `feat: add --dry-run to clean-volumes`.

## Flujo de feature

```bash
git checkout develop
git pull --ff-only
git flow feature start mi-cosa
# … commits …
git flow feature publish mi-cosa      # push + seguimiento
gh pr create --base develop --fill
git flow feature finish mi-cosa        # tras el merge del PR
```

Antes de pedir revisión, la CI tiene que estar en verde: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo audit`, `cargo build --verbose` y `cargo test --verbose`. Localmente:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Flujo de release

```bash
git checkout develop
git pull --ff-only
git flow release start 0.3.0
# subir .vampus.yml (current_version) y Cargo.toml
# commit: chore(release): bump version to 0.3.0
git flow release publish 0.3.0
gh pr create --base main --head release/0.3.0 --fill
# tras el merge:
git flow release finish -m "v0.3.0" 0.3.0
git push origin main --tags
gh release create v0.3.0 --target main --title v0.3.0 --generate-notes
```

El PR sale de `release/0.3.0`, nunca de `develop`: un merge `develop → main` lo bloquea [`branch-guard.yml`](.github/workflows/branch-guard.yml) a propósito.

Tres detalles que se olvidan:

1. **`.vampus.yml` es la fuente de verdad de la versión.** Se edita `current_version` ahí y la herramienta propaga el valor a `Cargo.toml` según el patrón declarado en `replaces`. No cambies la versión solo en `Cargo.toml`, o el siguiente bump la pisará.
2. **`release finish` crea el tag y hace merge-back a `develop`.** El merge-back es obligatorio: sin él `develop` se queda por detrás de `main` y el siguiente release arrastra commits fantasma o conflictos artificiales.
3. **Crear el GitHub Release dispara [`release.yml`](.github/workflows/release.yml).** Compila el binario estático para `x86_64-unknown-linux-musl`, genera el `.sha256` y sube los assets. Comprueba que el workflow termina en verde antes de dar la versión por publicada.
4. **Si `release finish` aborta con `Branches 'main' and 'origin/main' have diverged`**, es que `main` local se quedó desfasado tras el merge del PR. Se arregla con `git fetch origin main:main` y repitiendo el `finish`.

## Flujo de hotfix

```bash
git checkout main
git pull --ff-only
git flow hotfix start 0.2.3
# subir .vampus.yml (current_version) y Cargo.toml
# commit: chore(release): bump version to 0.2.3
gh pr create --base main --head hotfix/0.2.3 --fill
# tras el merge:
git flow hotfix finish -m "v0.2.3" 0.2.3
git push origin main --tags
gh release create v0.2.3 --target main --title v0.2.3 --generate-notes
```

`hotfix finish` hace merge a `main`, crea el tag `vX.Y.Z` y hace merge-back a `develop`. Igual que en un release, el merge-back no es opcional y el `gh release create` dispara `release.yml`.

## Qué corre la CI y cuándo

| Workflow | Se dispara con | Qué hace |
| --- | --- | --- |
| `rust.yml` | push a `main`, `develop`, `release/**`, `hotfix/**`; PRs hacia `main` y `develop` | `fmt`, `clippy -D warnings`, `cargo audit`, build y tests. |
| `branch-guard.yml` | PRs hacia `main` (opened, edited, synchronize, reopened) | Rechaza orígenes distintos de `release/*` o `hotfix/*`. |
| `release.yml` | Creación de un GitHub Release | Compila el binario musl estático, calcula el checksum y sube los assets. |

## Ajustes recomendados del repositorio

No se pueden imponer desde el propio repositorio; hay que configurarlos en GitHub (Settings → Branches):

- Proteger `main` y `develop`: exigir PR y que los checks de CI estén en verde antes de mergear.
- Prohibir force-push en `main` y `develop`.

## Más información

- [README.md](README.md) — uso, instalación y características de `qctl`.
- [docs/AUDIT.md](docs/AUDIT.md) — auditoría de seguridad.

# Tasks

## 1. Tests primero (RED)

- [x] 1.1 `install_aborts_on_basename_collision`: dos stacks con `x.container`; `install` desde el segundo no crea enlaces y sale ≠ 0.
- [x] 1.2 `install_aborts_on_containername_collision`: dos quadlets con `ContainerName=foo`; aborto atómico.
- [x] 1.3 `install_without_collision_links_normally`: guarda de no regresión del flujo normal.
- [x] 1.4 `check_no_args_clean_stacks_root_exits_zero`.
- [x] 1.5 `check_no_args_hard_collision_exits_nonzero`.
- [x] 1.6 `check_no_args_naming_warning_does_not_fail`.
- [x] 1.7 `check_with_arg_still_validates_single_quadlet` (no regresión).

## 2. Implementación (GREEN)

- [x] 2.1 Helper de descubrimiento del stacks root (`$HOME/.config/containers/available` por defecto; override con `--stacks-root`).
- [x] 2.2 Guarda de colisión global atómica en `install`.
- [x] 2.3 `Commands::Check`: argumento opcional; rama global de auditoría (colisiones + convención).

## 3. Refactor y documentación

- [x] 3.1 `cargo fmt --all -- --check` y `cargo clippy --all-targets --all-features -- -D warnings` limpios.
- [x] 3.2 `README.md` de `qctl`: nueva semántica de `check` y `--stacks-root`.

## 4. Verificación y cierre

- [x] 4.1 `openspec validate consolidate-stack-management --strict` en verde.
- [x] 4.2 `cargo test` completo en verde.
- [x] 4.3 Auditoría con `@rust-reviewer`.
- [x] 4.4 PR a `develop`, merge y `openspec archive consolidate-stack-management`.

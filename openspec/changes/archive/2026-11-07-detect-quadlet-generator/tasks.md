# Tasks

## 1. Tests primero (RED)

- [x] 1.1 Unit: `find_quadlet_bin` usa el override de env cuando existe/ejecutable.
- [x] 1.2 Unit: `find_quadlet_bin` devuelve el primer candidato existente/ejecutable.
- [x] 1.3 Unit: `find_quadlet_bin` falla con error que lista los paths probados y menciona `QCTL_QUADLET_BIN`.
- [x] 1.4 Integración: `check` con `QCTL_QUADLET_BIN` (regresión, ya existe).
- [x] 1.5 Integración: `check` sin env var recorre la ruta real de resolución por candidato (`check_without_override_reports_the_probed_candidate_paths`); el orden queda fijado por el unit test `find_quadlet_bin_probes_the_distro_candidates_in_order` sobre `QUADLET_BIN_CANDIDATES`.

## 2. Implementación (GREEN)

- [x] 2.1 `find_quadlet_bin(candidates, env_override)` con `std::fs::metadata` (existe + ejecutable).
- [x] 2.2 `check_quadlet` usa la lista de distro (`/usr/libexec/podman/quadlet`, `/usr/lib/podman/quadlet`) y el error claro.
- [x] 2.3 Docstring documentando el mecanismo y los paths por distro.

## 3. Refactor y documentación

- [x] 3.1 `cargo fmt --all -- --check` y `cargo clippy --all-targets --all-features -- -D warnings` limpios.
- [x] 3.2 README/docs actualizados con la detección multi-path y el override.

## 4. Verificación y cierre

- [ ] 4.1 `openspec validate detect-quadlet-generator --strict` en verde.
- [ ] 4.2 `cargo test` completo en verde.
- [ ] 4.3 Auditoría con `@rust-reviewer`.
- [ ] 4.4 PR a `develop`, merge y `openspec archive detect-quadlet-generator`.

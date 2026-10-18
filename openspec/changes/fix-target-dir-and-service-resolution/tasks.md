# Tasks

## 1. Tests primero (RED)

- [x] 1.1 `install_skips_quadlets_in_subdirectories_of_target_dir`: cwd = target dir con `quadlets/other.container`; dos `install` seguidos terminan OK y no dejan symlinks dentro del target.
- [x] 1.2 `install_recovers_from_stale_self_link`: symlink `other.container -> quadlets/other.container` preexistente; `install` y `uninstall` terminan OK.
- [x] 1.3 `duplicate_quadlet_names_fail_with_clear_error` sigue en verde (duplicado real).
- [x] 1.4 `uninstall_preserves_regular_files_in_target_dir`: fichero regular `one.container` en el target; `uninstall` no lo borra.
- [x] 1.5 `install_does_not_replace_regular_file_in_target_dir`: fichero regular en el target con el mismo nombre que un quadlet del proyecto; `install` avisa por stderr, no lo toca y enlaza el resto.
- [x] 1.6 `start_unknown_service_fails_without_installing`: `start typo` sale != 0, mensaje `no such quadlet: typo`, target dir vacío, `systemctl` no invocado.
- [x] 1.7 `stop_and_restart_unknown_service_fail`.
- [x] 1.8 `update_unknown_service_fails`: sale != 0 con `no such quadlet: typo`.
- [x] 1.9 `start_service_links_only_that_unit`: con dos cuadlets sin instalar, `start one` enlaza solo `one.container`.
- [ ] 1.10 Ajustar los tests existentes que asuman el contrato viejo (`update` con nombre inexistente saliendo con 0).

## 2. Implementación (GREEN)

- [x] 2.1 `source_is_in_target_dir`: ancestro (`starts_with`) sobre rutas canónicas.
- [x] 2.2 `collect_quadlets`: colapsar colisiones cuyo canónico coincide; error solo si difieren.
- [x] 2.3 `link_or_replace`: no reemplazar ficheros regulares; aviso por stderr y `continue`.
- [x] 2.4 `uninstall`: borrar solo symlinks; avisar y saltar ficheros regulares.
- [x] 2.5 `resolve_targets`: validar el servicio contra source y target; `no such quadlet: <name>`.
- [x] 2.6 `start`: instalar solo la unidad pedida; error si no queda enlazada.
- [x] 2.7 `update`: usar la misma validación.

## 3. Refactor y documentación

- [x] 3.1 Extraer el helper de validación para que no se duplique entre `start` y `update`.
- [x] 3.2 `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`.
- [x] 3.3 Actualizar `README.md` (contrato de `start <servicio>` y de los nombres inválidos).
- [x] 3.4 Actualizar `docs/AUDIT.md` y el recuento de tests en `AGENTS.md`.
- [x] 3.5 Nota en `CONTRIBUTING.md`: `git fetch origin main:main` antes de `git flow release finish`.

## 4. Release

- [ ] 4.1 `openspec validate fix-target-dir-and-service-resolution --strict`.
- [ ] 4.2 `openspec archive fix-target-dir-and-service-resolution` tras el merge.
- [ ] 4.3 `release/0.3.0` con el ritual de `CONTRIBUTING.md`.

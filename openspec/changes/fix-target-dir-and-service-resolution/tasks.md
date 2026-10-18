# Tasks

## 1. Tests primero (RED)

- [ ] 1.1 `install_skips_quadlets_in_subdirectories_of_target_dir`: cwd = target dir con `quadlets/other.container`; dos `install` seguidos terminan OK y no dejan symlinks dentro del target.
- [ ] 1.2 `install_recovers_from_stale_self_link`: symlink `other.container -> quadlets/other.container` preexistente; `install` y `uninstall` terminan OK.
- [ ] 1.3 `duplicate_quadlet_names_fail_with_clear_error` sigue en verde (duplicado real).
- [ ] 1.4 `uninstall_preserves_regular_files_in_target_dir`: fichero regular `one.container` en el target; `uninstall` no lo borra.
- [ ] 1.5 `install_does_not_replace_regular_file_in_target_dir`: fichero regular en el target con el mismo nombre que un quadlet del proyecto; `install` avisa por stderr, no lo toca y enlaza el resto.
- [ ] 1.6 `start_unknown_service_fails_without_installing`: `start typo` sale != 0, mensaje `no such quadlet: typo`, target dir vacío, `systemctl` no invocado.
- [ ] 1.7 `stop_and_restart_unknown_service_fail`.
- [ ] 1.8 `update_unknown_service_fails`: sale != 0 con `no such quadlet: typo`.
- [ ] 1.9 `start_service_links_only_that_unit`: con dos cuadlets sin instalar, `start one` enlaza solo `one.container`.
- [ ] 1.10 Ajustar los tests existentes que asuman el contrato viejo (`update` con nombre inexistente saliendo con 0).

## 2. Implementación (GREEN)

- [ ] 2.1 `source_is_in_target_dir`: ancestro (`starts_with`) sobre rutas canónicas.
- [ ] 2.2 `collect_quadlets`: colapsar colisiones cuyo canónico coincide; error solo si difieren.
- [ ] 2.3 `link_or_replace`: no reemplazar ficheros regulares; aviso por stderr y `continue`.
- [ ] 2.4 `uninstall`: borrar solo symlinks; avisar y saltar ficheros regulares.
- [ ] 2.5 `resolve_targets`: validar el servicio contra source y target; `no such quadlet: <name>`.
- [ ] 2.6 `start`: instalar solo la unidad pedida; error si no queda enlazada.
- [ ] 2.7 `update`: usar la misma validación.

## 3. Refactor y documentación

- [ ] 3.1 Extraer el helper de validación para que no se duplique entre `start` y `update`.
- [ ] 3.2 `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`.
- [ ] 3.3 Actualizar `README.md` (contrato de `start <servicio>` y de los nombres inválidos).
- [ ] 3.4 Actualizar `docs/AUDIT.md` y el recuento de tests en `AGENTS.md`.
- [ ] 3.5 Nota en `CONTRIBUTING.md`: `git fetch origin main:main` antes de `git flow release finish`.

## 4. Release

- [ ] 4.1 `openspec validate fix-target-dir-and-service-resolution --strict`.
- [ ] 4.2 `openspec archive fix-target-dir-and-service-resolution` tras el merge.
- [ ] 4.3 `release/0.3.0` con el ritual de `CONTRIBUTING.md`.

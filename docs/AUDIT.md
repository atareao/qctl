# Audit Report

Date: 2026-05-06

## Scope

This audit covered the `qctl` Rust CLI, its destructive operations, command execution behavior, dependency tree, tests, and CI/release workflows.

Reviewed surfaces:

- Symlink creation and removal in `$HOME/.config/containers/systemd`
- Podman volume removal through `clean-volumes`
- `systemctl --user`, `journalctl`, and `/usr/lib/podman/quadlet` command execution
- CLI argument behavior and service name normalization
- Quadlet discovery and duplicate handling
- CI and release workflows
- Cargo dependency graph and RustSec advisories

## Findings Fixed

- `start`, `stop`, and `restart` no longer hide `systemctl` failures.
- `start`, `stop`, and `restart` now wait for every unit and report an aggregated error that names each failing unit.
- External command failures now include command, exit status, and stderr.
- `clean-volumes` now requires confirmation by default and supports `--yes` for scripts.
- A global `--dry-run` mode now reports planned actions for commands that mutate the system or invoke external tools (`install`, `uninstall`, `reinstall`, `start`, `stop`, `restart`, `update`, `reload`, `clean-volumes`) without changing files or running mutating actions; the final status table still queries `systemctl --user is-active`. Read-only commands (`status`, `logs`, `logsf`, `check`) still query the system when `--dry-run` is used.
- `status --compact` now emits only compact script-friendly rows.
- Service name normalization now removes only the final extension.
- CI now runs formatting, clippy, dependency audit, build, and tests.
- Release workflow now runs formatting, clippy, and tests before publishing assets.
- The merge resolution reintroduced the `logsf` subcommand (lost when `develop` dropped the enum variant) and added `--dry-run` support to `update`.
- `uninstall` keeps tolerating a stop failure for an already-stopped unit, but now warns with the underlying cause instead of silencing it. Top-level commands propagate command failures.

## Verification

Commands executed:

```bash
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --verbose
cargo tree -d
env CARGO_HOME=/tmp/qctl-cargo-audit-home cargo audit
git diff --check
```

Results:

- Unit tests: 6 passed
- Integration tests: 20 in this 2026-05-06 round (not the current count; see the Round 2026-10-18 section for the total)
- Clippy: passed with `-D warnings`
- Formatting: passed
- Build: passed
- Duplicate dependency check: no duplicate dependencies reported
- RustSec audit: 70 crate dependencies scanned, no vulnerabilities reported

Note: `cargo build --verbose` may print a Cargo cache warning in this sandbox because the global Cargo last-use database is read-only. The project build itself completes successfully.

## Residual Risk

- `menu` remains interactive; only its EOF/termination behavior is covered by an automated integration test.
- `logs` and `logsf` stream `journalctl`; tests assert their argument sets but do not exercise long-running follow mode.
- `check` depends on `/usr/lib/podman/quadlet`; it is not integration-tested because the binary path is host-specific.
- `clean-volumes --yes` is intentionally destructive. The confirmation and `--dry-run` guard reduce accidental use, but scripts must still use it carefully.

## Recommended Next Steps

- Add `--source` and `--target` flags to make audits and tests less dependent on current directory and `$HOME`.
- Add a `doctor` command to validate required host tools before running operational commands.
- Add integration tests for `check` through a configurable quadlet binary path if that command needs stronger automated coverage.

## Round 2026-10-18 — Target directory and service resolution

Fixes for two bugs found while reviewing PR #2, proposed and validated through the
OpenSpec change `fix-target-dir-and-service-resolution`.

Findings fixed:

- **#3 — quadlets already inside the target directory.** `install` compared the
  source's direct parent with the target directory, so a project nested inside
  `$HOME/.config/containers/systemd` (for example a `quadlets/` subdirectory)
  created a symlink pointing back into the target directory; the second run then
  failed with `duplicate quadlet file name`, leaving `install`, `reinstall` and
  `uninstall` broken until the stray link was removed by hand. The check now uses
  canonical-path ancestry, and `collect_quadlets` collapses redundant entries that
  resolve to the same file (preferring the real file) while still rejecting
  genuine duplicates.
- **#4 — unknown service names.** `qctl start <typo>` installed every quadlet,
  ran `daemon-reload` and exited `0`, indistinguishable from `qctl install`.
  `start`, `stop`, `restart` and `update` now reject a name that matches no quadlet
  with `no such quadlet: <name>` (non-zero exit) before any side effect, and
  `start <service>` links only the requested unit.
- **Regular files in the target directory.** `qctl` no longer deletes or replaces
  regular (non-symlink) files there: `install` and `uninstall` only act on
  symlinks and warn on stderr about anything else.

Follow-up review of this round found a blocker and a major sharing the same root
cause: `start` ignored quadlets whose origin already lives inside the target tree,
so `install` (correctly) left them unlinked while `start` either did nothing or
recreated the very self-symlink #3 removes. Both are fixed:

- **B1 — `start` without arguments did nothing in the nested layout.** With the
  current directory set to the target directory and a nested `quadlets/` subdirectory,
  `qctl start` only ran `daemon-reload` and exited `0`, starting no unit. The
  "installed" predicate now also accepts a quadlet living one level below the target
  directory, so the unit is started. Covered by
  `start_without_arguments_starts_units_in_target_subdirs`.
- **M1 — `start <name>` recreated the redundant self-symlink.** `install_single`
  did not consult `source_is_in_target_dir` and linked an origin already inside the
  target tree back into it. It now creates nothing in that case. Covered by
  `start_service_already_in_target_dir_links_nothing`.
- **m1 — dead branch removed.** The "requested unit not linked after install"
  scenario was removed together with the unreachable `bail!` branch
  "not linked in the target directory" that it described: with the new "counts as
  installed" predicate a unit collected by `start` is installed by construction.
- **m2 — missing assertion added.** `install_skips_quadlets_in_subdirectories_of_target_dir`
  now asserts the `Skipping a.container (already in target dir)` message the
  scenario requires, not only the absence of symlinks.

Tests covering each fix:

| Fix | Integration tests |
| --- | --- |
| #3 (ancestry) | `install_skips_quadlets_in_subdirectories_of_target_dir`, `install_skips_quadlets_already_in_target_dir` |
| #3 (stale self-link) | `install_recovers_from_stale_self_link` (plus `duplicate_quadlet_names_fail_with_clear_error` for real duplicates) |
| #4 (unknown names) | `start_unknown_service_fails_without_installing`, `stop_and_restart_unknown_service_fail`, `update_unknown_service_fails` |
| #4 (single unit) | `start_service_links_only_that_unit`, `installed_unit_survives_missing_source` |
| Regular files | `install_does_not_replace_regular_file_in_target_dir`, `uninstall_preserves_regular_files_in_target_dir` |
| B1 (`start` without args) | `start_without_arguments_starts_units_in_target_subdirs` |
| M1 (self-symlink) | `start_service_already_in_target_dir_links_nothing` |

Verification:

- Unit tests: 6 passed
- Integration tests: 32 passed (38 total)

Decisions and compatibility:

- `status` and `menu` (and the read-only commands `check`, `logs`, `logsf`) do not
  validate service names: they are queries and must keep reporting whatever
  exists, or the underlying `systemctl` error, instead of failing up front.
- Contract change: `update <unknown-name>` used to exit `0` after printing
  `Container '<name>' not found`; it now exits non-zero with
  `no such quadlet: <name>`, consistent with the other unit commands.


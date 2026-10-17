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
- External command failures now include command, exit status, and stderr.
- `clean-volumes` now requires confirmation by default and supports `--yes` for scripts.
- A global `--dry-run` mode now reports planned actions without changing files or calling external tools.
- `status --compact` now emits only compact script-friendly rows.
- Service name normalization now removes only the final extension.
- CI now runs formatting, clippy, dependency audit, build, and tests.
- Release workflow now runs formatting, clippy, and tests before publishing assets.

## Verification

Commands executed:

```bash
cargo fmt -- --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --verbose
cargo tree -d
env CARGO_HOME=/tmp/qctl-cargo-audit-home cargo audit
git diff --check
```

Results:

- Unit tests: 5 passed
- Integration tests: 9 passed
- Clippy: passed with `-D warnings`
- Formatting: passed
- Build: passed
- Duplicate dependency check: no duplicate dependencies reported
- RustSec audit: 70 crate dependencies scanned, no vulnerabilities reported

Note: `cargo build --verbose` may print a Cargo cache warning in this sandbox because the global Cargo last-use database is read-only. The project build itself completes successfully.

## Residual Risk

- `menu` remains interactive and is not covered by automated integration tests.
- `logs` and `logsf` stream `journalctl`; tests do not exercise long-running follow mode.
- `check` depends on `/usr/lib/podman/quadlet`; it is not integration-tested because the binary path is host-specific.
- `clean-volumes --yes` is intentionally destructive. The confirmation and `--dry-run` guard reduce accidental use, but scripts must still use it carefully.

## Recommended Next Steps

- Add `--source` and `--target` flags to make audits and tests less dependent on current directory and `$HOME`.
- Add a `doctor` command to validate required host tools before running operational commands.
- Add integration tests for `check` through a configurable quadlet binary path if that command needs stronger automated coverage.

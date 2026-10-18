# qctl — agent guide

## Commands

```sh
cargo build               # debug build
cargo build --release     # optimized build, single binary
cargo test                # 36 tests: 6 unit + 30 integration
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo audit               # dependency advisories (not installed by default)
```

CI (`.github/workflows/rust.yml`) runs `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo audit`, `cargo build --verbose` and `cargo test --verbose` on push to `main`, `develop`, `release/**` and `hotfix/**`, and on PRs to `main` and `develop`.

Both workflows run on `ubuntu-24.04` with `actions/checkout@v7`. Release builds (`.github/workflows/release.yml`) cross-compile to `x86_64-unknown-linux-musl` with `RUSTFLAGS="-C target-feature=+crt-static"` for fully static binaries. Triggered by GitHub Release creation.

## Project structure

- **Single crate, single binary source** — `src/main.rs` is the only source file of the binary: no lib crate, no modules. Unit tests live in `src/main.rs` under `#[cfg(test)]`.
- **Integration tests** — `tests/cli.rs` (30 tests) drives the compiled binary with fake `systemctl`/`podman`/`journalctl` shims.
- **Specs** — `openspec/` drives the change flow (spec-driven development): every feature/fix opens a proposal under `openspec/changes/<change>/`, is validated with `openspec validate <change> --strict`, and is archived into `openspec/specs/` with `openspec archive` once the PR is merged.
- **Docs** — `docs/AUDIT.md` holds the security audit; `CONTRIBUTING.md` covers the branch model, commit style and the feature/release/hotfix procedures.
- **Entrypoint**: `qctl` CLI defined at `src/main.rs:22` via clap derive.

## Dependencies

clap (derive), clap_complete, comfy-table, tokio (full), anyhow, tracing, tracing-subscriber.

## Platform & requirements

Linux-only. Requires at runtime: `systemctl --user`, `podman`, `journalctl`, `/usr/lib/podman/quadlet`.

Quadlets are discovered from `./quadlets/` and `.` (current dir), symlinked to `$HOME/.config/containers/systemd`.

## Version management

Version is defined in `Cargo.toml` and bumped via `.vampus.yml` (`current_version`) — not manually edited.

## Branching model (gitflow)

- `develop` is the integration branch; every feature PR targets `develop`.
- `main` only receives merges from `release/*` and `hotfix/*` (enforced by `branch-guard.yml`); never commit or push directly to `main`.
- Version bumps happen on the release branch (`chore(release): …`), editing `.vampus.yml` and `Cargo.toml`.
- `git flow release finish` / `hotfix finish` merge to `main`, tag `vX.Y.Z` and merge back to `develop`; the merge-back is mandatory.
- Creating the release (`gh release create --target main`) triggers `release.yml`; verify the assets are uploaded.
- See `CONTRIBUTING.md` for the full procedure.

## Quirks

- `Cargo.lock` is ignored (`.gitignore:8`) and untracked (`git ls-files Cargo.lock` is empty); it is regenerated locally on build.
- Status output uses emoji (`✅`, `❌`, `🟢`, `🟡`, `⚫`).
- No `rustfmt.toml` or `clippy.toml` — uses Rust defaults.

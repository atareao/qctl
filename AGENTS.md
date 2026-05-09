# qctl — agent guide

## Commands

```sh
cargo build               # debug build
cargo build --release     # optimized build, single binary
cargo test                # runs, but there are 0 tests in this repo
cargo clippy              # uses default config (no clippy.toml)
```

CI (`.github/workflows/rust.yml`) runs `cargo build --verbose && cargo test --verbose` on push/PR to `main`.

Release builds (`.github/workflows/release.yml`) cross-compile to `x86_64-unknown-linux-musl` with `RUSTFLAGS="-C target-feature=+crt-static"` for fully static binaries. Triggered by GitHub Release creation.

## Project structure

- **Single crate, single file** — `src/main.rs` is the only source file. No lib crate, no modules.
- **Entrypoint**: `qctl` CLI defined at `src/main.rs:22` via clap derive.

## Dependencies

clap (derive), clap_complete, comfy-table, tokio (full), anyhow, tracing, tracing-subscriber.

## Platform & requirements

Linux-only. Requires at runtime: `systemctl --user`, `podman`, `journalctl`, `/usr/lib/podman/quadlet`.

Quadlets are discovered from `./quadlets/` and `.` (current dir), symlinked to `$HOME/.config/containers/systemd`.

## Version management

Version is defined in `Cargo.toml` and bumped via `.vampus.yml` — not manually edited.

## Quirks

- `.gitignore` lists `Cargo.lock` but the file is present in the repo (committed before being ignored, or tracked despite the rule).
- Status output uses emoji (`✅`, `❌`, `🟢`, `🟡`, `⚫`).
- No `rustfmt.toml` or `clippy.toml` — uses Rust defaults.

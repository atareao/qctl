# Tasks — Migrate to comfy-table 8

## Red

- [x] Bump `comfy-table` to `"8"` in `Cargo.toml`
- [x] `cargo check`: confirm the build fails with `E0432`/`E0599` on the two call sites

## Green

- [x] Migrate both call sites to `load_style(UTF8_FULL.with_rounded_corners())` (`status()` and `print_menu_table()`)
- [x] Drop the `comfy_table::modifiers::UTF8_ROUND_CORNERS` import
- [x] `cargo build` compiles
- [x] `cargo test`: all tests green (7 unit + 60 integration)

## Refactor

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`

## Clean-build validation (CI equivalent, no lockfile)

- [x] `rm -rf target Cargo.lock && cargo build`
- [x] `cargo test` on the clean build

## Close

- [x] `openspec validate migrate-comfy-table-8 --strict`
- [ ] `openspec archive migrate-comfy-table-8`
- [ ] Conventional gitmoji commits on the feature branch + PR to `development`
- [ ] Close issue #20 via the PR

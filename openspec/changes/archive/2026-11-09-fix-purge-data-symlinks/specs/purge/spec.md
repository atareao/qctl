# Spec Delta

## Purpose

Endurecer `qctl purge --data` (issue #31) para que resuelva symlinks y re-compruebe la
contención contra el `$HOME` canónico antes de borrar, cerrando el escape por symlink
intermedio.

## ADDED Requirements

### Requirement: purge --data SHALL resolve symlinks and re-check containment before deleting

Before removing any bind directory, `purge --data` SHALL resolve the on-disk real path and
re-check containment against the canonical `$HOME`: it SHALL compute
`home_canon = fs::canonicalize($HOME)` and `real = fs::canonicalize(<bind source>)`, and it
SHALL delete the resolved real path (`real`) and only when `real` is a strict descendant of
`home_canon` (never
`home_canon` itself). When `real` escapes `home_canon`, or when canonicalization fails, purge
SHALL skip the entry with a `warn!` naming both the declared path and the real target, and
SHALL NOT delete anything for it. The lexical normalization of `resolve_bind_source` SHALL
be kept as a first barrier. This verification SHALL apply identically to both `rm -rf` and
`podman unshare rm -rf`.

#### Scenario: intermediate symlink escaping $HOME is not followed

- **WHEN** `$HOME/.config/navidrome` is a symlink to `/mnt/ssd/navidrome` and a quadlet
  declares `Volume=%h/.config/navidrome/data:/data:Z`, and `--data` is set
- **THEN** purge resolves the real path to `/mnt/ssd/navidrome/data`, detects it escapes
  `$HOME`, warns naming the declared path and the real target, and removes nothing for it

#### Scenario: internal symlinks that stay under $HOME are deleted normally

- **WHEN** a bind's real path, after resolving symlinks, is a strict descendant of the
  canonical `$HOME`
- **THEN** purge removes it as usual (with `rm -rf` or `podman unshare rm -rf` per the
  mapped-uid rule)

#### Scenario: leaf symlink pointing outside $HOME is not followed

- **WHEN** the bind source itself is a symlink whose target is outside `$HOME`, and `--data`
  is set
- **THEN** purge resolves the target, detects it escapes `$HOME`, warns, and removes nothing
  for it

#### Scenario: canonicalization failure skips the entry

- **WHEN** the bind path no longer exists (or cannot be resolved) at deletion time and
  `--data` is set
- **THEN** purge warns and skips the entry without aborting the rest of the purge

## MODIFIED Requirements

### Requirement: purge --data SHALL remove only eligible host-side bind mounts

With `--data`, `purge` SHALL consider only `Volume=` entries whose source is a host path
(not a named volume) and, for each, SHALL resolve systemd suffixes (`%h` to `$HOME`, `%t`
to `XDG_RUNTIME_DIR` or `/run/user/<uid>`) and remove the resolved directory. It MUST NOT
remove sockets or files (only directories), MUST NOT remove `%t/podman/podman.sock` or any
path under `%t`, and MUST NOT remove absolute system paths outside `$HOME`. A path is
eligible only when it resolves under `$HOME` **strictly below it** (never `$HOME` itself),
after normalizing `.` and `..` lexically **and** after resolving symlinks against the real
filesystem. The eligible source prefixes are `%h/`, `~/`, `$HOME/` and `${HOME}/`; relative
paths (`./…`) are rejected because their base directory is ambiguous. A resolved path that
escapes `$HOME` through `..` or through a symlink MUST be rejected.

#### Scenario: bind under $HOME is removed

- **WHEN** a `.container` declares `Volume=%h/.config/navidrome/data:/data:Z` and `--data`
  is set
- **THEN** purge removes `$HOME/.config/navidrome/data`

#### Scenario: podman socket is ignored

- **WHEN** a `.container` declares `Volume=%t/podman/podman.sock:/var/run/docker.sock:ro`
  and `--data` is set
- **THEN** purge ignores that entry (socket, `%t` and non-directory) and removes nothing
  for it

#### Scenario: system path is ignored

- **WHEN** a `.container` declares a bind whose resolved source is outside `$HOME` (for
  example `/etc/foo`)
- **THEN** purge ignores that entry and removes nothing for it

#### Scenario: home itself is rejected

- **WHEN** a `.container` declares `Volume=%h:/x` (or `$HOME:/x`, `~:/x`, `${HOME}:/x`) and
  `--data` is set
- **THEN** purge rejects the entry (the resolved path equals `$HOME`) and removes nothing for it

#### Scenario: path escaping $HOME via .. is rejected

- **WHEN** a `.container` declares `Volume=%h/../../etc:/x` and `--data` is set
- **THEN** purge normalizes the path, detects it resolves outside `$HOME` and removes nothing
  for it

#### Scenario: named volume is not data

- **WHEN** a `.container` declares `Volume=memos-memos.volume:/var/opt/memos:Z` and `--data`
  is set
- **THEN** purge does not treat it as data (named volumes belong to `clean-volumes`)

#### Scenario: symlink escaping $HOME is rejected

- **WHEN** a bind whose lexical path is under `$HOME` resolves, after following symlinks,
  to a real path outside `$HOME`, and `--data` is set
- **THEN** purge rejects the entry and removes nothing for it

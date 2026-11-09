# purge Specification

## Purpose
Define `qctl purge`: la desinstalación destructiva y selectiva de un stack —desenlazarlo y
borrar, con opt-in explícito, sus volúmenes nombrados, secretos, imágenes y datos de bind
mounts host-side— respetando los recursos compartidos con otras stacks.

## Requirements

### Requirement: purge SHALL run from the current stack directory

`qctl purge` SHALL operate on the stack of the current working directory, exactly like the
other stack commands: its sources are discovered with `discover_source_dirs` over the
current directory.

#### Scenario: purge resolves the current stack

- **WHEN** the user runs `qctl purge` from a stack directory
- **THEN** purge acts only on the quadlets of that stack

### Requirement: purge without flags SHALL uninstall and clean named volumes only

With no extra category flags, `purge` SHALL `uninstall` (stop services and remove symlinks,
with a daemon reload) and then `clean-volumes` (remove the named volumes declared in the
stack's `.volume` files), and SHALL touch no secrets, images or bind-mounted data.

#### Scenario: default purge removes links and named volumes

- **WHEN** the user runs `qctl purge` in a stack that declares a `.volume`
- **THEN** the stack is uninstalled and its named volumes are removed, and no secret, image
  or bind-mounted data is removed

#### Scenario: --volumes is equivalent to the default base

- **WHEN** the user runs `qctl purge --volumes`
- **THEN** the behaviour is identical to running `qctl purge` with no flags

### Requirement: purge SHALL require explicit opt-in per extra category

Each destructive category beyond the base —secrets, images and bind-mounted data— SHALL
only be acted on when its flag is present (`--secrets`, `--images`, `--data`). An absent
flag MUST mean the category is untouched.

#### Scenario: no flag leaves the category untouched

- **WHEN** the user runs `qctl purge` without `--secrets`
- **THEN** no `podman secret rm` is executed for the stack

#### Scenario: enabling a category acts on it

- **WHEN** the user runs `qctl purge --secrets --images`
- **THEN** purge removes the stack's secrets and images (subject to the shared-resource
  guard) in addition to the base cleanup

### Requirement: purge --secrets SHALL remove the stack's declared secrets

With `--secrets`, `purge` SHALL read every `.container` of the stack, extract each
`Secret=<name>[,<options>]` declaration (the name is the text before the first comma; a
single file may declare several `Secret=` lines) and remove each with `podman secret rm`.

#### Scenario: multiple secrets in one quadlet

- **WHEN** a `.container` declares two `Secret=` lines and `--secrets` is set
- **THEN** both secrets are removed

#### Scenario: secret name with options

- **WHEN** a `.container` declares `Secret=my_api_key,type=env,target=API_KEY` and
  `--secrets` is set
- **THEN** purge removes the secret named `my_api_key`

#### Scenario: secret already absent does not abort

- **WHEN** a declared secret no longer exists in podman and `--secrets` is set
- **THEN** purge warns and continues with the remaining secrets

### Requirement: purge --images SHALL remove the stack's declared images

With `--images`, `purge` SHALL read every `.container` of the stack, extract each `Image=`
declaration and remove each with `podman rmi`.

#### Scenario: images declared by the stack are removed

- **WHEN** the stack's `.container` files declare images and `--images` is set
- **THEN** each declared image is removed with `podman rmi`

#### Scenario: image already absent does not abort

- **WHEN** a declared image is no longer present locally and `--images` is set
- **THEN** purge warns and continues with the remaining images

### Requirement: purge SHALL skip resources shared with another stack unless --force

Before removing a secret or an image, `purge` SHALL scan every stack under the stacks root
and, when a **different** stack still declares the same secret or image, SHALL skip it and
print a `skip: ...` line naming the other stack. With `--force`, the resource SHALL be
removed regardless.

#### Scenario: shared secret is skipped

- **WHEN** `--secrets` is set and a secret is also declared by another stack
- **THEN** purge prints `skip: secreto '<name>' todavía lo usa la stack '<other>'` and does
  not remove it

#### Scenario: shared image is skipped

- **WHEN** `--images` is set and an image is also declared by another stack
- **THEN** purge prints `skip: imagen '<ref>' todavía la usa la stack '<other>'` and does
  not remove it

#### Scenario: --force removes shared resources

- **WHEN** `--secrets` (or `--images`) is set together with `--force`
- **THEN** the shared resource is removed despite being declared by another stack

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

### Requirement: purge --data SHALL use podman unshare for mapped-uid directories

When the quadlet declaring a bind under `$HOME` runs with a mapped uid (for example
`User=1000:1000`), that bind's directory SHALL be removed with `podman unshare rm -rf`;
otherwise `rm -rf` SHALL be used.

#### Scenario: mapped uid uses podman unshare

- **WHEN** a bind under `$HOME` is declared by a quadlet with a `User=<uid>` mapping and
  `--data` is set
- **THEN** purge removes it with `podman unshare rm -rf <path>`

#### Scenario: unmapped uid uses plain rm

- **WHEN** a bind under `$HOME` is declared without a `User=` mapping and `--data` is set
- **THEN** purge removes it with `rm -rf <path>`

### Requirement: purge SHALL honour --dry-run

Under the global `--dry-run`, `purge` SHALL print every action it would take (`DRY-RUN
podman ...`, `DRY-RUN rm ...`) and MUST NOT execute any external command nor modify the
filesystem.

#### Scenario: dry-run prints without executing

- **WHEN** the user runs `qctl purge --secrets --images --data --dry-run`
- **THEN** purge prints the planned removals and runs no `podman`/`rm` command

### Requirement: purge SHALL confirm before destroying, unless --yes

Without `--yes`, `purge` SHALL list the concrete resources and resolved paths it is about
to remove and ask for confirmation; declining SHALL abort the destructive categories.
With `--yes`, no prompt SHALL be shown.

#### Scenario: confirmation is requested

- **WHEN** the user runs `qctl purge --secrets` without `--yes`
- **THEN** the secrets to remove are listed and a confirmation prompt is shown

#### Scenario: --yes skips the prompt

- **WHEN** the user runs `qctl purge --secrets --yes`
- **THEN** no prompt is shown and the removal proceeds

### Requirement: purge SHALL not abort on absent resources

A missing secret, image, volume or bind directory SHALL NOT abort the rest of the purge:
purge SHALL warn and continue with the remaining items.

#### Scenario: one absent resource does not stop the purge

- **WHEN** one of several declared secrets is already absent
- **THEN** purge warns about it and still removes the others, exiting successfully

### Requirement: purge SHALL NOT touch the source quadlets or other stacks

`purge` SHALL NOT remove the quadlet files in the stack's source directories (as
`uninstall` does not), and SHALL NOT modify resources belonging to other stacks (beyond
reading them for the shared-resource guard).

#### Scenario: source quadlets survive

- **WHEN** the user runs `qctl purge --images`
- **THEN** the stack's `.container`/`.volume` source files remain in place

#### Scenario: other stacks are untouched

- **WHEN** the user runs `qctl purge --secrets` and a secret is shared
- **THEN** the other stack's resources and files are left unmodified

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

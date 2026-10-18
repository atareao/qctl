# services

## ADDED Requirements

### Requirement: Unknown service names SHALL fail before any side effect

Commands that act on units (`start`, `stop`, `restart`, `update`) SHALL validate a
service argument against both `<source_dir>/<service>.container` and
`<target_dir>/<service>.container`. If neither exists, the command SHALL fail with
`no such quadlet: <service>` and a non-zero exit code, before creating symlinks,
running `daemon-reload` or invoking `systemctl`/`podman`.

#### Scenario: start with a typo

- **WHEN** the project provides `one.container` and `two.container` and the user runs `qctl start typo`
- **THEN** the command fails with `no such quadlet: typo`, exits non-zero, and leaves the target directory without symlinks

#### Scenario: stop and restart with an unknown name

- **WHEN** the user runs `qctl stop typo` or `qctl restart typo`
- **THEN** both fail with `no such quadlet: typo` and a non-zero exit code

#### Scenario: update with an unknown name

- **WHEN** the user runs `qctl update typo`
- **THEN** the command fails with `no such quadlet: typo` and exits non-zero instead of printing `Container 'typo' not found`

#### Scenario: installed unit whose source was removed

- **WHEN** `svc.container` exists in the target directory but no longer in the project
- **THEN** `qctl start svc` and `qctl stop svc` are accepted

### Requirement: start with a service SHALL install only that unit

When a service is given and its quadlet is neither linked into the target directory
nor already living inside the target directory tree, `start` SHALL create only that
symlink, pointing at the project source. It SHALL NOT install unrelated quadlets.

#### Scenario: first-run start of one unit

- **WHEN** the project provides `one.container` and `two.container`, nothing is installed, and the user runs `qctl start one`
- **THEN** only `one.container` is linked into the target directory

### Requirement: start SHALL find units whose quadlet already lives in the target directory

A container unit SHALL count as installed when `<target_dir>/<unit>.container` exists
— as the symlink `install` creates or as a regular file — **or** when
`<unit>.container` lives one level below the target directory, for example
`<target_dir>/quadlets/<unit>.container`. `start` SHALL start such a unit, with or
without a service argument, without `qctl` creating a redundant symlink for it first.

#### Scenario: start one unit whose quadlet already lives in the target tree

- **WHEN** the current directory is the target directory, it contains `quadlets/other.container`, and the user runs `qctl start other`
- **THEN** `systemctl --user start other.service` runs and the command exits successfully
- **AND** no `other.container` symlink is created inside the target directory

#### Scenario: start without arguments in a nested layout

- **WHEN** the current directory is the target directory and it contains `quadlets/other.container`
- **THEN** `qctl start` starts every container unit found, including `other.service`, and exits successfully
- **AND** it creates no symlink inside the target directory

#### Scenario: start without arguments is unchanged

- **WHEN** the user runs `qctl start` with no service
- **THEN** every missing quadlet is installed and every container unit is started

### Requirement: start SHALL fail when a requested unit could not be started

`start` SHALL return a non-zero exit code, naming the failing unit(s), when
`systemctl --user start` fails. A requested unit SHALL never be silently skipped:
`ensure_service_known` rejects a name that matches no quadlet with
`no such quadlet: <name>` before any side effect, and a requested unit that counts as
installed is always started.

#### Scenario: systemctl fails

- **WHEN** the fake `systemctl` exits non-zero for `svc`
- **THEN** `qctl start svc` exits non-zero and the error names `svc`

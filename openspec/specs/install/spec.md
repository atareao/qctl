# install Specification

## Purpose

Defines how `qctl` links a project's quadlets into the systemd user target directory: what counts as already installed, how entries that share a name are treated, and how regular files in the target directory are protected.

## Requirements

### Requirement: Sources inside the target directory SHALL NOT be linked into it

`install` SHALL treat a quadlet whose canonical parent directory is the target
directory or any of its descendants as already installed, and SHALL NOT create a
symlink for it. The check SHALL use path ancestry over canonical paths, not
direct-parent equality.

#### Scenario: repository nested inside the target directory

- **WHEN** the current directory is `$HOME/.config/containers/systemd`, it contains `quadlets/other.container`, and the user runs `qctl install`
- **THEN** no symlink is created inside the target directory and the command exits successfully
- **AND** running `qctl install` a second time also exits successfully with no duplicate-name error

#### Scenario: source in a subdirectory of the target directory

- **WHEN** the target directory contains `quadlets/a.container` and there is no top-level `a.container`
- **THEN** `install` reports `Skipping a.container (already in target dir)` and creates nothing

### Requirement: Redundant collected entries resolving to the same file SHALL be collapsed

When two collected entries share a file name, `collect_quadlets` SHALL compare
their canonical targets. If both resolve to the same file, it SHALL keep one entry
(preferring the one that is not a symlink) and continue. If they resolve to
different files, it SHALL keep failing with the existing duplicate-name error.

#### Scenario: stale self-link from an older version

- **WHEN** the target directory already contains `other.container` as a symlink to `quadlets/other.container`
- **THEN** `qctl install` and `qctl uninstall` both succeed instead of failing with `duplicate quadlet file name`

#### Scenario: genuine duplicate

- **WHEN** `quadlets/x.container` and `x.container` are two different regular files with the same name
- **THEN** the command fails with `duplicate quadlet file name 'x.container' found in ...` and exits non-zero

### Requirement: Regular files in the target directory SHALL NOT be deleted or overwritten

`install` SHALL refuse to replace a regular (non-symlink) file in the target
directory: it SHALL warn on stderr, skip that entry and continue with the rest.
`uninstall` SHALL only remove target-directory entries that are symlinks; a regular
file with a matching name SHALL be left untouched and reported as skipped. Neither
command SHALL write through a symlink to delete the file it points to.

#### Scenario: uninstall from inside the target directory

- **WHEN** the current directory is the target directory and it contains the regular file `one.container`
- **THEN** `qctl uninstall` leaves `one.container` in place and exits successfully

#### Scenario: regular file shadows a source quadlet

- **WHEN** the target directory contains the regular file `one.container` and the project provides `one.container`
- **THEN** `qctl install` warns that `one.container` is not a symlink, leaves the file untouched, links the remaining quadlets and exits successfully

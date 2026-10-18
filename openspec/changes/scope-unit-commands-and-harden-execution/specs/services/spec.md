# Spec Delta

## MODIFIED Requirements

### Requirement: start SHALL find units whose quadlet already lives in the target directory

A container unit SHALL count as installed when `<target_dir>/<unit>.container` exists
— as the symlink `install` creates or as a regular file — **or** when
`<unit>.container` lives one level below the target directory, for example
`<target_dir>/quadlets/<unit>.container`. `start` SHALL start such a unit when it is
given an explicit service name, without `qctl` creating a redundant symlink for it
first. When `start` runs without arguments, the unit SHALL be started only if it
belongs to the current project's collected quadlets.

#### Scenario: start one unit whose quadlet already lives in the target tree

- **WHEN** the current directory is the target directory, it contains `quadlets/other.container`, and the user runs `qctl start other`
- **THEN** `systemctl --user start other.service` runs and the command exits successfully
- **AND** no `other.container` symlink is created inside the target directory

#### Scenario: start without arguments in a nested layout

- **WHEN** the current directory is the target directory and it contains `quadlets/other.container`
- **THEN** `qctl start` starts every container unit found, including `other.service`, and exits successfully
- **AND** it creates no symlink inside the target directory

#### Scenario: start without arguments is unchanged

- **WHEN** the user runs `qctl start` with no service from a project directory
- **THEN** every missing quadlet of the current project is installed and every container unit of the current project is started

## ADDED Requirements

### Requirement: Unit commands without a service SHALL act only on the current project

`start`, `stop` and `restart` invoked without a service SHALL operate only on the
container quadlets collected from the current project's source directories. They SHALL
NOT act on units that another project merely linked into the target directory. `status`
and `menu` SHALL keep reporting every container unit found in the target directory,
from any project. The status table that the mutating unit commands (`install`, `uninstall`,
`reinstall`, `start`, `stop` and `restart`) print after acting SHALL reflect the
same project-scoped set the command operated on.

#### Scenario: stop in one project does not touch another project's units

- **WHEN** project A and project B are both linked into the same target directory and the user runs `qctl stop` from project A
- **THEN** `systemctl --user stop` runs only for project A's container units
- **AND** project B's units are not stopped

#### Scenario: restart in one project does not touch another project's units

- **WHEN** project A and project B are both linked into the same target directory and the user runs `qctl restart` from project A
- **THEN** `systemctl --user restart` runs only for project A's container units

#### Scenario: start without arguments does not start another project's units

- **WHEN** project A and project B are both linked into the same target directory and the user runs `qctl start` from project A
- **THEN** only project A's container units are started

#### Scenario: the table printed after a unit command matches what it acted on

- **WHEN** project A and project B are both linked into the same target directory and the user runs `qctl stop` from project A
- **THEN** the status table printed by that same command lists only project A's units

#### Scenario: status still reports the whole target directory

- **WHEN** project A and project B are both linked into the same target directory and the user runs `qctl status` from project A
- **THEN** the table reports the units of both projects

#### Scenario: menu still reports the whole target directory

- **WHEN** project A and project B are both linked into the same target directory and the user runs `qctl menu` from project A
- **THEN** the table reports the units of both projects

#### Scenario: an explicit service name still resolves against the target directory

- **WHEN** `svc.container` is linked in the target directory while its project source no longer exists, and the user runs `qctl stop svc`
- **THEN** the command is accepted and `systemctl --user stop svc.service` runs

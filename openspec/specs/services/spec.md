# services Specification

## Purpose

Defines which container units `qctl` acts on: the scope of the unit commands, how a service name is validated and resolved, when `start` installs a unit on demand, and how failures are reported.

## Requirements

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

### Requirement: start SHALL fail when a requested unit could not be started

`start` SHALL return a non-zero exit code, naming the failing unit(s), when
`systemctl --user start` fails. A requested unit SHALL never be silently skipped:
`ensure_service_known` rejects a name that matches no quadlet with
`no such quadlet: <name>` before any side effect, and a requested unit that counts as
installed is always started.

#### Scenario: systemctl fails

- **WHEN** the fake `systemctl` exits non-zero for `svc`
- **THEN** `qctl start svc` exits non-zero and the error names `svc`

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

### Requirement: status SHALL offer an opt-in project-scoped view

`status` SHALL keep the wide target-directory view by default: it SHALL report every
container unit found in the target directory, from any project, as it does today.
`status` SHALL additionally accept a `--project` flag that restricts the report to the
container quadlets collected from the current project's source directories, the same
scope `start`, `stop` and `restart` use when invoked without a service. `menu` SHALL be
unaffected and SHALL keep the wide view.

#### Scenario: status --project lists only the current project's units

- **WHEN** project A and project B are both linked into the same target directory and the user runs `qctl status --project` from project A
- **THEN** the table lists only project A's units
- **AND** project B's units are absent from the table

#### Scenario: status without the flag still reports the whole target directory

- **WHEN** project A and project B are both linked into the same target directory and the user runs `qctl status` from project A
- **THEN** the table reports the units of both projects

#### Scenario: status --project combined with an explicit service

- **WHEN** the user runs `qctl status --project voicebox`
- **THEN** the report shows `voicebox`, because an explicitly named service is resolved by name with or without `--project` (the flag only sets the argument-less scope and does not restrict a name)

#### Scenario: status --project with no units in the current project

- **WHEN** the current directory collects no quadlets and the user runs `qctl status --project`
- **THEN** the command reports no container units and exits successfully

### Requirement: SHALL strip only known quadlet extensions from a service argument

A service argument SHALL be resolved against quadlet files by stripping only a known
quadlet extension (`.container`, `.volume`, `.network`, `.pod`, `.kube`, `.image`,
`.build`, `.artifact`, `.service`, `.socket`, `.mount`, `.timer`, `.path`) when present.
Any other trailing dot-segment, such as the `.prod` in `voicebox.prod`, SHALL be kept as
part of the stem. Resolution SHALL NOT reject a stem that contains dots.

#### Scenario: a service argument with a dotted stem is accepted

- **WHEN** the project provides `voicebox.prod.container` and the user runs `qctl start voicebox.prod`
- **THEN** the command is accepted (no `no such quadlet`) and acts on `voicebox.prod`

#### Scenario: a service argument with a full quadlet filename is accepted

- **WHEN** the project provides `voicebox.prod.container` and the user runs `qctl stop voicebox.prod.container`
- **THEN** the command is accepted and acts on `voicebox.prod`

#### Scenario: a plain stem keeps the legacy behaviour

- **WHEN** the project provides `voicebox.container` and the user runs `qctl status voicebox`
- **THEN** the command is accepted and resolves to `voicebox`

### Requirement: SHALL form the systemd unit from a container stem by appending `.service`

A container stem such as `voicebox` or `voicebox.prod` SHALL resolve to the systemd user
unit `format!("{stem}.service")` — always appending `.service`, regardless of whether the
stem itself contains dots. `start`, `stop`, `restart` (including their `--dry-run` output),
`logs` and `recent_logs` SHALL operate on the formed unit. The heuristic that treats any
name containing a dot as already-formed SHALL be removed.

#### Scenario: start forms the unit for a plain stem

- **WHEN** the project provides `voicebox.container` and the user runs `qctl start voicebox`
- **THEN** `systemctl --user start voicebox.service` runs

#### Scenario: start forms the unit for a dotted stem

- **WHEN** the project provides `voicebox.prod.container` and the user runs `qctl start voicebox.prod`
- **THEN** `systemctl --user start voicebox.prod.service` runs, not `systemctl --user start voicebox.prod`

#### Scenario: stop forms the unit for a dotted stem

- **WHEN** the project provides `voicebox.prod.container` and the user runs `qctl stop voicebox.prod`
- **THEN** `systemctl --user stop voicebox.prod.service` runs

#### Scenario: restart forms the unit for a dotted stem

- **WHEN** the project provides `voicebox.prod.container` and the user runs `qctl restart voicebox.prod`
- **THEN** `systemctl --user restart voicebox.prod.service` runs

#### Scenario: dry-run prints the formed unit for a dotted stem

- **WHEN** the project provides `voicebox.prod.container` and the user runs `qctl --dry-run start voicebox.prod`
- **THEN** the output prints `DRY-RUN systemctl --user start voicebox.prod.service`

#### Scenario: logs forms the unit for a dotted stem

- **WHEN** the project provides `voicebox.prod.container` and the user runs `qctl logs voicebox.prod`
- **THEN** `journalctl --user` is invoked with `-u voicebox.prod.service`

#### Scenario: logs with a non-container suffixed name still forms a container unit

- **WHEN** the user runs `qctl logs foo.mount`
- **THEN** `journalctl --user` is invoked with `-u foo.service` (qctl logs always targets the container unit of the stem)

#### Scenario: a dotted stem that is already fully formed is not double-suffixed

- **WHEN** `systemctl_user` receives a fully formed unit name, for example the output of
  `generated_unit("voicebox.prod", "container")` = `voicebox.prod.service`
- **THEN** the unit is passed to `systemctl --user` unchanged (no `.service.service`)

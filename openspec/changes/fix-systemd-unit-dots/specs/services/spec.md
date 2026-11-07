# services Specification

## Purpose

Defines which container units `qctl` acts on: the scope of the unit commands, how a service name is validated and resolved, when `start` installs a unit on demand, and how failures are reported.

## ADDED Requirements

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

## KNOWN LIMITATIONS

### A stem that ends in a known quadlet extension cannot be round-tripped

A quadlet whose stem itself ends in a known quadlet extension, e.g. `app.service.container`
(stem `app.service`), is displayed as `app.service` by `status`/`menu`, but passing that
name back as a service argument strips the `.service` suffix and fails with
`no such quadlet: app`. Documented as a known limitation; a stem whose last dot-segment is
an extension of the *quadlet file* (`.container`, `.volume`, …) is indistinguishable from a
stem that simply ends that way.

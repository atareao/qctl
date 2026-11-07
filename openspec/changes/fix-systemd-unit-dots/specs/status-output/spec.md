# status-output Specification

## Purpose

Defines what `status` and `menu` report about the units found in the target directory: the table shape, the running state, and the interplay of the target-directory-wide view with the project-scoped view.

## ADDED Requirements

### Requirement: status SHALL report a dotted-stem unit as running when its formed unit is active

When a quadlet whose stem contains dots (for example `voicebox.prod.container`) is linked
and its formed unit `voicebox.prod.service` is active, `status` and `menu` SHALL report it
as running. Querying state SHALL use the formed unit, never the bare `voicebox.prod` stem.

#### Scenario: status reports a dotted-stem unit as running

- **WHEN** `voicebox.prod.container` is linked in the target directory and
  `systemctl --user is-active voicebox.prod.service` prints `active`
- **THEN** `qctl status` reports `voicebox.prod` as running

#### Scenario: status does not query the bare dotted stem

- **WHEN** `voicebox.prod.container` is linked in the target directory
- **THEN** `qctl status` never invokes `systemctl --user is-active voicebox.prod`
- **AND** it invokes `systemctl --user is-active voicebox.prod.service` instead

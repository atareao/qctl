# status-output Specification

## Purpose

Defines the machine-readable output contract of `qctl status --compact`, the mode the
command advertises as suitable for scripts: what a consumer can rely on — separators,
tokens and the absence of decoration.

## ADDED Requirements

### Requirement: status --compact SHALL emit machine-parseable, undecorated lines

With `--compact`, `status` SHALL print exactly one line per container unit and nothing
else: three tab-separated fields `<unit>\t<link>\t<state>`, with no emoji, ANSI colour
or other decoration. `<link>` SHALL be `linked` when a symlink exists in the target
directory and `missing` otherwise. `<state>` SHALL be `running` when the linked unit is
active in `systemd --user`, `stopped` when it is linked but inactive, and `-` when the
unit is not linked. The set of units reported, and the process exit behaviour, SHALL be
identical to the human-readable table.

#### Scenario: a linked, running unit

- **WHEN** `voicebox.container` is linked and `voicebox.service` is active, and the user runs `qctl status --compact`
- **THEN** stdout contains the line `voicebox\tlinked\trunning`
- **AND** the line contains no emoji

#### Scenario: a linked but inactive unit

- **WHEN** `voicebox.container` is linked and `voicebox.service` is inactive, and the user runs `qctl status --compact`
- **THEN** stdout contains the line `voicebox\tlinked\tstopped`

#### Scenario: a unit with no symlink

- **WHEN** `voicebox.container` is not linked in the target directory and the user runs `qctl status --compact`
- **THEN** stdout contains the line `voicebox\tmissing\t-`

#### Scenario: compact prints no summary block

- **WHEN** the user runs `qctl status --compact`
- **THEN** stdout contains no `Summary:` line and no `total`/`linked`/`running`/`stopped`/`missing` counters

#### Scenario: the human-readable table is unchanged

- **WHEN** the user runs `qctl status` without `--compact`
- **THEN** the decorated table and the `Summary:` block are printed as before

# status-output Specification

## Purpose
Defines the machine-readable output contract of `qctl status --compact`, the mode the
command advertises as suitable for scripts: what a consumer can rely on — separators,
tokens and the absence of decoration.

## Requirements

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
- **THEN** stdout contains no `Summary:` block and no counter lines (the `total … = …` lines the human-readable summary prints); the `linked`/`running`/`stopped`/`missing` tokens that are legitimate TSV field values SHALL still appear

#### Scenario: compact with no units prints nothing

- **WHEN** the current project collects no container quadlets and the user runs `qctl status --compact`
- **THEN** stdout is empty and the command exits successfully

#### Scenario: the human-readable table is unchanged

- **WHEN** the user runs `qctl status` without `--compact`
- **THEN** the decorated table and the `Summary:` block are printed as before

### Requirement: table rendering SHALL be stable across the comfy-table 8 migration

The human-readable table of `status` and the `menu` action table SHALL render with UTF8 borders and rounded corners using the comfy-table 8 API — `presets::UTF8_FULL.with_rounded_corners()` applied via `Table::load_style` — producing output byte-identical (same characters, same layout) to the previous comfy-table 7 rendering (`load_preset(UTF8_FULL)` and the `UTF8_ROUND_CORNERS` modifier).

#### Scenario: the status table keeps UTF8 rounded borders after the migration

- **WHEN** `comfy-table` has been migrated to version 8 and the user runs `qctl status` without `--compact` on a target with at least one unit
- **THEN** the decorated table renders with UTF8 borders and rounded corners, character-for-character as before the migration, followed by the `Summary:` block

#### Scenario: the menu table keeps UTF8 rounded borders after the migration

- **WHEN** `comfy-table` has been migrated to version 8 and the menu screen is drawn with at least one unit
- **THEN** the action table renders with UTF, borders and rounded corners, character-for-character as before the migration

# status-output Specification

## Purpose

Defines the output contract of `qctl status`: the machine-readable `--compact` mode and the human-readable decorated table, including the `menu` action table which shares the same rendering.

## ADDED Requirements

### Requirement: table rendering SHALL be stable across the comfy-table 8 migration

The human-readable table of `status` and the `menu` action table SHALL render with UTF8 borders and rounded corners using the comfy-table 8 API — `presets::UTF8_FULL.with_rounded_corners()` applied via `Table::load_style` — producing output byte-identical (same characters, same layout) to the previous comfy-table 7 rendering (`load_preset(UTF8_FULL)` and the `UTF8_ROUND_CORNERS` modifier).

#### Scenario: the status table keeps UTF8 rounded borders after the migration

- **WHEN** `comfy-table` has been migrated to version 8 and the user runs `qctl status` without `--compact` on a target with at least one unit
- **THEN** the decorated table renders with UTF8 borders and rounded corners, character-for-character as before the migration, followed by the `Summary:` block

#### Scenario: the menu table keeps UTF8 rounded borders after the migration

- **WHEN** `comfy-table` has been migrated to version 8 and the menu screen is drawn with at least one unit
- **THEN** the action table renders with UTF, borders and rounded corners, character-for-character as before the migration
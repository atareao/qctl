# Spec Delta

## ADDED Requirements

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

- **WHEN** the user runs `qctl status --project voicebox` and `voicebox.container` belongs to the current project
- **THEN** the report is scoped to the current project and shows `voicebox`

#### Scenario: status --project with no units in the current project

- **WHEN** the current directory collects no quadlets and the user runs `qctl status --project`
- **THEN** the command reports no container units and exits successfully

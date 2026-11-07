# check Specification

## Purpose

Defines `qctl check`: validating a single quadlet against the real Podman generator, and
auditing a whole stacks root for cross-stack collisions and naming-convention drift.

## ADDED Requirements

### Requirement: check SHALL validate a single quadlet file

`qctl check <QUADLET>` SHALL validate the given quadlet with the real generator
(`/usr/lib/podman/quadlet`), exactly as before.

#### Scenario: a valid single quadlet

- **WHEN** the user runs `qctl check stack.container` on a valid quadlet
- **THEN** the generator reports success and `check` exits 0

### Requirement: check SHALL audit the stacks root when called without an argument

`qctl check` with no argument SHALL scan every stack under the stacks root
(`$HOME/.config/containers/available/` by default, overridable with `--stacks-root`) and
report: hard collisions (the same unit basename in two stacks, or the same `ContainerName=`
in two quadlets) as errors; and units whose basename is neither `<stack>.<ext>` nor
`<stack>-<role>` as warnings. It SHALL exit non-zero when at least one hard collision exists,
and 0 otherwise (warnings alone SHALL NOT fail).

#### Scenario: a clean stacks root

- **WHEN** no basename or `ContainerName=` is duplicated across stacks
- **THEN** `check` reports no hard collision and exits 0

#### Scenario: a hard collision fails

- **WHEN** two stacks declare the same unit basename
- **THEN** `check` reports it as a hard collision and exits non-zero

#### Scenario: a naming-convention warning does not fail

- **WHEN** a unit named `weird.container` is neither `<stack>.<ext>` nor `<stack>-<role>`, and there are no collisions
- **THEN** `check` warns about the naming and still exits 0

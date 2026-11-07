# check Specification

## Purpose
Defines `qctl check`: validating a single quadlet against the real Podman generator, and
auditing a whole stacks root for cross-stack collisions and naming-convention drift.

## Requirements

### Requirement: check SHALL validate a single quadlet file

`qctl check <QUADLET>` SHALL validate the given quadlet with the real generator, invoking it
as `-dryrun -user <uid> <quadlet>`.

#### Scenario: a valid single quadlet

- **WHEN** the user runs `qctl check stack.container` on a valid quadlet
- **THEN** the generator reports success and `check` exits 0

#### Scenario: check resolves the generator from the environment override first

- **WHEN** `QCTL_QUADLET_BIN` points at an existing, executable generator binary
- **THEN** `qctl check` invokes exactly that binary, regardless of any distro candidate paths

#### Scenario: check resolves the generator from the distro candidate paths

- **WHEN** `QCTL_QUADLET_BIN` is unset and `/usr/libexec/podman/quadlet` exists and is executable
- **THEN** `qctl check` invokes `/usr/libexec/podman/quadlet`

#### Scenario: check falls back to the next candidate path

- **WHEN** `QCTL_QUADLET_BIN` is unset, `/usr/libexec/podman/quadlet` does not exist and
  `/usr/lib/podman/quadlet` exists and is executable
- **THEN** `qctl check` invokes `/usr/lib/podman/quadlet`

#### Scenario: check fails with a clear error when no generator is found

- **WHEN** `QCTL_QUADLET_BIN` is unset and neither candidate path exists
- **THEN** `qctl check` fails with a non-zero exit code and an error that lists the probed
  paths and mentions the `QCTL_QUADLET_BIN` override

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

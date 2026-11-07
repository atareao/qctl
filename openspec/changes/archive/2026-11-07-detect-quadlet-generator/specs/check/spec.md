# check Specification

## Purpose

Defines `qctl check`: validating a single quadlet against the real Podman generator, and
auditing a whole stacks root for cross-stack collisions and naming-convention drift.

## MODIFIED Requirements

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

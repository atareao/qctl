# Spec Delta

## Purpose

Define cómo `qctl` invoca las herramientas externas que puede esperar (`systemctl`, `journalctl`, `podman` y el generador `quadlet`): sin paginador interactivo y con un límite de tiempo explícito en las llamadas que pueden bloquearse.

## ADDED Requirements

### Requirement: External tools SHALL NOT open an interactive pager

Every invocation of a tool that can page its output MUST pass the flag that disables the pager. `journalctl` MUST be invoked with `--no-pager`, so a pager configured by the user can never take over the terminal of a command that is meant to be scriptable.

#### Scenario: logs disables the pager

- **WHEN** the user runs `qctl logs voicebox`
- **THEN** the `journalctl` invocation includes `--no-pager`

#### Scenario: logsf disables the pager

- **WHEN** the user runs `qctl logsf voicebox`
- **THEN** the `journalctl` invocation includes `--no-pager`

### Requirement: Bounded external commands SHALL run under a configurable timeout

`qctl` MUST apply a timeout to the external commands it waits for: `systemctl --user` (`start`, `stop`, `restart`, `daemon-reload` and `is-active`), `/usr/lib/podman/quadlet` and `podman volume rm`. The default timeout MUST be 60 seconds. A global `--timeout <SECONDS>` option MUST override it, and `0` MUST disable the limit for every bounded command. A value that is not a non-negative integer MUST be rejected before any work happens. When a command exceeds the timeout, `qctl` MUST terminate it, exit non-zero and print an error that names the command and, for an invocation that targets a unit, that unit.

#### Scenario: default timeout

- **WHEN** `systemctl` does not return and no `--timeout` is given
- **THEN** `qctl` aborts the command after 60 seconds, exits non-zero, and the error names the unit and the action

#### Scenario: custom timeout

- **WHEN** the user runs `qctl --timeout 1 stop voicebox` and `systemctl` does not return
- **THEN** `qctl` aborts within a few seconds and exits non-zero

#### Scenario: timeout disabled

- **WHEN** the user passes `--timeout 0` and the external command takes longer than 60 seconds
- **THEN** the command is not aborted and no timeout error is produced

#### Scenario: quadlet check is bounded

- **WHEN** `/usr/lib/podman/quadlet` does not return during `qctl check`
- **THEN** `qctl` aborts after the configured timeout and exits non-zero

#### Scenario: invalid timeout value

- **WHEN** the user passes a negative or non-numeric value to `--timeout`
- **THEN** the command is rejected with a usage error before doing any work

### Requirement: Network-bound pulls SHALL NOT be bounded by the timeout

`podman pull` MUST be exempt from the timeout. It depends on the network and may legitimately take longer than the default, so a bounded value would abort valid image updates.

#### Scenario: a slow pull is not aborted

- **WHEN** `podman pull` runs longer than the configured timeout during `qctl update`
- **THEN** `qctl` waits for it to finish instead of aborting it

### Requirement: Follow-mode log streaming SHALL NOT be bounded by the timeout

`logs` and `logsf` MUST NOT be subject to the timeout. Both run `journalctl` in follow mode (`-f`) and are meant to stream until the user interrupts them.

#### Scenario: logs keeps streaming past the timeout

- **WHEN** the user runs `qctl --timeout 1 logs voicebox` and `journalctl` runs longer than one second
- **THEN** `qctl` does not abort it

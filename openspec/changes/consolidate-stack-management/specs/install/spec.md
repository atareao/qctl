# Spec Delta

## ADDED Requirements

### Requirement: install SHALL refuse to link a stack that collides globally

Before creating any symlink, `install` SHALL scan every stack under the stacks root
(`$HOME/.config/containers/available/` by default, overridable with `--stacks-root`) and
SHALL abort, creating no symlink at all, when the quadlets to be installed would collide
with another stack: either the same unit basename in two different stacks, or the same
`ContainerName=` value in two different quadlets. On abort, `install` SHALL exit non-zero
and report the colliding names and the stacks involved.

#### Scenario: a basename collision aborts atomically

- **WHEN** stack `a` declares `x.container` and stack `b` also declares `x.container`, and the user runs `qctl install` from stack `b`
- **THEN** no symlink is created in the target directory
- **AND** `install` exits non-zero and reports the colliding basename `x.container`

#### Scenario: a ContainerName collision aborts atomically

- **WHEN** `a.container` has `ContainerName=foo` and `b.container` has `ContainerName=foo`, and the user runs `qctl install` from stack `b`
- **THEN** no symlink is created and `install` exits non-zero, reporting `ContainerName=foo`

#### Scenario: no collision installs normally

- **WHEN** the stack's units collide with no other stack and the user runs `qctl install`
- **THEN** the symlinks are created and systemd is reloaded as before

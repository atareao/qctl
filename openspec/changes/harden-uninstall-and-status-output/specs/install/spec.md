# Spec Delta

## ADDED Requirements

### Requirement: uninstall SHALL stop every generated unit it unlinks

When `uninstall` removes a symlink it owns, it SHALL first stop the systemd user unit
the quadlet generator derives from that file, whatever its type: `<stem>.service` for
a `.container`, `<stem>-volume.service` for a `.volume` and `<stem>-network.service`
for a `.network`. `uninstall` SHALL NOT leave a generated unit active for a quadlet it
no longer links. Regular (non-symlink) files in the target directory SHALL remain
untouched and SHALL never be stopped.

#### Scenario: uninstall of a stack that declares a volume

- **WHEN** `microbin.container` and `microbin-microbin.volume` are linked in the target directory and the user runs `qctl uninstall`
- **THEN** `systemctl --user stop microbin.service` and `systemctl --user stop microbin-microbin-volume.service` both run
- **AND** both symlinks are removed from the target directory

#### Scenario: uninstall of a stack that declares a network

- **WHEN** `stack.container` and `stack.network` are linked in the target directory and the user runs `qctl uninstall`
- **THEN** `systemctl --user stop stack.service` and `systemctl --user stop stack-network.service` both run
- **AND** both symlinks are removed from the target directory

#### Scenario: container units keep being stopped as before

- **WHEN** only `voicebox.container` is linked in the target directory and the user runs `qctl uninstall`
- **THEN** `systemctl --user stop voicebox.service` runs and the symlink is removed

#### Scenario: a regular file is still not stopped nor removed

- **WHEN** the target directory contains the regular file `one.container` and the user runs `qctl uninstall`
- **THEN** no `systemctl --user stop` runs for `one.service`
- **AND** `one.container` is left in place

#### Scenario: a failed stop does not abort the removal

- **WHEN** `systemctl --user stop` exits non-zero for a linked `.volume` unit
- **THEN** `uninstall` warns and still removes the symlink and continues with the remaining quadlets

use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("qctl-{name}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).expect("failed to create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_file(path: &Path, content: &str) {
    fs::write(path, content).expect("failed to write test file");
}

fn run_qctl(home: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qctl"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USER", "qctl-test")
        .env_remove("RUST_LOG")
        .output()
        .expect("failed to run qctl")
}

fn run_qctl_with_stdin(home: &Path, cwd: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_qctl"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USER", "qctl-test")
        .env_remove("RUST_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn qctl");

    child
        .stdin
        .as_mut()
        .expect("stdin not piped")
        .write_all(stdin.as_bytes())
        .expect("failed to write stdin");

    child.wait_with_output().expect("failed to wait for qctl")
}

/// Runs qctl with stdin closed (immediately at EOF) and fails the test if the
/// process does not exit within `timeout`, killing it first so the suite never
/// hangs. Returns `None` on timeout.
fn run_qctl_with_closed_stdin(
    home: &Path,
    cwd: &Path,
    args: &[&str],
    timeout: Duration,
) -> Option<Output> {
    let child = Command::new(env!("CARGO_BIN_EXE_qctl"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USER", "qctl-test")
        .env_remove("RUST_LOG")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn qctl");

    wait_with_timeout(child, timeout)
}

fn wait_with_timeout(mut child: Child, timeout: Duration) -> Option<Output> {
    let deadline = Instant::now() + timeout;
    loop {
        if child
            .try_wait()
            .expect("failed to poll qctl process")
            .is_some()
        {
            return Some(
                child
                    .wait_with_output()
                    .expect("failed to collect qctl output"),
            );
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

fn run_qctl_with_path(home: &Path, cwd: &Path, path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qctl"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USER", "qctl-test")
        .env_remove("RUST_LOG")
        .env("PATH", path)
        .output()
        .expect("failed to run qctl")
}

fn prepend_path(path: &Path) -> String {
    let current_path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![path.to_path_buf()];
    paths.extend(std::env::split_paths(&current_path));
    std::env::join_paths(paths)
        .expect("failed to join PATH")
        .to_string_lossy()
        .to_string()
}

fn fake_systemctl(bin: &Path, log: &Path) {
    let script = format!("#!/bin/sh\necho \"$@\" >> {}\nexit 0\n", log.display());
    let systemctl = bin.join("systemctl");
    write_file(&systemctl, &script);
    fs::set_permissions(&systemctl, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fake systemctl");
}

fn fake_podman(bin: &Path, log: &Path) {
    let script = format!("#!/bin/sh\necho \"$@\" >> {}\nexit 0\n", log.display());
    let podman = bin.join("podman");
    write_file(&podman, &script);
    fs::set_permissions(&podman, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fake podman");
}

fn fake_journalctl(bin: &Path, log: &Path) {
    let script = format!("#!/bin/sh\necho \"$@\" >> {}\nexit 0\n", log.display());
    let journalctl = bin.join("journalctl");
    write_file(&journalctl, &script);
    fs::set_permissions(&journalctl, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fake journalctl");
}

/// A fake `systemctl` that fails for `failbox.service` and `failbox2.service`
/// and, for `okbox.service`, writes `START` then sleeps then writes `END` so
/// tests can prove qctl waited for every unit before reporting.
fn fake_failing_start_systemctl(bin: &Path, log: &Path) {
    let script = format!(
        r#"#!/bin/sh
echo "$@" >> {log}
if [ "$2" = "start" ]; then
  if [ "$3" = "failbox.service" ] || [ "$3" = "failbox2.service" ]; then
    echo "start failed for $3" >&2
    exit 1
  fi
  if [ "$3" = "okbox.service" ]; then
    echo "START okbox.service" >> {log}
    sleep 1
    echo "END okbox.service" >> {log}
  fi
fi
if [ "$2" = "is-active" ]; then
  echo active
fi
exit 0
"#,
        log = log.display()
    );
    let systemctl = bin.join("systemctl");
    write_file(&systemctl, &script);
    fs::set_permissions(&systemctl, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fake systemctl");
}

/// Returns true when `path` is a symlink, inspected without following it (so a
/// broken link still counts).
fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

/// Recursively collects every symlink under `root` without following any of
/// them, so a test can prove a command left no stray links inside the target
/// directory.
fn symlinks_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            found.push(path);
        } else if meta.is_dir() {
            found.extend(symlinks_under(&path));
        }
    }
    found.sort();
    found
}

#[test]
fn version_flag_reports_crate_version() {
    let root = TestDir::new("version");
    let output = run_qctl(root.path(), root.path(), &["--version"]);

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "expected version {} in stdout: {stdout}",
        env!("CARGO_PKG_VERSION")
    );
}

#[test]
fn dry_run_install_reports_actions_without_touching_home() {
    let root = TestDir::new("dry-run-install");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let output = run_qctl(&home, &project, &["--dry-run", "install"]);
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DRY-RUN create"));
    assert!(stdout.contains("DRY-RUN link"));
    assert!(stdout.contains("DRY-RUN systemctl --user daemon-reload"));
    assert!(!home.join(".config/containers/systemd").exists());
}

#[test]
fn install_creates_symlink_and_reload_with_fake_systemctl() {
    let root = TestDir::new("install");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_qctl"))
        .arg("install")
        .current_dir(&project)
        .env("HOME", &home)
        .env("USER", "qctl-test")
        .env_remove("RUST_LOG")
        .env("PATH", prepend_path(&bin))
        .output()
        .expect("failed to run qctl");
    assert!(output.status.success(), "{output:?}");

    let link = home
        .join(".config")
        .join("containers")
        .join("systemd")
        .join("voicebox.container");
    assert_eq!(
        fs::read_link(&link).expect("install should create symlink"),
        project.join("voicebox.container")
    );
    assert!(fs::read_to_string(&log)
        .expect("missing systemctl log")
        .contains("--user daemon-reload"));
}

#[test]
fn uninstall_removes_symlink_and_stops_container_unit() {
    let root = TestDir::new("uninstall");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    let target = home.join(".config").join("containers").join("systemd");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    let source = project.join("voicebox.container");
    let link = target.join("voicebox.container");
    write_file(&source, "[Container]\nImage=example\n");
    symlink(&source, &link).expect("failed to create test symlink");

    let output = Command::new(env!("CARGO_BIN_EXE_qctl"))
        .arg("uninstall")
        .current_dir(&project)
        .env("HOME", &home)
        .env("USER", "qctl-test")
        .env_remove("RUST_LOG")
        .env("PATH", prepend_path(&bin))
        .output()
        .expect("failed to run qctl");
    assert!(output.status.success(), "{output:?}");

    assert!(!link.exists());
    let log = fs::read_to_string(&log).expect("missing systemctl log");
    assert!(log.contains("--user stop voicebox.service"));
    assert!(log.contains("--user daemon-reload"));
}

#[test]
fn uninstall_stop_warning_goes_to_stderr() {
    let root = TestDir::new("uninstall-warn");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    let target = home.join(".config").join("containers").join("systemd");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");

    let script = format!(
        r#"#!/bin/sh
echo "$@" >> {log}
if [ "$2" = "stop" ]; then
  echo "Unit voicebox.service not loaded." >&2
  exit 5
fi
exit 0
"#,
        log = log.display()
    );
    let systemctl = bin.join("systemctl");
    write_file(&systemctl, &script);
    fs::set_permissions(&systemctl, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fake systemctl");

    let source = project.join("voicebox.container");
    let link = target.join("voicebox.container");
    write_file(&source, "[Container]\nImage=example\n");
    symlink(&source, &link).expect("failed to create test symlink");

    let output = Command::new(env!("CARGO_BIN_EXE_qctl"))
        .arg("uninstall")
        .current_dir(&project)
        .env("HOME", &home)
        .env("USER", "qctl-test")
        .env_remove("RUST_LOG")
        .env("PATH", prepend_path(&bin))
        .output()
        .expect("failed to run qctl");
    assert!(output.status.success(), "{output:?}");

    // A stop failure is a diagnostic: it must be readable when only stderr is
    // redirected, and must not pollute the stdout a script parses.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to stop voicebox"),
        "stop warning must reach stderr: {stderr}"
    );
    assert!(
        !stdout.contains("failed to stop voicebox"),
        "stop warning must not go to stdout: {stdout}"
    );
    assert!(!link.exists(), "uninstall must still remove the link");
}

#[test]
fn start_stop_restart_call_expected_systemctl_units() {
    let root = TestDir::new("service-actions");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    let target = home.join(".config").join("containers").join("systemd");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    let source = project.join("voicebox.container");
    write_file(&source, "[Container]\nImage=example\n");
    symlink(&source, target.join("voicebox.container")).expect("failed to create test symlink");

    for action in ["start", "stop", "restart"] {
        let output = Command::new(env!("CARGO_BIN_EXE_qctl"))
            .args([action, "voicebox"])
            .current_dir(&project)
            .env("HOME", &home)
            .env("USER", "qctl-test")
            .env_remove("RUST_LOG")
            .env("PATH", prepend_path(&bin))
            .output()
            .expect("failed to run qctl");
        assert!(output.status.success(), "{action}: {output:?}");
    }

    let log = fs::read_to_string(&log).expect("missing systemctl log");
    assert!(log.contains("--user start voicebox.service"));
    assert!(log.contains("--user stop voicebox.service"));
    assert!(log.contains("--user restart voicebox.service"));
}

#[test]
fn clean_volumes_requires_confirmation_by_default() {
    let root = TestDir::new("clean-confirm");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    write_file(
        &project.join("data.volume"),
        "[Volume]\nVolumeName=qctl-test-data\n",
    );

    let output = run_qctl_with_stdin(&home, &project, &["clean-volumes"], "n\n");
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("This will remove 1 podman volume(s):"));
    assert!(stdout.contains("qctl-test-data"));
    assert!(stdout.contains("Continue? [y/N]:"));
    assert!(stdout.contains("Aborted"));
}

#[test]
fn dry_run_clean_volumes_does_not_prompt_or_call_podman() {
    let root = TestDir::new("dry-run-clean");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    write_file(
        &project.join("data.volume"),
        "[Volume]\nVolumeName=qctl-test-data\n",
    );

    let output = run_qctl(&home, &project, &["--dry-run", "clean-volumes"]);
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DRY-RUN podman volume rm -f qctl-test-data"));
    assert!(!stdout.contains("Continue?"));
}

#[test]
fn compact_status_lists_missing_container_without_summary() {
    let root = TestDir::new("status-compact");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let output = run_qctl(&home, &project, &["status", "--compact"]);
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("voicebox\t❌\t⚫"));
    assert!(!stdout.contains("Summary:"));
}

#[test]
fn duplicate_quadlet_names_fail_with_clear_error() {
    let root = TestDir::new("duplicate");
    let project = root.path().join("project");
    let quadlets = project.join("quadlets");
    let home = root.path().join("home");
    fs::create_dir_all(&quadlets).expect("failed to create quadlets");
    fs::create_dir_all(&home).expect("failed to create home");
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );
    write_file(
        &quadlets.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let output = run_qctl(&home, &project, &["status", "--compact"]);
    assert!(!output.status.success(), "{output:?}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("duplicate quadlet file name 'voicebox.container'"));
}

#[test]
fn external_command_failures_include_status_and_stderr() {
    let root = TestDir::new("external-error");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    fs::create_dir_all(&bin).expect("failed to create bin");
    write_file(
        &project.join("data.volume"),
        "[Volume]\nVolumeName=qctl-test-data\n",
    );

    let podman = bin.join("podman");
    write_file(&podman, "#!/bin/sh\necho podman exploded >&2\nexit 42\n");
    fs::set_permissions(&podman, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fake podman");

    let output = run_qctl_with_path(&home, &project, &bin, &["clean-volumes", "--yes"]);
    assert!(!output.status.success(), "{output:?}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("command failed: podman volume rm -f qctl-test-data"));
    assert!(stderr.contains("exit status: 42"));
    assert!(stderr.contains("podman exploded"));
}

#[test]
fn dry_run_update_does_not_call_podman() {
    let root = TestDir::new("dry-run-update");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("podman.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_podman(&bin, &log);
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["--dry-run", "update"]);
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DRY-RUN podman pull"));
    assert!(
        stdout.contains("DRY-RUN summary: 1 would be pulled"),
        "dry-run summary must not claim updates: {stdout}"
    );
    // Any casing of the verb would mean `update` reported real work in dry-run.
    assert!(
        !stdout.to_lowercase().contains("updated"),
        "dry-run must not report updates: {stdout}"
    );

    let logged = fs::read_to_string(&log).unwrap_or_default();
    assert!(logged.is_empty(), "podman should not be called: {logged}");
}

#[test]
fn logsf_uses_extra_journalctl_flags() {
    let root = TestDir::new("logsf");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("journalctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_journalctl(&bin, &log);
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["logsf", "voicebox"]);
    assert!(output.status.success(), "{output:?}");

    let logged = fs::read_to_string(&log).expect("missing journalctl log");
    assert!(
        logged.contains("--user -xsfu voicebox.service"),
        "logsf must pass extra flags: {logged}"
    );
}

#[test]
fn logs_uses_plain_journalctl_flags() {
    let root = TestDir::new("logs");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("journalctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_journalctl(&bin, &log);
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["logs", "voicebox"]);
    assert!(output.status.success(), "{output:?}");

    let logged = fs::read_to_string(&log).expect("missing journalctl log");
    assert!(
        logged.contains("--user -u voicebox.service -f --since 1 hour ago"),
        "logs must pass plain flags: {logged}"
    );
}

#[test]
fn menu_exits_on_eof_without_looping() {
    let root = TestDir::new("menu-eof");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let output = run_qctl_with_closed_stdin(&home, &project, &["menu"], Duration::from_secs(10))
        .expect("menu did not exit on EOF");
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.len() < 10_000,
        "menu must not spin on EOF, produced {} bytes",
        stdout.len()
    );
}

#[test]
fn start_reports_all_unit_failures() {
    let root = TestDir::new("start-failures");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    let target = home.join(".config").join("containers").join("systemd");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_failing_start_systemctl(&bin, &log);
    for name in ["failbox", "failbox2", "okbox"] {
        let source = project.join(format!("{name}.container"));
        write_file(&source, "[Container]\nImage=example\n");
        symlink(&source, target.join(format!("{name}.container")))
            .expect("failed to create test symlink");
    }

    let child = Command::new(env!("CARGO_BIN_EXE_qctl"))
        .arg("start")
        .current_dir(&project)
        .env("HOME", &home)
        .env("USER", "qctl-test")
        .env_remove("RUST_LOG")
        .env("PATH", prepend_path(&bin))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn qctl");
    let output = wait_with_timeout(child, Duration::from_secs(10))
        .expect("qctl start did not finish in time");

    assert!(!output.status.success(), "start must fail: {output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed for 2 unit(s)"),
        "aggregated error must count both failures: {stderr}"
    );

    // Assert on the aggregated report itself, not merely on the unit names
    // appearing anywhere in stderr: the child stderr is forwarded too, so a
    // bare `contains("failbox")` would pass even if failures were dropped.
    let aggregated = stderr
        .split_once("failed for 2 unit(s):")
        .map(|(_, rest)| rest)
        .expect("aggregated failure block missing");
    for unit in ["failbox", "failbox2"] {
        assert!(
            aggregated
                .lines()
                .any(|line| line.trim_start().starts_with(&format!("{unit}:"))),
            "aggregated error must carry a context line for {unit}: {aggregated}"
        );
    }

    // The healthy unit must have run to completion, proving every task was
    // awaited instead of bailing out on the first failure.
    let logged = fs::read_to_string(&log).expect("missing systemctl log");
    assert!(
        logged.contains("END okbox.service"),
        "start must wait for every unit before returning: {logged}"
    );
}

#[test]
fn dry_run_start_does_not_invoke_systemctl() {
    let root = TestDir::new("dry-run-start");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(
        &home,
        &project,
        Path::new(&path),
        &["--dry-run", "start", "voicebox"],
    );
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DRY-RUN systemctl --user start voicebox.service"));
    assert!(
        fs::read_to_string(&log).unwrap_or_default().is_empty(),
        "systemctl must not run in dry-run"
    );
}

#[test]
fn dry_run_stop_does_not_invoke_systemctl() {
    let root = TestDir::new("dry-run-stop");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(
        &home,
        &project,
        Path::new(&path),
        &["--dry-run", "stop", "voicebox"],
    );
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DRY-RUN systemctl --user stop voicebox.service"));
    assert!(
        fs::read_to_string(&log).unwrap_or_default().is_empty(),
        "systemctl must not run in dry-run"
    );
}

#[test]
fn dry_run_restart_does_not_invoke_systemctl() {
    let root = TestDir::new("dry-run-restart");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(
        &home,
        &project,
        Path::new(&path),
        &["--dry-run", "restart", "voicebox"],
    );
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DRY-RUN systemctl --user restart voicebox.service"));
    assert!(
        fs::read_to_string(&log).unwrap_or_default().is_empty(),
        "systemctl must not run in dry-run"
    );
}

#[test]
fn dry_run_uninstall_does_not_stop_or_remove() {
    let root = TestDir::new("dry-run-uninstall");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    let target = home.join(".config").join("containers").join("systemd");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    let source = project.join("voicebox.container");
    let link = target.join("voicebox.container");
    write_file(&source, "[Container]\nImage=example\n");
    symlink(&source, &link).expect("failed to create test symlink");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(
        &home,
        &project,
        Path::new(&path),
        &["--dry-run", "uninstall"],
    );
    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DRY-RUN systemctl --user stop voicebox.service"));
    assert!(stdout.contains("DRY-RUN remove"));

    assert!(link.exists(), "dry-run uninstall must not remove the link");
    let logged = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        !logged.contains("--user stop"),
        "systemctl stop must not run in dry-run: {logged}"
    );
    assert!(
        !logged.contains("daemon-reload"),
        "daemon-reload must not run in dry-run: {logged}"
    );
}

#[test]
fn install_skips_quadlets_already_in_target_dir() {
    let root = TestDir::new("install-target-dir");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    let source = target.join("voicebox.container");
    write_file(&source, "[Container]\nImage=example\n");

    let output = Command::new(env!("CARGO_BIN_EXE_qctl"))
        .arg("install")
        .current_dir(&target)
        .env("HOME", &home)
        .env("USER", "qctl-test")
        .env_remove("RUST_LOG")
        .env("PATH", prepend_path(&bin))
        .output()
        .expect("failed to run qctl");
    assert!(output.status.success(), "{output:?}");

    let metadata = fs::symlink_metadata(&source).expect("quadlet should still exist");
    assert!(
        metadata.file_type().is_file(),
        "quadlet must stay a regular file, not a self-referential symlink"
    );
    assert_eq!(
        fs::read_to_string(&source).expect("failed to read quadlet"),
        "[Container]\nImage=example\n"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Skipping voicebox.container (already in target dir)"),
        "{stdout}"
    );
}

#[test]
fn install_skips_quadlets_in_subdirectories_of_target_dir() {
    let root = TestDir::new("install-subdir-of-target");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let nested = target.join("quadlets");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&nested).expect("failed to create nested target dir");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    write_file(
        &nested.join("other.container"),
        "[Container]\nImage=example\n",
    );

    let path = prepend_path(&bin);
    let first = run_qctl_with_path(&home, &target, Path::new(&path), &["install"]);
    assert!(first.status.success(), "first install failed: {first:?}");

    // A quadlet living in a subdirectory of the target dir is already
    // installed: nothing may be linked into the target dir itself.
    assert!(
        !target.join("other.container").exists(),
        "install must not link a quadlet that already lives inside the target dir"
    );
    assert!(
        symlinks_under(&target).is_empty(),
        "install must not create any symlink inside the target dir: {:?}",
        symlinks_under(&target)
    );

    let second = run_qctl_with_path(&home, &target, Path::new(&path), &["install"]);
    let second_err = String::from_utf8_lossy(&second.stderr);
    assert!(
        second.status.success(),
        "second install must succeed instead of hitting a duplicate: {second:?}"
    );
    assert!(
        !second_err.contains("duplicate quadlet file name"),
        "second install reported a duplicate: {second_err}"
    );
}

#[test]
fn install_recovers_from_stale_self_link() {
    let root = TestDir::new("install-stale-self-link");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let nested = target.join("quadlets");
    let link = target.join("other.container");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&nested).expect("failed to create nested target dir");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    write_file(
        &nested.join("other.container"),
        "[Container]\nImage=example\n",
    );
    // Reproduce the state older versions left behind: a link inside the target
    // dir resolving to a file that already lives inside the target dir.
    symlink(Path::new("quadlets/other.container"), &link)
        .expect("failed to create stale self link");
    assert!(is_symlink(&link), "setup must leave a symlink behind");

    let path = prepend_path(&bin);
    let install = run_qctl_with_path(&home, &target, Path::new(&path), &["install"]);
    let install_err = String::from_utf8_lossy(&install.stderr);
    assert!(
        install.status.success(),
        "install must recover from a stale self link: {install:?}"
    );
    assert!(
        !install_err.contains("duplicate quadlet file name"),
        "install reported a duplicate: {install_err}"
    );

    let uninstall = run_qctl_with_path(&home, &target, Path::new(&path), &["uninstall"]);
    let uninstall_err = String::from_utf8_lossy(&uninstall.stderr);
    assert!(
        uninstall.status.success(),
        "uninstall must recover from a stale self link: {uninstall:?}"
    );
    assert!(
        !uninstall_err.contains("duplicate quadlet file name"),
        "uninstall reported a duplicate: {uninstall_err}"
    );
    assert!(
        fs::symlink_metadata(&link).is_err(),
        "uninstall must remove the stale self link"
    );
}

#[test]
fn uninstall_preserves_regular_files_in_target_dir() {
    let root = TestDir::new("uninstall-regular-file");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&target).expect("failed to create target dir");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    let file = target.join("one.container");
    write_file(&file, "[Container]\nImage=sentinel\n");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &target, Path::new(&path), &["uninstall"]);
    assert!(output.status.success(), "uninstall failed: {output:?}");

    assert!(
        !is_symlink(&file),
        "uninstall must not turn a regular file into a symlink"
    );
    assert_eq!(
        fs::read_to_string(&file).expect("uninstall must leave a regular file in the target dir"),
        "[Container]\nImage=sentinel\n"
    );
}

#[test]
fn install_does_not_replace_regular_file_in_target_dir() {
    let root = TestDir::new("install-regular-file");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    write_file(&project.join("one.container"), "[Container]\nImage=one\n");
    write_file(&project.join("two.container"), "[Container]\nImage=two\n");
    let sentinel = "[Container]\nImage=keep-me\n";
    write_file(&target.join("one.container"), sentinel);

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["install"]);
    assert!(output.status.success(), "install failed: {output:?}");

    let shadowed = target.join("one.container");
    assert!(
        !is_symlink(&shadowed),
        "install must not replace a regular file with a symlink"
    );
    assert_eq!(
        fs::read_to_string(&shadowed).expect("regular file must survive install"),
        sentinel
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("one.container") && stderr.to_lowercase().contains("symlink"),
        "install must warn on stderr that one.container is not a symlink: {stderr}"
    );

    let linked = target.join("two.container");
    assert!(is_symlink(&linked), "install must still link two.container");
    assert_eq!(
        fs::read_link(&linked).expect("two.container must be a symlink"),
        project.join("two.container")
    );
}

#[test]
fn start_unknown_service_fails_without_installing() {
    let root = TestDir::new("start-unknown");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    write_file(&project.join("one.container"), "[Container]\nImage=one\n");
    write_file(&project.join("two.container"), "[Container]\nImage=two\n");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["start", "typo"]);
    assert!(!output.status.success(), "start typo must fail: {output:?}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no such quadlet: typo"),
        "start must name the unknown quadlet: {stderr}"
    );

    assert!(
        !target.join("one.container").exists() && !target.join("two.container").exists(),
        "start typo must not install any quadlet"
    );
    let logged = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        logged.is_empty(),
        "start typo must not invoke systemctl: {logged}"
    );
}

#[test]
fn stop_and_restart_unknown_service_fail() {
    let root = TestDir::new("stop-restart-unknown");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    write_file(&project.join("one.container"), "[Container]\nImage=one\n");

    let path = prepend_path(&bin);
    for action in ["stop", "restart"] {
        let output = run_qctl_with_path(&home, &project, Path::new(&path), &[action, "typo"]);
        assert!(
            !output.status.success(),
            "{action} typo must fail: {output:?}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("no such quadlet: typo"),
            "{action} must name the unknown quadlet: {stderr}"
        );
    }
}

#[test]
fn update_unknown_service_fails() {
    let root = TestDir::new("update-unknown");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    write_file(&project.join("one.container"), "[Container]\nImage=one\n");

    let output = run_qctl(&home, &project, &["update", "typo"]);
    assert!(
        !output.status.success(),
        "update typo must fail: {output:?}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no such quadlet: typo"),
        "update must name the unknown quadlet: {stderr}"
    );
}

#[test]
fn start_service_links_only_that_unit() {
    let root = TestDir::new("start-one-unit");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    write_file(&project.join("one.container"), "[Container]\nImage=one\n");
    write_file(&project.join("two.container"), "[Container]\nImage=two\n");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["start", "one"]);
    assert!(output.status.success(), "start one failed: {output:?}");

    let linked = target.join("one.container");
    assert!(is_symlink(&linked), "start one must link one.container");
    assert_eq!(
        fs::read_link(&linked).expect("one.container must be a symlink"),
        project.join("one.container")
    );
    assert!(
        !target.join("two.container").exists(),
        "start one must not install unrelated quadlets"
    );
}

#[test]
fn installed_unit_survives_missing_source() {
    let root = TestDir::new("installed-missing-source");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    let source = project.join("svc.container");
    write_file(&source, "[Container]\nImage=svc\n");

    let path = prepend_path(&bin);
    let install = run_qctl_with_path(&home, &project, Path::new(&path), &["install"]);
    assert!(
        install.status.success(),
        "setup install failed: {install:?}"
    );
    assert!(is_symlink(&target.join("svc.container")));

    // The project no longer provides svc.container, but it is installed.
    fs::remove_file(&source).expect("failed to remove project source");

    for action in ["start", "stop"] {
        let output = run_qctl_with_path(&home, &project, Path::new(&path), &[action, "svc"]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("no such quadlet"),
            "{action} svc must accept a unit still present in the target dir: {stderr}"
        );
        assert!(
            output.status.success(),
            "{action} svc must succeed: {output:?}"
        );
    }
}

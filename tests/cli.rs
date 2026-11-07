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
    assert!(stdout.contains("voicebox\tmissing\t-"));
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
    assert!(
        logged.contains("--no-pager"),
        "logsf must not hand the terminal over to a pager: {logged}"
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
    assert!(
        logged.contains("--no-pager"),
        "logs must not hand the terminal over to a pager: {logged}"
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

    let first_out = String::from_utf8_lossy(&first.stdout);
    assert!(
        first_out.contains("Skipping other.container (already in target dir)"),
        "install must report the nested quadlet it skips: {first_out}"
    );

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

#[test]
fn start_service_already_in_target_dir_links_nothing() {
    let root = TestDir::new("start-single-in-target");
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
    let output = run_qctl_with_path(&home, &target, Path::new(&path), &["start", "other"]);
    assert!(output.status.success(), "start other failed: {output:?}");

    let logged = fs::read_to_string(&log).expect("missing systemctl log");
    assert!(
        logged.contains("--user start other.service"),
        "start other must start the unit that lives in a target subdirectory: {logged}"
    );

    assert!(
        fs::symlink_metadata(target.join("other.container")).is_err(),
        "start other must not create an entry for a quadlet already in the target dir"
    );
    assert!(
        symlinks_under(&target).is_empty(),
        "start other must not create any symlink inside the target dir: {:?}",
        symlinks_under(&target)
    );
}

#[test]
fn start_without_arguments_starts_units_in_target_subdirs() {
    let root = TestDir::new("start-subdir-of-target");
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
    let output = run_qctl_with_path(&home, &target, Path::new(&path), &["start"]);
    assert!(output.status.success(), "start must succeed: {output:?}");

    let logged = fs::read_to_string(&log).expect("missing systemctl log");
    assert!(
        logged.contains("--user start other.service"),
        "start without arguments must start a unit living in a target subdirectory: {logged}"
    );
    assert!(
        symlinks_under(&target).is_empty(),
        "start must not create any symlink inside the target dir: {:?}",
        symlinks_under(&target)
    );
}

/// Runs qctl with a fake `PATH` and closed stdin, killing it (and returning
/// `None`) if it outlives `timeout`, so a suite exercising external timeouts
/// can never hang the runner.
fn run_qctl_with_path_and_timeout(
    home: &Path,
    cwd: &Path,
    path: &Path,
    args: &[&str],
    timeout: Duration,
) -> Option<Output> {
    let child = Command::new(env!("CARGO_BIN_EXE_qctl"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USER", "qctl-test")
        .env_remove("RUST_LOG")
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn qctl");

    wait_with_timeout(child, timeout)
}

fn fake_slow_systemctl(bin: &Path, log: &Path) {
    let script = format!(
        "#!/bin/sh\necho \"$@\" >> {}\nsleep 3\nexit 0\n",
        log.display()
    );
    let systemctl = bin.join("systemctl");
    write_file(&systemctl, &script);
    fs::set_permissions(&systemctl, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fake systemctl");
}

fn fake_slow_podman(bin: &Path, log: &Path) {
    let script = format!(
        "#!/bin/sh\necho \"$@\" >> {}\nsleep 3\nexit 0\n",
        log.display()
    );
    let podman = bin.join("podman");
    write_file(&podman, &script);
    fs::set_permissions(&podman, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fake podman");
}

fn fake_slow_journalctl(bin: &Path, log: &Path) {
    let script = format!(
        "#!/bin/sh\necho \"$@\" >> {}\nsleep 3\nexit 0\n",
        log.display()
    );
    let journalctl = bin.join("journalctl");
    write_file(&journalctl, &script);
    fs::set_permissions(&journalctl, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fake journalctl");
}

/// Layout shared by the local-scope tests: two projects, each owning one
/// container quadlet, both linked into the same target dir.
///
/// The unit names are deliberately distinguishable (`alpha` vs `bravo`) so a
/// test can assert on a substring without matching the wrong unit, and the
/// second project is only ever reached through the target dir — which is what
/// makes it a witness for an over-wide resolution scope.
struct LocalScopeLayout {
    home: PathBuf,
    proj_a: PathBuf,
    bin: PathBuf,
    log: PathBuf,
}

impl LocalScopeLayout {
    fn new(root: &Path, image: &str) -> Self {
        let home = root.join("home");
        let proj_a = root.join("projA");
        let proj_b = root.join("projB");
        let bin = root.join("bin");
        let log = root.join("systemctl.log");
        let target = home.join(".config").join("containers").join("systemd");

        fs::create_dir_all(&target).expect("failed to create target dir");
        fs::create_dir_all(&proj_a).expect("failed to create projA");
        fs::create_dir_all(&proj_b).expect("failed to create projB");
        fs::create_dir_all(&bin).expect("failed to create bin");

        let content = format!("[Container]\nImage={image}\n");
        for (project, unit) in [(&proj_a, "alpha"), (&proj_b, "bravo")] {
            let source = project.join(format!("{unit}.container"));
            write_file(&source, &content);
            symlink(&source, target.join(format!("{unit}.container")))
                .expect("failed to create test symlink");
        }

        Self {
            home,
            proj_a,
            bin,
            log,
        }
    }

    fn path(&self) -> String {
        prepend_path(&self.bin)
    }

    fn logged(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }
}

#[test]
fn stop_without_arguments_does_not_stop_other_project_units() {
    let root = TestDir::new("stop-local-scope");
    let layout = LocalScopeLayout::new(root.path(), "example");
    fake_systemctl(&layout.bin, &layout.log);

    let path = layout.path();
    let output = run_qctl_with_path(&layout.home, &layout.proj_a, Path::new(&path), &["stop"]);
    assert!(output.status.success(), "stop failed: {output:?}");

    let logged = layout.logged();
    assert!(
        logged.contains("--user stop alpha.service"),
        "stop must stop the current project's unit: {logged}"
    );
    assert!(
        !logged.contains("stop bravo.service"),
        "stop without arguments must not stop another project's unit: {logged}"
    );
}

#[test]
fn restart_without_arguments_does_not_restart_other_project_units() {
    let root = TestDir::new("restart-local-scope");
    let layout = LocalScopeLayout::new(root.path(), "example");
    fake_systemctl(&layout.bin, &layout.log);

    let path = layout.path();
    let output = run_qctl_with_path(&layout.home, &layout.proj_a, Path::new(&path), &["restart"]);
    assert!(output.status.success(), "restart failed: {output:?}");

    let logged = layout.logged();
    assert!(
        logged.contains("--user restart alpha.service"),
        "restart must restart the current project's unit: {logged}"
    );
    assert!(
        !logged.contains("restart bravo.service"),
        "restart without arguments must not restart another project's unit: {logged}"
    );
}

#[test]
fn start_without_arguments_does_not_start_other_project_units() {
    let root = TestDir::new("start-local-scope");
    let layout = LocalScopeLayout::new(root.path(), "example");
    fake_systemctl(&layout.bin, &layout.log);

    let path = layout.path();
    let output = run_qctl_with_path(&layout.home, &layout.proj_a, Path::new(&path), &["start"]);
    assert!(output.status.success(), "start failed: {output:?}");

    let logged = layout.logged();
    assert!(
        logged.contains("--user start alpha.service"),
        "start must start the current project's unit: {logged}"
    );
    assert!(
        !logged.contains("start bravo.service"),
        "start without arguments must not start another project's unit: {logged}"
    );
}

#[test]
fn stop_table_lists_only_the_current_project() {
    let root = TestDir::new("stop-table-local-scope");
    let layout = LocalScopeLayout::new(root.path(), "example");
    fake_systemctl(&layout.bin, &layout.log);

    let path = layout.path();
    let output = run_qctl_with_path(&layout.home, &layout.proj_a, Path::new(&path), &["stop"]);
    assert!(output.status.success(), "stop failed: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("alpha"),
        "the trailing table must list the current project's unit: {stdout}"
    );
    assert!(
        !stdout.contains("bravo"),
        "the trailing table must not list another project's unit: {stdout}"
    );
}

#[test]
fn status_reports_units_from_every_project() {
    let root = TestDir::new("status-wide-scope");
    let layout = LocalScopeLayout::new(root.path(), "example");
    fake_systemctl(&layout.bin, &layout.log);

    let path = layout.path();
    let output = run_qctl_with_path(&layout.home, &layout.proj_a, Path::new(&path), &["status"]);
    assert!(output.status.success(), "status failed: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("alpha"),
        "status must report the current project's unit: {stdout}"
    );
    assert!(
        stdout.contains("bravo"),
        "status must report units installed from other projects: {stdout}"
    );
}

#[test]
fn stop_with_explicit_name_still_resolves_target_dir() {
    let root = TestDir::new("stop-explicit-installed");
    let project = root.path().join("projA");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    let source = project.join("charlie.container");
    write_file(&source, "[Container]\nImage=example\n");

    let path = prepend_path(&bin);
    let install = run_qctl_with_path(&home, &project, Path::new(&path), &["install"]);
    assert!(
        install.status.success(),
        "setup install failed: {install:?}"
    );
    assert!(is_symlink(&target.join("charlie.container")));

    // The project no longer provides charlie.container, but it stays installed.
    fs::remove_file(&source).expect("failed to remove project source");

    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["stop", "charlie"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("no such quadlet"),
        "stop charlie must accept a unit still present in the target dir: {stderr}"
    );
    assert!(output.status.success(), "stop charlie failed: {output:?}");
}

#[test]
fn systemctl_timeout_fails_with_unit_and_action() {
    let root = TestDir::new("timeout-systemctl");
    let layout = LocalScopeLayout::new(root.path(), "example");
    fake_slow_systemctl(&layout.bin, &layout.log);

    let path = layout.path();
    let output = run_qctl_with_path_and_timeout(
        &layout.home,
        &layout.proj_a,
        Path::new(&path),
        &["--timeout", "1", "stop", "alpha"],
        Duration::from_secs(20),
    )
    .expect("qctl ignored the external timeout and never exited");

    assert!(
        !output.status.success(),
        "stop must fail once the unit outlives --timeout: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("alpha"),
        "the timeout error must name the unit it gave up on: {stderr}"
    );
}

#[test]
fn timeout_zero_disables_the_limit() {
    let root = TestDir::new("timeout-disabled");
    let layout = LocalScopeLayout::new(root.path(), "example");
    fake_slow_systemctl(&layout.bin, &layout.log);

    let path = layout.path();
    let start = Instant::now();
    let output = run_qctl_with_path_and_timeout(
        &layout.home,
        &layout.proj_a,
        Path::new(&path),
        &["--timeout", "0", "stop", "alpha"],
        Duration::from_secs(20),
    )
    .expect("qctl ignored the external timeout and never exited");
    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "--timeout 0 must let a slow command finish: {output:?}"
    );
    assert!(
        elapsed >= Duration::from_secs(2),
        "--timeout 0 must actually wait for the slow command: {elapsed:?}"
    );
}

#[test]
fn podman_pull_is_not_bounded_by_the_timeout() {
    let root = TestDir::new("timeout-podman");
    let layout = LocalScopeLayout::new(root.path(), "example.com/alpha");
    let log = root.path().join("podman.log");
    fake_slow_podman(&layout.bin, &log);

    let path = layout.path();
    let output = run_qctl_with_path_and_timeout(
        &layout.home,
        &layout.proj_a,
        Path::new(&path),
        &["--timeout", "1", "update"],
        Duration::from_secs(20),
    )
    .expect("qctl ignored the external timeout and never exited");

    assert!(
        output.status.success(),
        "a slow podman pull must not trip the unit timeout: {output:?}"
    );

    let logged = fs::read_to_string(&log).expect("missing podman log");
    assert!(
        logged.contains("pull example.com/alpha"),
        "update must pull every Image= it finds: {logged}"
    );
}

#[test]
fn logs_is_not_bounded_by_the_timeout() {
    let root = TestDir::new("timeout-logs");
    let layout = LocalScopeLayout::new(root.path(), "example");
    let log = root.path().join("journalctl.log");
    fake_slow_journalctl(&layout.bin, &log);

    let path = layout.path();
    let output = run_qctl_with_path_and_timeout(
        &layout.home,
        &layout.proj_a,
        Path::new(&path),
        &["--timeout", "1", "logs", "alpha"],
        Duration::from_secs(20),
    )
    .expect("qctl ignored the external timeout and never exited");

    assert!(
        output.status.success(),
        "a followed log stream must not trip the unit timeout: {output:?}"
    );

    let logged = fs::read_to_string(&log).expect("missing journalctl log");
    assert!(
        logged.contains("-f"),
        "logs must still follow the journal: {logged}"
    );
}

#[test]
fn negative_timeout_is_rejected() {
    let root = TestDir::new("timeout-negative");
    let output = run_qctl(root.path(), root.path(), &["--timeout", "-1", "status"]);

    assert!(
        !output.status.success(),
        "a negative timeout must be rejected: {output:?}"
    );
}

/// A fake `systemctl` that reports every queried unit as `active` on
/// `is-active` and logs every invocation, so a compact-status test can prove a
/// linked unit renders as `running`.
fn fake_active_systemctl(bin: &Path, log: &Path) {
    let script = format!(
        r#"#!/bin/sh
echo "$@" >> {log}
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

#[test]
fn uninstall_stops_generated_volume_unit() {
    let root = TestDir::new("uninstall-volume-unit");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);

    let container_src = project.join("microbin.container");
    let volume_src = project.join("microbin-microbin.volume");
    write_file(&container_src, "[Container]\nImage=example\n");
    write_file(&volume_src, "[Volume]\nVolumeName=microbin\n");
    let container_link = target.join("microbin.container");
    let volume_link = target.join("microbin-microbin.volume");
    symlink(&container_src, &container_link).expect("failed to link container");
    symlink(&volume_src, &volume_link).expect("failed to link volume");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["uninstall"]);
    assert!(output.status.success(), "uninstall failed: {output:?}");

    let logged = fs::read_to_string(&log).expect("missing systemctl log");
    assert!(
        logged.contains("--user stop microbin.service"),
        "uninstall must stop the container unit: {logged}"
    );
    assert!(
        logged.contains("--user stop microbin-microbin-volume.service"),
        "uninstall must stop the generated volume unit: {logged}"
    );
    assert!(
        !is_symlink(&container_link),
        "the container symlink must be removed"
    );
    assert!(
        !is_symlink(&volume_link),
        "the volume symlink must be removed"
    );
}

#[test]
fn uninstall_stops_generated_unit_for_dotted_container_name() {
    let root = TestDir::new("uninstall-dotted-container");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);

    let source = project.join("voicebox.prod.container");
    let link = target.join("voicebox.prod.container");
    write_file(&source, "[Container]\nImage=example\n");
    symlink(&source, &link).expect("failed to link container");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["uninstall"]);
    assert!(output.status.success(), "uninstall failed: {output:?}");

    let logged = fs::read_to_string(&log).expect("missing systemctl log");
    assert!(
        logged.contains("--user stop voicebox.prod.service"),
        "a dotted stem must still be stopped with its .service suffix: {logged}"
    );
    assert!(!is_symlink(&link), "the container symlink must be removed");
}

#[test]
fn uninstall_stops_generated_network_unit() {
    let root = TestDir::new("uninstall-network-unit");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);

    let container_src = project.join("stack.container");
    let network_src = project.join("stack.network");
    write_file(&container_src, "[Container]\nImage=example\n");
    write_file(&network_src, "[Network]\n");
    let container_link = target.join("stack.container");
    let network_link = target.join("stack.network");
    symlink(&container_src, &container_link).expect("failed to link container");
    symlink(&network_src, &network_link).expect("failed to link network");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["uninstall"]);
    assert!(output.status.success(), "uninstall failed: {output:?}");

    let logged = fs::read_to_string(&log).expect("missing systemctl log");
    assert!(
        logged.contains("--user stop stack.service"),
        "uninstall must stop the container unit: {logged}"
    );
    assert!(
        logged.contains("--user stop stack-network.service"),
        "uninstall must stop the generated network unit: {logged}"
    );
    assert!(
        !is_symlink(&container_link),
        "the container symlink must be removed"
    );
    assert!(
        !is_symlink(&network_link),
        "the network symlink must be removed"
    );
}

#[test]
fn uninstall_still_stops_container_unit() {
    let root = TestDir::new("uninstall-still-container");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);

    let source = project.join("voicebox.container");
    let link = target.join("voicebox.container");
    write_file(&source, "[Container]\nImage=example\n");
    symlink(&source, &link).expect("failed to link container");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["uninstall"]);
    assert!(output.status.success(), "uninstall failed: {output:?}");

    let logged = fs::read_to_string(&log).expect("missing systemctl log");
    assert!(
        logged.contains("--user stop voicebox.service"),
        "uninstall must keep stopping the container unit: {logged}"
    );
    assert!(!is_symlink(&link), "the container symlink must be removed");
}

#[test]
fn uninstall_does_not_stop_regular_files() {
    let root = TestDir::new("uninstall-regular-stop");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&target).expect("failed to create target dir");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);

    // A regular file that shadows a quadlet name: uninstall must neither stop
    // its generated unit nor remove it.
    let file = target.join("one.container");
    write_file(&file, "[Container]\nImage=sentinel\n");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &target, Path::new(&path), &["uninstall"]);
    assert!(output.status.success(), "uninstall failed: {output:?}");

    let logged = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        !logged.contains("stop one.service"),
        "a regular file must never be stopped: {logged}"
    );
    assert_eq!(
        fs::read_to_string(&file).expect("regular file must survive uninstall"),
        "[Container]\nImage=sentinel\n"
    );
}

#[test]
fn uninstall_keeps_going_when_stop_fails() {
    let root = TestDir::new("uninstall-stop-fails");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");

    let script = format!(
        r#"#!/bin/sh
echo "$@" >> {log}
if [ "$2" = "stop" ] && [ "$3" = "stack-x-volume.service" ]; then
  echo "volume stop failed" >&2
  exit 7
fi
exit 0
"#,
        log = log.display()
    );
    let systemctl = bin.join("systemctl");
    write_file(&systemctl, &script);
    fs::set_permissions(&systemctl, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fake systemctl");

    let container_src = project.join("stack.container");
    let volume_src = project.join("stack-x.volume");
    write_file(&container_src, "[Container]\nImage=example\n");
    write_file(&volume_src, "[Volume]\nVolumeName=x\n");
    let container_link = target.join("stack.container");
    let volume_link = target.join("stack-x.volume");
    symlink(&container_src, &container_link).expect("failed to link container");
    symlink(&volume_src, &volume_link).expect("failed to link volume");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["uninstall"]);
    assert!(
        output.status.success(),
        "a failed stop must not abort uninstall: {output:?}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to stop stack-x-volume"),
        "the failed stop must be warned about: {stderr}"
    );
    assert!(
        !is_symlink(&volume_link),
        "uninstall must remove the symlink even after a failed stop"
    );
    assert!(
        !is_symlink(&container_link),
        "uninstall must continue with the remaining quadlets"
    );
}

#[test]
fn compact_output_has_no_emoji() {
    let root = TestDir::new("compact-no-emoji");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_active_systemctl(&bin, &log);

    let source = project.join("voicebox.container");
    write_file(&source, "[Container]\nImage=example\n");
    symlink(&source, target.join("voicebox.container")).expect("failed to link container");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["status", "--compact"]);
    assert!(output.status.success(), "status failed: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for emoji in ["✅", "❌", "🟢", "🟡", "⚫"] {
        assert!(
            !stdout.contains(emoji),
            "compact output must be free of emoji ({emoji}): {stdout}"
        );
    }
}

#[test]
fn compact_output_is_tab_separated_plain_tokens() {
    let root = TestDir::new("compact-tokens");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_active_systemctl(&bin, &log);

    let source = project.join("voicebox.container");
    write_file(&source, "[Container]\nImage=example\n");
    symlink(&source, target.join("voicebox.container")).expect("failed to link container");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["status", "--compact"]);
    assert!(output.status.success(), "status failed: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line == "voicebox\tlinked\trunning"),
        "expected exact TSV line 'voicebox\\tlinked\\trunning': {stdout}"
    );
}

#[test]
fn compact_linked_inactive_unit_reports_stopped() {
    let root = TestDir::new("compact-stopped");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    // This fake never answers `active` on `is-active`, so the linked unit is
    // reported as stopped.
    fake_systemctl(&bin, &log);

    let source = project.join("voicebox.container");
    write_file(&source, "[Container]\nImage=example\n");
    symlink(&source, target.join("voicebox.container")).expect("failed to link container");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["status", "--compact"]);
    assert!(output.status.success(), "status failed: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line == "voicebox\tlinked\tstopped"),
        "expected exact TSV line 'voicebox\\tlinked\\tstopped': {stdout}"
    );
}

#[test]
fn compact_missing_unit_reports_dash_state() {
    let root = TestDir::new("compact-missing");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let output = run_qctl(&home, &project, &["status", "--compact"]);
    assert!(output.status.success(), "status failed: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.lines().any(|line| line == "voicebox\tmissing\t-"),
        "expected exact TSV line 'voicebox\\tmissing\\t-': {stdout}"
    );
}

#[test]
fn compact_prints_no_summary() {
    let root = TestDir::new("compact-no-summary");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&home).expect("failed to create home");
    write_file(
        &project.join("voicebox.container"),
        "[Container]\nImage=example\n",
    );

    let output = run_qctl(&home, &project, &["status", "--compact"]);
    assert!(output.status.success(), "status failed: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("Summary:"), "no summary block: {stdout}");
    assert!(!stdout.contains("total"), "no counters: {stdout}");
}

#[test]
fn human_readable_status_keeps_decoration() {
    let root = TestDir::new("status-human");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_active_systemctl(&bin, &log);

    let source = project.join("voicebox.container");
    write_file(&source, "[Container]\nImage=example\n");
    symlink(&source, target.join("voicebox.container")).expect("failed to link container");

    let path = prepend_path(&bin);
    let output = run_qctl_with_path(&home, &project, Path::new(&path), &["status"]);
    assert!(output.status.success(), "status failed: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Summary:"),
        "the human-readable status must keep its summary block: {stdout}"
    );
    assert!(
        stdout.contains("✅") && stdout.contains("🟢"),
        "the human-readable status must keep its emoji decoration: {stdout}"
    );
}

#[test]
fn compact_with_no_units_prints_nothing() {
    let root = TestDir::new("compact-empty");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    fs::create_dir_all(&project).expect("failed to create project");
    fs::create_dir_all(&target).expect("failed to create target");

    let output = run_qctl(&home, &project, &["status", "--compact"]);
    assert!(output.status.success(), "status failed: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.is_empty(),
        "compact with no units must print nothing: {stdout:?}"
    );
}

#[test]
fn status_project_lists_only_current_project() {
    let root = TestDir::new("status-project-scope");
    let layout = LocalScopeLayout::new(root.path(), "example");
    fake_systemctl(&layout.bin, &layout.log);

    let path = layout.path();
    let output = run_qctl_with_path(
        &layout.home,
        &layout.proj_a,
        Path::new(&path),
        &["status", "--project"],
    );
    assert!(
        output.status.success(),
        "status --project failed: {output:?}"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("alpha"),
        "status --project must list the current project's unit: {stdout}"
    );
    assert!(
        !stdout.contains("bravo"),
        "status --project must hide another project's unit: {stdout}"
    );
}

#[test]
fn status_without_flag_reports_whole_target_dir() {
    let root = TestDir::new("status-default-wide");
    let layout = LocalScopeLayout::new(root.path(), "example");
    fake_systemctl(&layout.bin, &layout.log);

    let path = layout.path();
    let output = run_qctl_with_path(&layout.home, &layout.proj_a, Path::new(&path), &["status"]);
    assert!(output.status.success(), "status failed: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("alpha"),
        "the default view must report the current project's unit: {stdout}"
    );
    assert!(
        stdout.contains("bravo"),
        "the default view must report the whole target directory: {stdout}"
    );
}

#[test]
fn status_project_with_explicit_service_resolves_the_name() {
    let root = TestDir::new("status-project-explicit");
    let layout = LocalScopeLayout::new(root.path(), "example");
    fake_systemctl(&layout.bin, &layout.log);

    // A name given explicitly is resolved with or without `--project`: the flag
    // only widens/narrows the argument-less scope, it does not restrict a name.
    // This test therefore documents that documented behaviour, not scope.
    let path = layout.path();
    let output = run_qctl_with_path(
        &layout.home,
        &layout.proj_a,
        Path::new(&path),
        &["status", "--project", "voicebox"],
    );
    assert!(output.status.success(), "status failed: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("voicebox"),
        "the report must show the requested unit: {stdout}"
    );
}

#[test]
fn status_project_with_no_units_exits_successfully() {
    let root = TestDir::new("status-project-empty");
    let proj_a = root.path().join("projA");
    let proj_b = root.path().join("projB");
    let home = root.path().join("home");
    let target = home.join(".config").join("containers").join("systemd");
    let bin = root.path().join("bin");
    let log = root.path().join("systemctl.log");
    fs::create_dir_all(&proj_a).expect("failed to create projA");
    fs::create_dir_all(&proj_b).expect("failed to create projB");
    fs::create_dir_all(&target).expect("failed to create target");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);

    // projA has no quadlets; projB's unit is linked in the shared target dir.
    let bravo = proj_b.join("bravo.container");
    write_file(&bravo, "[Container]\nImage=example\n");
    symlink(&bravo, target.join("bravo.container")).expect("failed to link bravo");

    let path = prepend_path(&bin);
    let scoped = run_qctl_with_path(&home, &proj_a, Path::new(&path), &["status", "--project"]);
    assert!(
        scoped.status.success(),
        "status --project with no units must succeed: {scoped:?}"
    );
    let scoped_stdout = String::from_utf8_lossy(&scoped.stdout);
    assert!(
        !scoped_stdout.contains("bravo"),
        "status --project must not leak another project's unit: {scoped_stdout}"
    );

    // Contrast guard: without the flag the same command still reports projB.
    let wide = run_qctl_with_path(&home, &proj_a, Path::new(&path), &["status"]);
    assert!(wide.status.success(), "status failed: {wide:?}");
    let wide_stdout = String::from_utf8_lossy(&wide.stdout);
    assert!(
        wide_stdout.contains("bravo"),
        "the default wide view must still report projB: {wide_stdout}"
    );
}

// ---------------------------------------------------------------------------
// consolidate-stack-management: global collision guard on `install` and the
// stacks-root audit of `check` (openspec/changes/consolidate-stack-management).
//
// A "stack" is a subdirectory of the stacks root; the default root is
// `$HOME/.config/containers/available`, overridden here with `--stacks-root`
// so every test stays hermetic and never touches the real HOME.
// ---------------------------------------------------------------------------

/// Materialises a stacks root under `root/available`, creating one directory
/// per stack and one file per `(name, content)` pair. Returns the stacks root
/// so tests can point `--stacks-root` at it and use one of its stacks as the
/// working directory.
fn stacks_root_with(root: &Path, stacks: &[(&str, &[(&str, &str)])]) -> PathBuf {
    let stacks_root = root.join("available");
    for (stack, files) in stacks {
        let stack_dir = stacks_root.join(stack);
        fs::create_dir_all(&stack_dir).expect("failed to create stack dir");
        for (name, content) in *files {
            write_file(&stack_dir.join(name), content);
        }
    }
    stacks_root
}

/// Prepares the fake-`systemctl` scaffolding shared by the `install` tests and
/// returns `(home, bin_path, systemctl_log)`.
fn install_sandbox(root: &Path) -> (PathBuf, String, PathBuf) {
    let home = root.join("home");
    let bin = root.join("bin");
    let log = root.join("systemctl.log");
    fs::create_dir_all(&home).expect("failed to create home");
    fs::create_dir_all(&bin).expect("failed to create bin");
    fake_systemctl(&bin, &log);
    (home, prepend_path(&bin), log)
}

#[test]
fn install_aborts_on_basename_collision() {
    let root = TestDir::new("install-basename-collision");
    let (home, path, _log) = install_sandbox(root.path());
    let stacks = stacks_root_with(
        root.path(),
        &[
            ("alpha", &[("x.container", "[Container]\nImage=alpha\n")]),
            ("bravo", &[("x.container", "[Container]\nImage=bravo\n")]),
        ],
    );
    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");
    let cwd = stacks.join("bravo");

    let output = run_qctl_with_path(
        &home,
        &cwd,
        Path::new(&path),
        &["install", "--stacks-root", stacks_arg],
    );

    assert!(
        !output.status.success(),
        "install must abort when two stacks declare the same unit basename: {output:?}"
    );
    let target = home.join(".config").join("containers").join("systemd");
    assert!(
        symlinks_under(&target).is_empty(),
        "a collision must abort atomically, leaving no symlink behind: {:?}",
        symlinks_under(&target)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("x.container"),
        "install must report the colliding basename: {stderr}"
    );
}

#[test]
fn install_aborts_on_containername_collision() {
    let root = TestDir::new("install-containername-collision");
    let (home, path, _log) = install_sandbox(root.path());
    let stacks = stacks_root_with(
        root.path(),
        &[
            (
                "alpha",
                &[(
                    "a.container",
                    "[Container]\nImage=alpha\nContainerName=foo\n",
                )],
            ),
            (
                "bravo",
                &[(
                    "b.container",
                    "[Container]\nImage=bravo\nContainerName=foo\n",
                )],
            ),
        ],
    );
    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");
    let cwd = stacks.join("bravo");

    let output = run_qctl_with_path(
        &home,
        &cwd,
        Path::new(&path),
        &["install", "--stacks-root", stacks_arg],
    );

    assert!(
        !output.status.success(),
        "install must abort when two quadlets share a ContainerName: {output:?}"
    );
    let target = home.join(".config").join("containers").join("systemd");
    assert!(
        symlinks_under(&target).is_empty(),
        "a ContainerName collision must abort atomically, leaving no symlink behind: {:?}",
        symlinks_under(&target)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("foo"),
        "install must report the colliding ContainerName: {stderr}"
    );
}

#[test]
fn install_without_collision_links_normally() {
    let root = TestDir::new("install-no-collision");
    let (home, path, log) = install_sandbox(root.path());
    let stacks = stacks_root_with(
        root.path(),
        &[
            ("alpha", &[("a.container", "[Container]\nImage=alpha\n")]),
            ("bravo", &[("b.container", "[Container]\nImage=bravo\n")]),
        ],
    );
    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");
    let cwd = stacks.join("bravo");

    let output = run_qctl_with_path(
        &home,
        &cwd,
        Path::new(&path),
        &["install", "--stacks-root", stacks_arg],
    );
    assert!(
        output.status.success(),
        "install must succeed when nothing collides: {output:?}"
    );

    let target = home.join(".config").join("containers").join("systemd");
    let link = target.join("b.container");
    assert_eq!(
        fs::read_link(&link).expect("install must link the current stack's quadlet"),
        cwd.join("b.container")
    );
    assert!(
        !target.join("a.container").exists(),
        "install must not link another stack's quadlet"
    );
    assert!(
        fs::read_to_string(&log)
            .expect("missing systemctl log")
            .contains("--user daemon-reload"),
        "install must reload systemd as before"
    );
}

#[test]
fn check_no_args_clean_stacks_root_exits_zero() {
    let root = TestDir::new("check-clean");
    let home = root.path().join("home");
    fs::create_dir_all(&home).expect("failed to create home");
    let stacks = stacks_root_with(
        root.path(),
        &[
            (
                "alpha",
                &[("alpha.container", "[Container]\nImage=alpha\n")],
            ),
            (
                "bravo",
                &[("bravo.container", "[Container]\nImage=bravo\n")],
            ),
        ],
    );
    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");

    let output = run_qctl(&home, root.path(), &["check", "--stacks-root", stacks_arg]);

    assert!(
        output.status.success(),
        "check must exit 0 on a clean stacks root: {output:?}"
    );
}

#[test]
fn check_no_args_hard_collision_exits_nonzero() {
    let root = TestDir::new("check-collision");
    let home = root.path().join("home");
    fs::create_dir_all(&home).expect("failed to create home");
    let stacks = stacks_root_with(
        root.path(),
        &[
            ("alpha", &[("x.container", "[Container]\nImage=alpha\n")]),
            ("bravo", &[("x.container", "[Container]\nImage=bravo\n")]),
        ],
    );
    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");

    let output = run_qctl(&home, root.path(), &["check", "--stacks-root", stacks_arg]);

    assert!(
        !output.status.success(),
        "check must exit non-zero on a hard basename collision: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("x.container"),
        "check must report the colliding basename: {stderr}"
    );
}

#[test]
fn check_no_args_naming_warning_does_not_fail() {
    let root = TestDir::new("check-naming-warning");
    let home = root.path().join("home");
    fs::create_dir_all(&home).expect("failed to create home");
    // `weird.container` lives in stack `alpha`, so it is neither
    // `<stack>.<ext>` nor `<stack>-<role>`: a naming warning, not a collision.
    let stacks = stacks_root_with(
        root.path(),
        &[(
            "alpha",
            &[("weird.container", "[Container]\nImage=weird\n")],
        )],
    );
    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");

    let output = run_qctl(&home, root.path(), &["check", "--stacks-root", stacks_arg]);

    assert!(
        output.status.success(),
        "a naming-convention warning must not fail the audit: {output:?}"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("weird"),
        "the audit must warn about the off-convention unit: {combined}"
    );
}

#[test]
fn check_with_arg_still_validates_single_quadlet() {
    let root = TestDir::new("check-single");
    let home = root.path().join("home");
    fs::create_dir_all(&home).expect("failed to create home");
    let stacks = stacks_root_with(
        root.path(),
        &[(
            "alpha",
            &[("alpha.container", "[Container]\nImage=example\n")],
        )],
    );
    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");
    let quadlet = stacks.join("alpha").join("alpha.container");
    let quadlet_arg = quadlet.to_str().expect("quadlet path must be utf8");
    // Hermetic: a stub generator instead of the host's `/usr/lib/podman/quadlet`,
    // injected through `QCTL_QUADLET_BIN`.
    let quadlet_bin = fake_quadlet(&root.path().join("bin"), 0);

    let output = run_qctl_with_quadlet_bin(
        &home,
        root.path(),
        &quadlet_bin,
        &["check", quadlet_arg, "--stacks-root", stacks_arg],
    );

    assert!(
        output.status.success(),
        "check <QUADLET> must still validate a single file: {output:?}"
    );
}

/// Writes an executable stub named `quadlet` that exits with `code`, so `check`
/// tests can point `QCTL_QUADLET_BIN` at it instead of the host's real
/// `/usr/lib/podman/quadlet` and stay hermetic.
fn fake_quadlet(bin: &Path, code: i32) -> PathBuf {
    fs::create_dir_all(bin).expect("failed to create bin dir");
    let path = bin.join("quadlet");
    write_file(&path, &format!("#!/bin/sh\nexit {code}\n"));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fake quadlet");
    path
}

/// Runs qctl with `QCTL_QUADLET_BIN` pointing at a stub generator.
fn run_qctl_with_quadlet_bin(home: &Path, cwd: &Path, quadlet_bin: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qctl"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USER", "qctl-test")
        .env("QCTL_QUADLET_BIN", quadlet_bin)
        .env_remove("RUST_LOG")
        .output()
        .expect("failed to run qctl")
}

#[test]
fn check_with_arg_propagates_generator_failure() {
    let root = TestDir::new("check-single-fail");
    let home = root.path().join("home");
    fs::create_dir_all(&home).expect("failed to create home");
    let stacks = stacks_root_with(
        root.path(),
        &[(
            "alpha",
            &[("alpha.container", "[Container]\nImage=example\n")],
        )],
    );
    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");
    let quadlet = stacks.join("alpha").join("alpha.container");
    let quadlet_arg = quadlet.to_str().expect("quadlet path must be utf8");
    let quadlet_bin = fake_quadlet(&root.path().join("bin"), 1);

    let output = run_qctl_with_quadlet_bin(
        &home,
        root.path(),
        &quadlet_bin,
        &["check", quadlet_arg, "--stacks-root", stacks_arg],
    );

    assert!(
        !output.status.success(),
        "a failing generator must fail `check <QUADLET>`: {output:?}"
    );
}

#[test]
fn check_no_args_containername_collision_within_same_stack_exits_nonzero() {
    let root = TestDir::new("check-containername-same-stack");
    let home = root.path().join("home");
    fs::create_dir_all(&home).expect("failed to create home");
    // Both quadlets live in the SAME stack but declare the same ContainerName,
    // which Podman requires to be globally unique: a hard collision.
    let stacks = stacks_root_with(
        root.path(),
        &[(
            "alpha",
            &[
                (
                    "a.container",
                    "[Container]\nImage=alpha\nContainerName=foo\n",
                ),
                (
                    "b.container",
                    "[Container]\nImage=beta\nContainerName=foo\n",
                ),
            ],
        )],
    );
    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");

    let output = run_qctl(&home, root.path(), &["check", "--stacks-root", stacks_arg]);

    assert!(
        !output.status.success(),
        "two quadlets in the same stack sharing a ContainerName must fail the audit: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("foo"),
        "check must report the colliding ContainerName: {stderr}"
    );
}

#[test]
fn install_aborts_on_containername_collision_within_same_stack() {
    let root = TestDir::new("install-containername-same-stack");
    let (home, path, _log) = install_sandbox(root.path());
    let stacks = stacks_root_with(
        root.path(),
        &[(
            "alpha",
            &[
                (
                    "a.container",
                    "[Container]\nImage=alpha\nContainerName=foo\n",
                ),
                (
                    "b.container",
                    "[Container]\nImage=beta\nContainerName=foo\n",
                ),
            ],
        )],
    );
    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");
    let cwd = stacks.join("alpha");

    let output = run_qctl_with_path(
        &home,
        &cwd,
        Path::new(&path),
        &["install", "--stacks-root", stacks_arg],
    );

    assert!(
        !output.status.success(),
        "install must abort when two quadlets in the same stack share a ContainerName: {output:?}"
    );
    let target = home.join(".config").join("containers").join("systemd");
    assert!(
        symlinks_under(&target).is_empty(),
        "a ContainerName collision must abort atomically, leaving no symlink behind: {:?}",
        symlinks_under(&target)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("foo"),
        "install must report the colliding ContainerName: {stderr}"
    );
}

#[test]
fn check_warns_when_quadlet_is_unreadable() {
    let root = TestDir::new("check-unreadable-quadlet");
    let home = root.path().join("home");
    fs::create_dir_all(&home).expect("failed to create home");
    // Convention-compliant basenames, so the only possible warning is the one
    // about the unreadable file: a naming warning must not mask the failure.
    let stacks = stacks_root_with(
        root.path(),
        &[
            (
                "alpha",
                &[(
                    "alpha.container",
                    "[Container]\nImage=alpha\nContainerName=foo\n",
                )],
            ),
            (
                "bravo",
                &[(
                    "bravo.container",
                    "[Container]\nImage=bravo\nContainerName=foo\n",
                )],
            ),
        ],
    );
    let unreadable = stacks.join("bravo").join("bravo.container");
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))
        .expect("failed to chmod unreadable quadlet");

    // Running as root defeats the permission bit, so the premise does not hold:
    // skip rather than assert something false.
    if fs::read_to_string(&unreadable).is_ok() {
        eprintln!("skipping: this user can read a 0o000 file (running as root?)");
        return;
    }

    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");
    let output = run_qctl(&home, root.path(), &["check", "--stacks-root", stacks_arg]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("bravo.container"),
        "an unreadable quadlet must be surfaced, not swallowed silently: {stderr}"
    );
}

#[test]
fn check_warns_when_stacks_root_is_missing() {
    let root = TestDir::new("check-missing-root");
    let home = root.path().join("home");
    fs::create_dir_all(&home).expect("failed to create home");
    let missing = root.path().join("does-not-exist");
    let missing_arg = missing.to_str().expect("path must be utf8");

    let output = run_qctl(&home, root.path(), &["check", "--stacks-root", missing_arg]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does-not-exist"),
        "check must warn when the stacks root is unreadable instead of reporting a clean audit: {stderr}"
    );
}

#[test]
fn install_aborts_on_collision_from_nested_quadlets_dir() {
    let root = TestDir::new("install-nested-quadlets");
    let (home, path, _log) = install_sandbox(root.path());
    let stacks = stacks_root_with(
        root.path(),
        &[("alpha", &[("x.container", "[Container]\nImage=alpha\n")])],
    );
    // `bravo` keeps its quadlets in the supported `quadlets/` subdirectory.
    let bravo_quadlets = stacks.join("bravo").join("quadlets");
    fs::create_dir_all(&bravo_quadlets).expect("failed to create nested quadlets dir");
    write_file(
        &bravo_quadlets.join("x.container"),
        "[Container]\nImage=bravo\n",
    );
    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");

    let output = run_qctl_with_path(
        &home,
        &bravo_quadlets,
        Path::new(&path),
        &["install", "--stacks-root", stacks_arg],
    );

    assert!(
        !output.status.success(),
        "install from a nested quadlets/ dir must still enforce the global guard: {output:?}"
    );
    let target = home.join(".config").join("containers").join("systemd");
    assert!(
        symlinks_under(&target).is_empty(),
        "a collision must abort atomically, leaving no symlink behind: {:?}",
        symlinks_under(&target)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("x.container"),
        "install must report the colliding basename: {stderr}"
    );
}

#[test]
fn install_guard_does_not_apply_outside_stacks_root() {
    let root = TestDir::new("install-outside-root");
    let (home, path, log) = install_sandbox(root.path());
    // A hard collision exists inside the stacks root, but this install runs from
    // a directory outside it, so the global guard must not apply.
    let stacks = stacks_root_with(
        root.path(),
        &[
            ("alpha", &[("x.container", "[Container]\nImage=alpha\n")]),
            ("bravo", &[("x.container", "[Container]\nImage=bravo\n")]),
        ],
    );
    let work = root.path().join("work");
    fs::create_dir_all(&work).expect("failed to create work dir");
    write_file(&work.join("w.container"), "[Container]\nImage=work\n");
    let stacks_arg = stacks.to_str().expect("stacks root must be utf8");

    let output = run_qctl_with_path(
        &home,
        &work,
        Path::new(&path),
        &["install", "--stacks-root", stacks_arg],
    );

    assert!(
        output.status.success(),
        "the guard must not abort installs from outside the stacks root: {output:?}"
    );
    let target = home.join(".config").join("containers").join("systemd");
    assert!(
        target.join("w.container").exists(),
        "install must link the current project's quadlet"
    );
    assert!(
        fs::read_to_string(&log)
            .expect("missing systemctl log")
            .contains("--user daemon-reload"),
        "install must reload systemd as before"
    );
}

use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

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
        .output()
        .expect("failed to run qctl")
}

fn run_qctl_with_stdin(home: &Path, cwd: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_qctl"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USER", "qctl-test")
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

fn run_qctl_with_path(home: &Path, cwd: &Path, path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qctl"))
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("USER", "qctl-test")
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

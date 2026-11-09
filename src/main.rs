use std::collections::HashMap;
use std::future::Future;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};
use tokio::fs;
use tokio::process::Command;
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use tracing_subscriber::EnvFilter;

const QUADLET_EXTENSIONS: &[&str] = &[
    "container",
    "volume",
    "network",
    "pod",
    "kube",
    "image",
    "build",
    "artifact",
    "service",
    "socket",
    "mount",
    "timer",
    "path",
];

#[derive(Debug, Parser)]
#[command(name = "qctl", version)]
#[command(about = "Manage quadlets", long_about = None)]
struct Cli {
    /// Report planned actions for mutating commands without changing files or
    /// running mutating actions; the final status table still queries
    /// `systemctl --user is-active`. Read-only commands (status, logs, logsf,
    /// check) still query the system.
    #[arg(long, global = true)]
    dry_run: bool,
    /// Timeout in seconds for external commands; 0 disables the limit
    #[arg(long, global = true, value_name = "SECONDS", default_value_t = 60)]
    timeout: u64,
    /// Root directory holding one directory per stack
    /// (defaults to `$HOME/.config/containers/available`)
    #[arg(long, global = true, value_name = "DIR")]
    stacks_root: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create symlinks and reload user systemd
    Install,
    /// Stop services, remove symlinks and reload user systemd
    Uninstall,
    /// Uninstall and then install again (stops services, recreates symlinks)
    Reinstall,
    /// Start all container units or one specific service
    Start {
        /// Service name, with or without .container extension
        service: Option<String>,
    },
    /// Stop all container units or one specific service
    Stop {
        /// Service name, with or without .container extension
        service: Option<String>,
    },
    /// Restart all container units or one specific service
    Restart {
        /// Service name, with or without .container extension
        service: Option<String>,
    },
    /// Pull latest container images
    Update {
        /// Service name, with or without .container extension
        service: Option<String>,
    },
    /// Show status for all container units or one specific service
    Status {
        /// Service name, with or without .container extension
        service: Option<String>,
        /// Script-friendly output: one undecorated TSV line per unit as
        /// `<unit>\t<linked|missing>\t<running|stopped|->`, with no emoji,
        /// colour or `Summary:` block
        #[arg(long)]
        compact: bool,
        /// Restrict the report to the current project's quadlets instead of
        /// every unit found in the target directory
        #[arg(long)]
        project: bool,
    },
    /// Interactive table to start, stop, restart or inspect services
    Menu {
        /// Optional service filter, with or without .container extension
        service: Option<String>,
    },
    /// Remove podman volumes declared in .volume files
    CleanVolumes {
        /// Confirm removal without prompting
        #[arg(short, long)]
        yes: bool,
    },
    /// Uninstall the current stack and, with explicit opt-in, remove its
    /// volumes, secrets, images and host-side bind-mounted data
    Purge {
        /// Also remove the stack's named volumes (already part of the base)
        #[arg(long)]
        volumes: bool,
        /// Remove the stack's declared secrets (`podman secret rm`)
        #[arg(long)]
        secrets: bool,
        /// Remove the stack's declared images (`podman rmi`)
        #[arg(long)]
        images: bool,
        /// Remove eligible host-side bind-mount directories under $HOME
        #[arg(long)]
        data: bool,
        /// Remove resources even when another stack still declares them
        #[arg(long)]
        force: bool,
        /// Confirm removal without prompting
        #[arg(short, long)]
        yes: bool,
    },
    /// Validate one quadlet file, or audit the whole stacks root when called
    /// without an argument
    Check {
        /// Quadlet file path; omit to audit every stack under the stacks root
        quadlet: Option<String>,
    },
    /// Show logs in follow mode for a service since last hour
    Logs {
        /// Systemd user unit name
        service: String,
    },
    /// Show logs in follow mode with extra metadata
    Logsf {
        /// Systemd user unit name
        service: String,
    },
    /// Reload systemd user daemon
    Reload,
    /// Generate shell completions
    #[command(hide = true)]
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },
}

struct AppContext {
    source_dirs: Vec<PathBuf>,
    target_dir: PathBuf,
    /// Directory the process was started from, used to locate the current stack
    /// inside the stacks root.
    current_dir: PathBuf,
    /// Root directory holding one directory per stack.
    stacks_root: PathBuf,
    user: String,
    dry_run: bool,
    /// Deadline applied to bounded external commands; `None` waits forever.
    timeout: Option<Duration>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .without_time()
        // Diagnostics and warnings belong on stderr so that redirecting stdout
        // keeps only the data (tables, DRY-RUN plans, summaries) a script parses.
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    if let Commands::Completions { shell } = cli.command {
        generate(shell, &mut Cli::command(), "qctl", &mut std::io::stdout());
        return Ok(());
    }

    let ctx = AppContext::new(cli.dry_run, cli.timeout, cli.stacks_root)?;

    match cli.command {
        Commands::Install => {
            install(&ctx).await?;
            status(&ctx, None, false, TargetScope::Project).await?;
        }
        Commands::Uninstall => {
            uninstall(&ctx).await?;
            status(&ctx, None, false, TargetScope::Project).await?;
        }
        Commands::Reinstall => {
            reinstall(&ctx).await?;
            status(&ctx, None, false, TargetScope::Project).await?;
        }
        Commands::Start { service } => {
            start(&ctx, service.clone()).await?;
            status(&ctx, service, false, TargetScope::Project).await?;
        }
        Commands::Stop { service } => {
            stop(&ctx, service.clone()).await?;
            status(&ctx, service, false, TargetScope::Project).await?;
        }
        Commands::Restart { service } => {
            restart(&ctx, service.clone()).await?;
            status(&ctx, service, false, TargetScope::Project).await?;
        }
        Commands::Update { service } => update(&ctx, service).await?,
        Commands::Status {
            service,
            compact,
            project,
        } => {
            let scope = if project {
                TargetScope::Project
            } else {
                TargetScope::Wide
            };
            status(&ctx, service, compact, scope).await?
        }
        Commands::Menu { service } => menu(&ctx, service).await?,
        Commands::CleanVolumes { yes } => clean_volumes(&ctx, yes).await?,
        Commands::Purge {
            volumes,
            secrets,
            images,
            data,
            force,
            yes,
        } => purge(&ctx, volumes, secrets, images, data, force, yes).await?,
        Commands::Check { quadlet } => match quadlet {
            Some(quadlet) => check_quadlet(&ctx, quadlet).await?,
            None => audit_stacks_root(&ctx).await?,
        },
        Commands::Logs { service } => logs(service, false).await?,
        Commands::Logsf { service } => logs(service, true).await?,
        Commands::Reload => daemon_reload(&ctx).await?,
        Commands::Completions { .. } => unreachable!(),
    }

    Ok(())
}

impl AppContext {
    fn new(dry_run: bool, timeout_secs: u64, stacks_root: Option<PathBuf>) -> Result<Self> {
        let root_dir = std::env::current_dir().context("failed to resolve current directory")?;
        let source_dirs = discover_source_dirs(&root_dir);

        let home = std::env::var("HOME").context("HOME is not set")?;
        let target_dir = Path::new(&home).join(".config/containers/systemd");
        let stacks_root =
            stacks_root.unwrap_or_else(|| Path::new(&home).join(".config/containers/available"));

        let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());

        // `0` is the documented escape hatch that turns the deadline off: a pull
        // or a followed log stream may legitimately outlive any fixed bound.
        let timeout = if timeout_secs == 0 {
            None
        } else {
            Some(Duration::from_secs(timeout_secs))
        };

        Ok(Self {
            source_dirs,
            target_dir,
            current_dir: root_dir,
            stacks_root,
            user,
            dry_run,
            timeout,
        })
    }

    fn source_display(&self) -> String {
        self.source_dirs
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

async fn install(ctx: &AppContext) -> Result<()> {
    debug!(
        "Installing quadlets from {} into {}",
        ctx.source_display(),
        ctx.target_dir.display()
    );

    // Refuse the whole install, before touching the filesystem, when the current
    // stack's units collide with another stack under the stacks root.
    ensure_no_global_collision(ctx).await?;

    if ctx.dry_run {
        println!("DRY-RUN create {}", ctx.target_dir.display());
    } else {
        fs::create_dir_all(&ctx.target_dir)
            .await
            .with_context(|| format!("failed to create {}", ctx.target_dir.display()))?;
    }

    let target_canon = canonical_target_dir(ctx).await;
    let files = collect_quadlets(&ctx.source_dirs).await?;
    for src in files {
        let name = basename(&src)?;
        let dest = ctx.target_dir.join(&name);

        if source_is_in_target_dir(&src, target_canon.as_deref()).await {
            println!("Skipping {name} (already in target dir)");
            continue;
        }

        link_quadlet(ctx, &src, &dest).await?;
    }

    daemon_reload(ctx).await?;
    debug!("Install complete");
    Ok(())
}

/// Canonical form of the target directory, when it exists.
///
/// `None` means the directory is absent: nothing can live inside it yet, so every
/// source counts as external and is linked normally. Callers that need to know
/// whether a source sits *inside* the target tree pass the result to
/// `source_is_in_target_dir`.
async fn canonical_target_dir(ctx: &AppContext) -> Option<PathBuf> {
    fs::canonicalize(&ctx.target_dir).await.ok()
}

/// Returns true when `src` already lives inside the target directory, comparing
/// canonical paths rather than string prefixes so symlinked directories and
/// `..` segments do not fool the check.
///
/// The test is ancestry, not direct-parent equality: a quadlet kept in
/// `target/quadlets/x.container` is *also* already inside the target directory,
/// and linking it back into `target/` would create a self-referential entry
/// that the next run reports as a duplicate.
///
/// A failed `canonicalize` (for example a path that vanished mid-scan) yields
/// `false`: the source is then treated as external and linked normally, which is
/// the safe default and needs no error path at the call site.
async fn source_is_in_target_dir(src: &Path, target_canon: Option<&Path>) -> bool {
    let Some(target_canon) = target_canon else {
        return false;
    };

    let parent = src.parent().unwrap_or_else(|| Path::new("."));
    let Ok(parent_canon) = fs::canonicalize(parent).await else {
        return false;
    };
    parent_canon.starts_with(target_canon)
}

/// True when `<unit>.container` counts as installed for the target directory:
/// either an entry directly inside it — the symlink `install` creates, or a
/// regular file — or a quadlet living one level below the target directory, for
/// example `<target_dir>/quadlets/<unit>.container`.
///
/// `install` deliberately leaves quadlets that already live inside the target
/// tree unlinked, so commands that act on units must accept them by name instead
/// of looking only at `<target_dir>/<unit>.container`; otherwise they would
/// either recreate the redundant self-symlink or silently skip the unit.
async fn unit_installed_in_target_dir(ctx: &AppContext, unit: &str) -> Result<bool> {
    let in_target = ctx.target_dir.join(format!("{unit}.container"));
    if path_exists(&in_target).await? {
        return Ok(true);
    }

    let Some(src) = find_source_quadlet(ctx, &format!("{unit}.container")).await? else {
        return Ok(false);
    };
    Ok(source_is_in_target_dir(&src, canonical_target_dir(ctx).await.as_deref()).await)
}

/// Stops container units and removes their symlinks from the target directory.
/// Only symlinks are removed: a regular file that merely shares a quadlet name
/// is warned about on stderr and left untouched (its stop is skipped too),
/// because such a file does not belong to `qctl`.
async fn uninstall(ctx: &AppContext) -> Result<()> {
    debug!(
        "Stopping services and removing symlinks from {}",
        ctx.target_dir.display()
    );

    let files = collect_quadlets(&ctx.source_dirs).await?;
    for src in files {
        let name = basename(&src)?;
        let stem = strip_dot_extension(&name);
        let ext = extension_of(&name);

        let target = ctx.target_dir.join(&name);
        // Inspect the entry itself (never the file it points to): a regular file
        // that shadows a quadlet name is not ours to stop or remove.
        let Ok(meta) = fs::symlink_metadata(&target).await else {
            continue;
        };
        if !meta.file_type().is_symlink() {
            warn!(
                "not removing {} (not a symlink); leaving it untouched",
                target.display()
            );
            continue;
        }

        let unit = generated_unit(&stem, ext);
        if ctx.dry_run {
            println!("DRY-RUN systemctl --user stop {unit}");
        } else {
            match systemctl_user("stop", &unit, ctx.timeout).await {
                Ok(()) => debug!("Stopped {unit} (if running)"),
                Err(err) => warn!("failed to stop {unit} (continuing): {err:#}"),
            }
        }

        if ctx.dry_run {
            println!("DRY-RUN remove {}", target.display());
        } else {
            fs::remove_file(&target)
                .await
                .with_context(|| format!("failed to remove {}", target.display()))?;
        }
        debug!("Removed {}", target.display());
    }

    daemon_reload(ctx).await?;
    debug!("Uninstall complete");
    Ok(())
}

async fn reinstall(ctx: &AppContext) -> Result<()> {
    debug!("Reinstalling quadlets");
    uninstall(ctx).await?;
    install(ctx).await?;
    debug!("Reinstall complete");
    Ok(())
}

async fn start(ctx: &AppContext, service: Option<String>) -> Result<()> {
    ensure_service_known(ctx, &service).await?;
    let requested = service.is_some();
    let targets = resolve_targets(ctx, service, TargetScope::Project).await?;

    if requested {
        // `start <service>` installs only the requested unit, never the whole
        // project: link each target whose symlink is missing, one by one.
        for unit in &targets {
            let unit_file = ctx.target_dir.join(format!("{unit}.container"));
            if !path_exists(&unit_file).await? {
                install_single(ctx, unit).await?;
            }
        }
    } else if ctx.dry_run {
        install_missing_targets(ctx, &targets).await?;
    } else {
        ensure_targets_installed(ctx, &targets).await?;
    }

    let mut handles = Vec::new();
    for unit in targets {
        if ctx.dry_run {
            println!("DRY-RUN systemctl --user start {}", container_unit(&unit));
            continue;
        }

        if !unit_installed_in_target_dir(ctx, &unit).await? {
            debug!("Skipping {unit} (not linked in target dir)");
            continue;
        }

        println!("Starting {unit}");
        let unit_for_task = container_unit(&unit);
        let timeout = ctx.timeout;
        handles.push((
            unit,
            tokio::spawn(async move {
                systemctl_user("start", &unit_for_task, timeout).await?;
                debug!("Started {unit_for_task}");
                Ok::<(), anyhow::Error>(())
            }),
        ));
    }

    await_unit_tasks("start", handles).await
}

/// Links only `<unit>.container` from the project into the target directory,
/// creating that one symlink and reloading systemd, without touching unrelated
/// quadlets. Used by `start <service>` so a first run installs exactly what it
/// starts.
async fn install_single(ctx: &AppContext, unit: &str) -> Result<()> {
    let file_name = format!("{unit}.container");
    let Some(src) = find_source_quadlet(ctx, &file_name).await? else {
        bail!("no such quadlet: {unit}");
    };

    // A quadlet that already lives inside the target tree is installed by
    // definition: `install` does not link it, and `start` must not create the
    // redundant self-symlink #3 removes either. Check the origin, not the
    // destination: an existing `<target_dir>/<unit>.container` never reaches
    // this function, its caller skips it first.
    if source_is_in_target_dir(&src, canonical_target_dir(ctx).await.as_deref()).await {
        debug!("{unit} already lives in the target dir; not linking it");
        return Ok(());
    }

    let dest = ctx.target_dir.join(&file_name);

    if ctx.dry_run {
        return link_quadlet(ctx, &src, &dest).await;
    }

    fs::create_dir_all(&ctx.target_dir)
        .await
        .with_context(|| format!("failed to create {}", ctx.target_dir.display()))?;

    link_quadlet(ctx, &src, &dest).await?;
    daemon_reload(ctx).await
}

/// Plans or performs the `src -> dest` symlink in one place: under `--dry-run`
/// it prints the planned link and creates nothing, otherwise it calls
/// `link_or_replace` and logs the link once created. A pre-existing regular file
/// is left in place (already warned about by `link_or_replace`), so `install`
/// and `install_single` share the same stderr contract.
async fn link_quadlet(ctx: &AppContext, src: &Path, dest: &Path) -> Result<()> {
    if ctx.dry_run {
        println!("DRY-RUN link {} -> {}", src.display(), dest.display());
        return Ok(());
    }

    if link_or_replace(src, dest).await? {
        debug!("Linked {} -> {}", src.display(), dest.display());
    }
    Ok(())
}

/// Returns the first `<file_name>` found in a source directory, if any.
async fn find_source_quadlet(ctx: &AppContext, file_name: &str) -> Result<Option<PathBuf>> {
    for dir in &ctx.source_dirs {
        let candidate = dir.join(file_name);
        if path_exists(&candidate).await? {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

async fn ensure_targets_installed(ctx: &AppContext, targets: &[String]) -> Result<()> {
    if missing_targets(ctx, targets).await?.is_empty() {
        return Ok(());
    }

    debug!("Missing links. Installing quadlets first.");
    install(ctx).await
}

async fn install_missing_targets(ctx: &AppContext, targets: &[String]) -> Result<()> {
    let missing_targets = missing_targets(ctx, targets).await?;
    if missing_targets.is_empty() {
        return Ok(());
    }

    println!(
        "DRY-RUN install missing target links for {}",
        missing_targets.join(", ")
    );
    install(ctx).await
}

async fn missing_targets(ctx: &AppContext, targets: &[String]) -> Result<Vec<String>> {
    let mut missing_targets = Vec::new();

    for unit in targets {
        let unit_file = ctx.target_dir.join(format!("{unit}.container"));
        if !path_exists(&unit_file).await? {
            missing_targets.push(unit.clone());
        }
    }

    Ok(missing_targets)
}

async fn stop(ctx: &AppContext, service: Option<String>) -> Result<()> {
    ensure_service_known(ctx, &service).await?;
    let targets = resolve_targets(ctx, service, TargetScope::Project).await?;
    let mut handles = Vec::new();
    for unit in targets {
        if ctx.dry_run {
            println!("DRY-RUN systemctl --user stop {}", container_unit(&unit));
            continue;
        }

        let unit_file = ctx.target_dir.join(format!("{unit}.container"));
        if !path_exists(&unit_file).await? {
            debug!("Skipping {unit} (not linked in target dir)");
            continue;
        }

        let unit_for_task = container_unit(&unit);
        let timeout = ctx.timeout;
        handles.push((
            unit,
            tokio::spawn(async move {
                systemctl_user("stop", &unit_for_task, timeout).await?;
                debug!("Stopped {unit_for_task}");
                Ok::<(), anyhow::Error>(())
            }),
        ));
    }

    await_unit_tasks("stop", handles).await
}

async fn restart(ctx: &AppContext, service: Option<String>) -> Result<()> {
    ensure_service_known(ctx, &service).await?;
    let targets = resolve_targets(ctx, service, TargetScope::Project).await?;
    let mut handles = Vec::new();
    for unit in targets {
        if ctx.dry_run {
            println!("DRY-RUN systemctl --user restart {}", container_unit(&unit));
            continue;
        }

        let unit_file = ctx.target_dir.join(format!("{unit}.container"));
        if !path_exists(&unit_file).await? {
            debug!("Skipping {unit} (not linked in target dir)");
            continue;
        }

        let unit_for_task = container_unit(&unit);
        let timeout = ctx.timeout;
        handles.push((
            unit,
            tokio::spawn(async move {
                systemctl_user("restart", &unit_for_task, timeout).await?;
                debug!("Restarted {unit_for_task}");
                Ok::<(), anyhow::Error>(())
            }),
        ));
    }

    await_unit_tasks("restart", handles).await
}

/// Waits for every spawned unit task to finish, collecting all failures instead
/// of aborting at the first one, and reports an aggregated error that names each
/// failing unit.
async fn await_unit_tasks(
    action: &str,
    handles: Vec<(String, JoinHandle<Result<()>>)>,
) -> Result<()> {
    let mut failures = Vec::new();

    for (unit, handle) in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => failures.push(format!("  {unit}: {err:#}")),
            Err(join_err) => failures.push(format!("  {unit}: task failed: {join_err}")),
        }
    }

    if failures.is_empty() {
        return Ok(());
    }

    bail!(
        "systemctl {action} failed for {} unit(s):\n{}",
        failures.len(),
        failures.join("\n")
    )
}

async fn update(ctx: &AppContext, service: Option<String>) -> Result<()> {
    ensure_service_known(ctx, &service).await?;
    let files = collect_quadlets(&ctx.source_dirs).await?;
    let containers: Vec<_> = files
        .into_iter()
        .filter(|path| extension_of_path(path) == Some("container"))
        .collect();

    let mut pulled = 0;
    let mut failed = 0;
    let mut dry_run_pulls = 0;
    let mut failures = Vec::new();

    for path in &containers {
        let unit = strip_dot_extension(&basename(path)?);

        if let Some(ref svc) = service {
            if unit != strip_quadlet_extension(svc) {
                continue;
            }
        }

        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read {}", path.display()))?;

        let Some(image) = parse_image_name(&content) else {
            println!("Skipping {unit} (no Image= found)");
            continue;
        };

        println!("Pulling {unit}: {image}");

        if ctx.dry_run {
            println!("DRY-RUN podman pull {image}");
            dry_run_pulls += 1;
            continue;
        }

        match run_command("podman", &["pull", &image], false, None).await {
            Ok(()) => {
                println!("Updated {unit}");
                pulled += 1;
            }
            Err(err) => {
                eprintln!("Failed to pull image for {unit}");
                failures.push(format!("  {unit}: {err:#}"));
                failed += 1;
            }
        }
    }

    if ctx.dry_run {
        if dry_run_pulls == 0 {
            if let Some(ref svc) = service {
                println!("Container '{svc}' not found in {}", ctx.source_display());
            } else {
                println!("No container quadlets found in {}", ctx.source_display());
            }
        } else {
            println!();
            println!("DRY-RUN summary: {dry_run_pulls} would be pulled");
        }
        return Ok(());
    }

    if pulled == 0 && failed == 0 {
        if let Some(ref svc) = service {
            println!("Container '{svc}' not found in {}", ctx.source_display());
        } else {
            println!("No container quadlets found in {}", ctx.source_display());
        }
    } else {
        println!();
        println!("Summary: {pulled} updated, {failed} failed");
    }

    if failures.is_empty() {
        return Ok(());
    }

    bail!(
        "podman pull failed for {} image(s):\n{}",
        failures.len(),
        failures.join("\n")
    )
}

async fn status(
    ctx: &AppContext,
    service: Option<String>,
    compact: bool,
    scope: TargetScope,
) -> Result<()> {
    let targets = resolve_targets(ctx, service, scope).await?;

    if targets.is_empty() {
        // The machine contract is "one line per unit and nothing else": with
        // no units, compact prints nothing at all instead of a prose notice.
        if !compact {
            println!("No container units found in {}", ctx.source_display());
        }
        return Ok(());
    }

    if !compact {
        println!("Source: {}", ctx.source_display());
        println!("Target: {}", ctx.target_dir.display());
        println!();
    }

    let mut table = Table::new();
    if !compact {
        table
            .load_style(UTF8_FULL.with_rounded_corners())
            .set_header(vec!["#", "Link", "State", "Unit"]);
    }

    let mut linked_count = 0;
    let mut running_count = 0;
    let mut stopped_count = 0;
    let mut missing_count = 0;

    for (index, unit) in targets.iter().enumerate() {
        let linked = path_exists(&ctx.target_dir.join(format!("{unit}.container"))).await?;
        let running = if linked {
            match is_active(&container_unit(unit), ctx.timeout).await {
                Ok(running) => running,
                Err(err) => {
                    tracing::warn!("could not query state of {unit}: {err:#}");
                    false
                }
            }
        } else {
            false
        };

        let link_txt = if linked { "✅" } else { "❌" };
        let state_txt = if running {
            "🟢"
        } else if linked {
            "🟡"
        } else {
            "⚫"
        };

        if linked {
            linked_count += 1;
        } else {
            missing_count += 1;
        }

        if running {
            running_count += 1;
        } else if linked {
            stopped_count += 1;
        }

        if compact {
            // Machine contract: one undecorated TSV line per unit.
            let link = if linked { "linked" } else { "missing" };
            let state = if running {
                "running"
            } else if linked {
                "stopped"
            } else {
                "-"
            };
            println!("{unit}\t{link}\t{state}");
        } else {
            let link_color = if linked { Color::Green } else { Color::Red };
            let state_color = if running {
                Color::Green
            } else if linked {
                Color::Yellow
            } else {
                Color::DarkGrey
            };

            table.add_row(vec![
                Cell::new(index + 1),
                Cell::new(link_txt).fg(link_color),
                Cell::new(state_txt).fg(state_color),
                Cell::new(unit),
            ]);
        }
    }

    if compact {
        return Ok(());
    }

    println!("{table}");
    println!();
    println!("Summary:");
    println!("  total   = {} (container units found)", targets.len());
    println!(
        "  linked  = {} ✅ (has symlink in target dir)",
        linked_count
    );
    println!(
        "  running = {} 🟢 (active in systemd --user)",
        running_count
    );
    println!("  stopped = {} 🟡 (linked but not active)", stopped_count);
    println!(
        "  missing = {} ❌ (no symlink in target dir)",
        missing_count
    );

    Ok(())
}

async fn menu(ctx: &AppContext, service: Option<String>) -> Result<()> {
    loop {
        let rows = service_rows(ctx, service.clone()).await?;
        if rows.is_empty() {
            println!("No container units found in {}", ctx.source_display());
            return Ok(());
        }

        print!("\x1B[2J\x1B[H");
        println!("Source: {}", ctx.source_display());
        println!("Target: {}", ctx.target_dir.display());
        println!();
        print_menu_table(&rows);
        println!();
        println!("Actions: number = toggle, s = start, p = stop, r = restart, l = logs, q = quit");
        print!("Selection: ");
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        let bytes = io::stdin()
            .read_line(&mut input)
            .context("failed to read selection")?;
        if bytes == 0 {
            // EOF on stdin (for example a closed pipe or /dev/null): end the
            // menu cleanly instead of redrawing it forever.
            break;
        }
        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        if matches!(input, "q" | "quit" | "exit") {
            break;
        }

        let parts = input.split_whitespace().collect::<Vec<_>>();
        let (index, action) = match parse_menu_selection(&parts) {
            Ok(selection) => selection,
            Err(err) => {
                println!("Invalid selection: {err}");
                wait_for_enter()?;
                continue;
            }
        };
        let Some(row) = rows.get(index.saturating_sub(1)) else {
            println!("Invalid selection: {index}");
            wait_for_enter()?;
            continue;
        };

        let action = action.unwrap_or(if row.running { "p" } else { "s" });
        let result = match action {
            "s" | "start" => start(ctx, Some(row.unit.clone())).await,
            "p" | "stop" => stop(ctx, Some(row.unit.clone())).await,
            "r" | "restart" => restart(ctx, Some(row.unit.clone())).await,
            "l" | "logs" => {
                print!("\x1B[2J\x1B[H");
                println!("Recent logs for {}", row.unit);
                println!();
                recent_logs(&row.unit, ctx.timeout).await
            }
            other => {
                println!("Unknown action: {other}");
                wait_for_enter()?;
                continue;
            }
        };

        if let Err(err) = result {
            // A failing action must not tear down the whole menu: report it and
            // return to the prompt.
            println!("Action failed: {err:#}");
            wait_for_enter()?;
        }
    }

    Ok(())
}

struct ServiceRow {
    unit: String,
    linked: bool,
    running: bool,
}

async fn service_rows(ctx: &AppContext, service: Option<String>) -> Result<Vec<ServiceRow>> {
    let targets = resolve_targets(ctx, service, TargetScope::Wide).await?;
    let mut rows = Vec::with_capacity(targets.len());

    for unit in targets {
        let linked = path_exists(&ctx.target_dir.join(format!("{unit}.container"))).await?;
        let running = if linked {
            match is_active(&container_unit(&unit), ctx.timeout).await {
                Ok(running) => running,
                Err(err) => {
                    tracing::warn!("could not query state of {unit}: {err:#}");
                    false
                }
            }
        } else {
            false
        };

        rows.push(ServiceRow {
            unit,
            linked,
            running,
        });
    }

    Ok(rows)
}

fn print_menu_table(rows: &[ServiceRow]) {
    let mut table = Table::new();
    table
        .load_style(UTF8_FULL.with_rounded_corners())
        .set_header(vec!["#", "Link", "State", "Action", "Unit"]);

    for (index, row) in rows.iter().enumerate() {
        let link_txt = if row.linked { "✅" } else { "❌" };
        let state_txt = if row.running {
            "🟢 running"
        } else if row.linked {
            "🟡 stopped"
        } else {
            "⚫ missing"
        };
        let action_txt = if row.running {
            "p stop | r restart | l logs"
        } else if row.linked {
            "s start | l logs"
        } else {
            "s install+start"
        };

        let link_color = if row.linked { Color::Green } else { Color::Red };
        let state_color = if row.running {
            Color::Green
        } else if row.linked {
            Color::Yellow
        } else {
            Color::DarkGrey
        };

        table.add_row(vec![
            Cell::new(index + 1),
            Cell::new(link_txt).fg(link_color),
            Cell::new(state_txt).fg(state_color),
            Cell::new(action_txt),
            Cell::new(&row.unit),
        ]);
    }

    println!("{table}");
}

fn parse_menu_selection<'a>(parts: &[&'a str]) -> Result<(usize, Option<&'a str>)> {
    if parts.is_empty() {
        return Err(anyhow!("empty selection"));
    }

    if let Ok(index) = parts[0].parse::<usize>() {
        validate_menu_index(index)?;
        return Ok((index, parts.get(1).copied()));
    }

    let Some(index) = parts.get(1).and_then(|value| value.parse::<usize>().ok()) else {
        return Err(anyhow!("use: <number> [action] or <action> <number>"));
    };
    validate_menu_index(index)?;
    Ok((index, Some(parts[0])))
}

fn validate_menu_index(index: usize) -> Result<()> {
    if index == 0 {
        return Err(anyhow!("selection must be a positive row number"));
    }
    Ok(())
}

fn wait_for_enter() -> Result<()> {
    print!("Press Enter to continue...");
    io::stdout().flush().context("failed to flush stdout")?;
    let mut input = String::new();
    let bytes = io::stdin()
        .read_line(&mut input)
        .context("failed to wait for enter")?;
    if bytes == 0 {
        // EOF: there is nobody to press Enter, so stop waiting instead of
        // spinning on an empty read.
        return Ok(());
    }
    Ok(())
}

async fn clean_volumes(ctx: &AppContext, yes: bool) -> Result<()> {
    debug!("Removing podman volumes declared in .volume files");

    let files = collect_quadlets(&ctx.source_dirs).await?;
    let mut volume_names = Vec::new();

    for path in files {
        if extension_of_path(&path) != Some("volume") {
            continue;
        }

        let content = fs::read_to_string(&path)
            .await
            .with_context(|| format!("failed to read {}", path.display()))?;

        let Some(volume_name) = parse_volume_name(&content) else {
            debug!("Skipping {} (VolumeName not found)", path.display());
            continue;
        };

        volume_names.push(volume_name);
    }

    if volume_names.is_empty() {
        println!("No podman volumes declared in .volume files");
        return Ok(());
    }

    if ctx.dry_run {
        for volume_name in volume_names {
            println!("DRY-RUN podman volume rm -f {volume_name}");
        }
        return Ok(());
    }

    if !yes && !confirm_clean_volumes(&volume_names)? {
        println!("Aborted");
        return Ok(());
    }

    for volume_name in volume_names {
        run_command(
            "podman",
            &["volume", "rm", "-f", &volume_name],
            false,
            ctx.timeout,
        )
        .await?;
        debug!("Removed volume: {volume_name}");
    }

    Ok(())
}

fn confirm_clean_volumes(volume_names: &[String]) -> Result<bool> {
    println!("This will remove {} podman volume(s):", volume_names.len());
    for volume_name in volume_names {
        println!("  {volume_name}");
    }
    print!("Continue? [y/N]: ");
    io::stdout().flush().context("failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read confirmation")?;

    Ok(matches!(input.trim(), "y" | "Y" | "yes" | "YES"))
}

/// One symlink `uninstall` would remove from the target directory, together with
/// the systemd unit whose service is stopped first.
struct BaseLink {
    target: PathBuf,
    unit: String,
}

/// Everything `purge` would remove: the base (symlinks + named volumes) plus the
/// opt-in extras (secrets, images and host-side bind data).
#[derive(Default)]
struct PurgePlan {
    links: Vec<BaseLink>,
    volumes: Vec<String>,
    secrets: Vec<String>,
    images: Vec<String>,
    binds: Vec<VolumeMount>,
}

impl PurgePlan {
    fn is_empty(&self) -> bool {
        self.links.is_empty()
            && self.volumes.is_empty()
            && self.secrets.is_empty()
            && self.images.is_empty()
            && self.binds.is_empty()
    }
}

/// Where the cross-stack guard leaves a declared secret or image.
enum Guard {
    /// No other stack declares it: safe to remove.
    Removable,
    /// Another stack still declares it; skip, naming that stack.
    Shared(String),
    /// The current stack could not be resolved, so nothing can be proven
    /// unshared: skip (fail-closed).
    Unresolved,
}

/// Uninstalls the current stack and removes the resources selected by the flags.
///
/// The base (always included) removes the stack's symlinks (`uninstall`) and its
/// named volumes (`clean-volumes`); the extra categories (secrets, images,
/// host-side bind data) are opt-in. The whole plan —base included— is built up
/// front and a single confirmation is asked before anything is destroyed, so
/// declining leaves the system untouched. Secrets and images still declared by
/// another stack under the stacks root are skipped unless `force` is set. `yes`
/// skips the prompt and `ctx.dry_run` only prints the plan.
async fn purge(
    ctx: &AppContext,
    _volumes: bool,
    secrets: bool,
    images: bool,
    data: bool,
    force: bool,
    yes: bool,
) -> Result<()> {
    debug!("Purging the current stack");

    let plan = build_purge_plan(ctx, secrets, images, data, force).await?;
    if plan.is_empty() {
        debug!("Nothing to purge");
        return Ok(());
    }

    if ctx.dry_run {
        print_dry_run_plan(&plan);
        return Ok(());
    }

    if !yes && !confirm_purge(&plan)? {
        println!("Aborted");
        return Ok(());
    }

    // Base first, then the extras. `clean_volumes` is called with `yes = true`
    // because the single confirmation above already covered the named volumes.
    uninstall(ctx).await?;
    clean_volumes(ctx, true).await?;

    remove_secrets(ctx, &plan.secrets).await;
    remove_images(ctx, &plan.images).await;
    remove_binds(ctx, &plan.binds).await;

    Ok(())
}

/// Builds the full purge plan: the base (target symlinks and named volumes) plus
/// the requested extra categories, applying the cross-stack guard to secrets and
/// images. An unreadable quadlet degrades to a `warn!` and is skipped so one bad
/// file cannot abort the whole plan.
async fn build_purge_plan(
    ctx: &AppContext,
    secrets: bool,
    images: bool,
    data: bool,
    force: bool,
) -> Result<PurgePlan> {
    let files = collect_quadlets(&ctx.source_dirs).await?;

    let mut plan = PurgePlan {
        links: collect_base_links(ctx, &files).await?,
        volumes: volume_names(&files).await,
        ..PurgePlan::default()
    };

    let mut declared_secrets: Vec<String> = Vec::new();
    let mut declared_images: Vec<String> = Vec::new();
    let mut mounts: Vec<VolumeMount> = Vec::new();

    for path in &files {
        if extension_of_path(path) != Some("container") {
            continue;
        }
        let content = match fs::read_to_string(path).await {
            Ok(content) => content,
            Err(e) => {
                warn!("cannot read quadlet {}: {e}", path.display());
                continue;
            }
        };
        if secrets {
            declared_secrets.extend(parse_secret_names(&content));
        }
        if images {
            if let Some(image) = parse_image_name(&content) {
                declared_images.push(image);
            }
        }
        if data {
            mounts.extend(parse_volume_binds(&content));
        }
    }

    if secrets || images {
        let current = current_stack_name(ctx).await;
        if current.is_none() {
            warn!(
                "current stack not resolved under {}; treating every declared secret/image as shared (fail-closed)",
                ctx.stacks_root.display()
            );
        }
        let resources = scan_stack_resources(&ctx.stacks_root).await?;

        declared_secrets.sort();
        declared_secrets.dedup();
        for name in declared_secrets {
            match classify_guard(
                &resources,
                current.as_deref(),
                &name,
                ResourceKind::Secret,
                force,
            ) {
                Guard::Removable => plan.secrets.push(name),
                Guard::Shared(other) => {
                    println!("skip: secreto '{name}' todavía lo usa la stack '{other}'");
                }
                Guard::Unresolved => {
                    println!("skip: secreto '{name}' (stack actual no resuelta; no se borra)");
                }
            }
        }

        declared_images.sort();
        declared_images.dedup();
        for image in declared_images {
            match classify_guard(
                &resources,
                current.as_deref(),
                &image,
                ResourceKind::Image,
                force,
            ) {
                Guard::Removable => plan.images.push(image),
                Guard::Shared(other) => {
                    println!("skip: imagen '{image}' todavía la usa la stack '{other}'");
                }
                Guard::Unresolved => {
                    println!("skip: imagen '{image}' (stack actual no resuelta; no se borra)");
                }
            }
        }
    }

    if data {
        mounts.sort_by(|a, b| a.source.cmp(&b.source));
        mounts.dedup_by(|a, b| a.source == b.source);
        plan.binds = mounts;
    }

    Ok(plan)
}

/// Target-directory symlinks that `uninstall` would stop and remove, mirroring
/// its naming so the dry-run plan matches the real execution.
async fn collect_base_links(ctx: &AppContext, files: &[PathBuf]) -> Result<Vec<BaseLink>> {
    let mut links = Vec::new();
    for src in files {
        let name = basename(src)?;
        let target = ctx.target_dir.join(&name);
        let Ok(meta) = fs::symlink_metadata(&target).await else {
            continue;
        };
        if !meta.file_type().is_symlink() {
            continue;
        }
        links.push(BaseLink {
            unit: generated_unit(&strip_dot_extension(&name), extension_of(&name)),
            target,
        });
    }
    links.sort_by(|a, b| a.target.cmp(&b.target));
    Ok(links)
}

/// Named volumes declared by the stack's `.volume` files. An unreadable file is
/// warned about and skipped.
async fn volume_names(files: &[PathBuf]) -> Vec<String> {
    let mut names = Vec::new();
    for path in files {
        if extension_of_path(path) != Some("volume") {
            continue;
        }
        let content = match fs::read_to_string(path).await {
            Ok(content) => content,
            Err(e) => {
                warn!("cannot read quadlet {}: {e}", path.display());
                continue;
            }
        };
        if let Some(name) = parse_volume_name(&content) {
            names.push(name);
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Decides whether a declared secret/image may be removed, applying the
/// cross-stack guard. `force` bypasses the guard; an unresolved current stack is
/// fail-closed (treated as shared).
fn classify_guard(
    resources: &HashMap<String, StackResources>,
    current: Option<&str>,
    resource: &str,
    kind: ResourceKind,
    force: bool,
) -> Guard {
    if force {
        return Guard::Removable;
    }
    let Some(current) = current else {
        return Guard::Unresolved;
    };
    match other_stack_declaring(resources, current, resource, kind) {
        Some(other) => Guard::Shared(other),
        None => Guard::Removable,
    }
}

/// Prints every action the plan would take under `--dry-run`, without executing
/// anything.
fn print_dry_run_plan(plan: &PurgePlan) {
    for link in &plan.links {
        println!("DRY-RUN systemctl --user stop {}", link.unit);
        println!("DRY-RUN remove {}", link.target.display());
    }
    println!("DRY-RUN systemctl --user daemon-reload");
    for volume in &plan.volumes {
        println!("DRY-RUN podman volume rm -f {volume}");
    }
    for name in &plan.secrets {
        println!("DRY-RUN podman secret rm {name}");
    }
    for image in &plan.images {
        println!("DRY-RUN podman rmi {image}");
    }
    for mount in &plan.binds {
        println!("{}", dry_run_remove_line(mount));
    }
}

/// The `DRY-RUN` line for removing one bind path, picking `podman unshare` when
/// the declaring quadlet maps a uid.
fn dry_run_remove_line(mount: &VolumeMount) -> String {
    if mount.uid_mapped {
        format!("DRY-RUN podman unshare rm -rf {}", mount.source.display())
    } else {
        format!("DRY-RUN rm -rf {}", mount.source.display())
    }
}

/// Lists every resource the plan would remove —base included— and asks for a
/// single confirmation.
fn confirm_purge(plan: &PurgePlan) -> Result<bool> {
    println!("This purge will remove:");
    if !plan.links.is_empty() {
        println!("  links:");
        for link in &plan.links {
            println!("    {}", link.target.display());
        }
    }
    if !plan.volumes.is_empty() {
        println!("  volumes:");
        for volume in &plan.volumes {
            println!("    {volume}");
        }
    }
    if !plan.secrets.is_empty() {
        println!("  secrets:");
        for name in &plan.secrets {
            println!("    {name}");
        }
    }
    if !plan.images.is_empty() {
        println!("  images:");
        for image in &plan.images {
            println!("    {image}");
        }
    }
    if !plan.binds.is_empty() {
        println!("  data:");
        for mount in &plan.binds {
            println!("    {}", mount.source.display());
        }
    }
    print!("Continue? [y/N]: ");
    io::stdout().flush().context("failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read confirmation")?;

    Ok(matches!(input.trim(), "y" | "Y" | "yes" | "YES"))
}

/// Removes each declared secret, warning and continuing on a failure (for
/// example a secret that is already absent).
async fn remove_secrets(ctx: &AppContext, names: &[String]) {
    for name in names {
        match run_command("podman", &["secret", "rm", name], false, ctx.timeout).await {
            Ok(()) => debug!("Removed secret: {name}"),
            Err(e) => warn!("failed to remove secret '{name}' (continuing): {e:#}"),
        }
    }
}

/// Removes each declared image, warning and continuing on a failure (for
/// example an image that is no longer present locally).
async fn remove_images(ctx: &AppContext, images: &[String]) {
    for image in images {
        match run_command("podman", &["rmi", image], false, ctx.timeout).await {
            Ok(()) => debug!("Removed image: {image}"),
            Err(e) => warn!("failed to remove image '{image}' (continuing): {e:#}"),
        }
    }
}

/// Removes the eligible bind directories, using `podman unshare rm -rf` when the
/// declaring quadlet maps a uid and a plain `rm -rf` otherwise. Entries that are
/// not directories are skipped; a failure warns and continues.
async fn remove_binds(ctx: &AppContext, mounts: &[VolumeMount]) {
    for mount in mounts {
        let is_dir = fs::metadata(&mount.source)
            .await
            .map(|meta| meta.is_dir())
            .unwrap_or(false);
        if !is_dir {
            debug!("Skipping {} (not a directory)", mount.source.display());
            continue;
        }

        let path = mount.source.to_string_lossy().to_string();
        let result = if mount.uid_mapped {
            run_command(
                "podman",
                &["unshare", "rm", "-rf", &path],
                false,
                ctx.timeout,
            )
            .await
        } else {
            run_command("rm", &["-rf", &path], false, ctx.timeout).await
        };

        match result {
            Ok(()) => debug!("Removed data path: {}", mount.source.display()),
            Err(e) => warn!(
                "failed to remove {} (continuing): {e:#}",
                mount.source.display()
            ),
        }
    }
}

/// Returns `true` when `path` is a regular file with any execute bit set
/// (owner, group or other).
///
/// This is enough to pick a generator among distro candidates: it never spawns
/// the binary, it only checks that it is plausibly runnable.
///
/// Limitation: a group/other-only execute bit counts, so the file may not be
/// runnable by the invoking user. That is acceptable for probing well-known
/// distro paths, where the generator is a system binary shipped world-executable.
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Locates the podman quadlet generator binary.
///
/// `env_override` (from `QCTL_QUADLET_BIN`) wins when it points at an existing
/// executable. Otherwise the first existing, executable entry of `candidates`
/// is returned, in order. When nothing matches, the error lists every probed
/// path so the user can see what was tried and how to fix it.
fn find_quadlet_bin(candidates: &[PathBuf], env_override: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = env_override.as_ref() {
        if is_executable_file(path) {
            return Ok(path.clone());
        }
        debug!(
            "QCTL_QUADLET_BIN={} is not an executable file; falling back to candidates",
            path.display()
        );
    }

    for candidate in candidates {
        if is_executable_file(candidate) {
            return Ok(candidate.clone());
        }
    }

    let mut probed: Vec<String> = Vec::new();
    if let Some(override_path) = env_override {
        probed.push(format!("QCTL_QUADLET_BIN={}", override_path.display()));
    }
    probed.extend(candidates.iter().map(|p| p.display().to_string()));

    bail!(
        "quadlet generator not found; probed: {}. Install podman or set QCTL_QUADLET_BIN to the generator binary.",
        probed.join(", ")
    )
}

/// Distro candidate paths for the podman quadlet generator, probed in order:
/// usrmerge systems (Fedora, openSUSE) first, then Debian/Arch layouts.
const QUADLET_BIN_CANDIDATES: &[&str] = &["/usr/libexec/podman/quadlet", "/usr/lib/podman/quadlet"];

/// The [`QUADLET_BIN_CANDIDATES`] distro paths as owned [`PathBuf`]s, preserving
/// probe order so the usrmerge layout wins when both exist.
fn quadlet_candidates() -> Vec<PathBuf> {
    QUADLET_BIN_CANDIDATES.iter().map(PathBuf::from).collect()
}

/// Validates a single quadlet with the podman generator.
///
/// The generator is located by [`find_quadlet_bin`]: the distro candidates are
/// probed in order and `QCTL_QUADLET_BIN` overrides them, so hosts with a
/// non-standard layout (and tests) can point at their own binary.
async fn check_quadlet(ctx: &AppContext, quadlet: String) -> Result<()> {
    debug!("Checking {quadlet}");
    let candidates = quadlet_candidates();
    let bin = find_quadlet_bin(
        &candidates,
        std::env::var_os("QCTL_QUADLET_BIN").map(PathBuf::from),
    )?;
    run_command(
        &bin.to_string_lossy(),
        &["-dryrun", "-user", &ctx.user, &quadlet],
        false,
        ctx.timeout,
    )
    .await?;
    Ok(())
}

/// A quadlet discovered while scanning a stack: which stack owns it, its file
/// basename and the `ContainerName=` it declares, if any.
struct StackUnit {
    stack: String,
    basename: String,
    container_name: Option<String>,
}

/// Cross-stack problems found while auditing a stacks root.
struct AuditFindings {
    /// Unit basenames declared by two or more stacks, with the stacks involved.
    basename_collisions: Vec<(String, Vec<String>)>,
    /// `ContainerName=` values declared by two or more quadlets, with the stacks
    /// involved.
    containername_collisions: Vec<(String, Vec<String>)>,
    /// Units whose basename is neither `<stack>.<ext>` nor `<stack>-<role>`.
    naming_warnings: Vec<(String, String)>,
}

impl AuditFindings {
    fn has_hard_collision(&self) -> bool {
        !self.basename_collisions.is_empty() || !self.containername_collisions.is_empty()
    }
}

/// Scans every stack directory directly under `root`, collecting one `StackUnit`
/// per quadlet file. A missing or unreadable root yields an empty list: there is
/// nothing to audit, which is the safe default for both `install` and `check`
/// (the audit path warns about an unreadable root separately).
///
/// Individual failures never abort the scan: an unreadable file or an invalid
/// filename is reported with `warn!` and skipped, so one bad quadlet cannot hide
/// the rest of the audit (and cannot silently vanish either).
async fn scan_stacks_root(root: &Path) -> Result<Vec<StackUnit>> {
    let mut units = Vec::new();
    let Ok(mut stacks) = fs::read_dir(root).await else {
        return Ok(units);
    };

    while let Some(entry) = stacks.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let stack = entry.file_name().to_string_lossy().to_string();

        for dir in stack_quadlet_dirs(&entry.path()) {
            let mut files = match fs::read_dir(&dir).await {
                Ok(files) => files,
                Err(e) => {
                    warn!("cannot read stack directory {}: {e}", dir.display());
                    continue;
                }
            };
            while let Some(file) = files.next_entry().await? {
                let path = file.path();
                let Some(ext) = extension_of_path(&path) else {
                    continue;
                };
                if !QUADLET_EXTENSIONS.contains(&ext) {
                    continue;
                }

                let basename = match basename(&path) {
                    Ok(name) => name,
                    Err(e) => {
                        warn!(
                            "skipping quadlet with an invalid name {}: {e}",
                            path.display()
                        );
                        continue;
                    }
                };
                let content = match fs::read_to_string(&path).await {
                    Ok(content) => content,
                    Err(e) => {
                        warn!("cannot read quadlet {}: {e}", path.display());
                        continue;
                    }
                };

                units.push(StackUnit {
                    stack: stack.clone(),
                    basename,
                    container_name: parse_container_name(&content),
                });
            }
        }
    }

    Ok(units)
}

/// Directories under one stack that may hold quadlets: the optional nested
/// `quadlets/` subdirectory (mirroring `discover_source_dirs`), then the stack
/// directory itself.
fn stack_quadlet_dirs(stack_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::with_capacity(2);
    let nested = stack_dir.join("quadlets");
    if nested.is_dir() {
        dirs.push(nested);
    }
    dirs.push(stack_dir.to_path_buf());
    dirs
}

/// Classifies the cross-stack problems in a set of scanned units: hard
/// collisions (a duplicated basename or `ContainerName=`) and naming-convention
/// drift.
fn analyze_units(units: &[StackUnit]) -> AuditFindings {
    let mut basename_quadlets: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    let mut containername_quadlets: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    let mut naming_warnings = Vec::new();

    for unit in units {
        // Identify each declaring quadlet by `(stack, basename)`: `ContainerName=`
        // must be unique in Podman globally, so two quadlets of the *same* stack
        // collide too, not only quadlets in different stacks.
        let quadlet = (unit.stack.as_str(), unit.basename.as_str());
        basename_quadlets
            .entry(unit.basename.as_str())
            .or_default()
            .push(quadlet);

        if let Some(name) = unit.container_name.as_deref() {
            containername_quadlets
                .entry(name)
                .or_default()
                .push(quadlet);
        }

        let stem = strip_dot_extension(&unit.basename);
        if stem != unit.stack && !stem.starts_with(&format!("{}-", unit.stack)) {
            naming_warnings.push((unit.basename.clone(), unit.stack.clone()));
        }
    }

    let basename_collisions = collect_collisions(basename_quadlets);
    let containername_collisions = collect_collisions(containername_quadlets);
    naming_warnings.sort();

    AuditFindings {
        basename_collisions,
        containername_collisions,
        naming_warnings,
    }
}

/// Keeps only the entries declared by two or more *distinct quadlets* — keyed by
/// `(stack, basename)`, never deduplicated down to stacks — returning the sorted,
/// deduplicated stacks involved.
fn collect_collisions(groups: HashMap<&str, Vec<(&str, &str)>>) -> Vec<(String, Vec<String>)> {
    let mut collisions: Vec<(String, Vec<String>)> = groups
        .into_iter()
        .filter_map(|(name, quadlets)| {
            let mut quadlets = quadlets;
            quadlets.sort_unstable();
            quadlets.dedup();
            if quadlets.len() < 2 {
                return None;
            }
            let mut stacks: Vec<String> = quadlets
                .into_iter()
                .map(|(stack, _)| stack.to_string())
                .collect();
            stacks.sort();
            stacks.dedup();
            Some((name.to_string(), stacks))
        })
        .collect();
    collisions.sort();
    collisions
}

/// Name of the stack the process is running from, when the current directory is
/// a stack directory under the stacks root. A stack may keep its quadlets in a
/// nested `quadlets/` directory, so running from `available/<stack>/quadlets`
/// also resolves to `<stack>`. `None` means the caller is not installing a stack
/// from the root, so the global guard does not apply.
async fn current_stack_name(ctx: &AppContext) -> Option<String> {
    let root = fs::canonicalize(&ctx.stacks_root).await.ok()?;
    // A nested `quadlets/` directory belongs to its parent stack.
    let stack_dir = if ctx.current_dir.file_name().and_then(|n| n.to_str()) == Some("quadlets") {
        ctx.current_dir.parent()?.to_path_buf()
    } else {
        ctx.current_dir.clone()
    };
    let parent = fs::canonicalize(stack_dir.parent()?).await.ok()?;
    if parent != root {
        return None;
    }
    stack_dir
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
}

/// Audits the stacks root without touching anything: hard collisions and naming
/// warnings go to stderr, and at least one hard collision fails the command.
/// Warnings alone never fail.
async fn audit_stacks_root(ctx: &AppContext) -> Result<()> {
    if let Err(e) = fs::read_dir(&ctx.stacks_root).await {
        warn!(
            "stacks root {} is not readable; nothing to audit: {e}",
            ctx.stacks_root.display()
        );
    }

    let units = scan_stacks_root(&ctx.stacks_root).await?;
    let findings = analyze_units(&units);

    for (name, stacks) in &findings.basename_collisions {
        warn!(
            "collision: unit basename '{name}' is declared by stacks: {}",
            stacks.join(", ")
        );
    }
    for (name, stacks) in &findings.containername_collisions {
        warn!(
            "collision: ContainerName={name} is declared by stacks: {}",
            stacks.join(", ")
        );
    }
    for (name, stack) in &findings.naming_warnings {
        warn!(
            "naming: '{name}' in stack '{stack}' is neither '<stack>.<ext>' nor '<stack>-<role>'"
        );
    }

    if findings.has_hard_collision() {
        let hard = findings.basename_collisions.len() + findings.containername_collisions.len();
        bail!(
            "stacks root {} has {hard} hard collision(s)",
            ctx.stacks_root.display()
        );
    }

    debug!(
        "Audited {} unit(s) under {}",
        units.len(),
        ctx.stacks_root.display()
    );
    Ok(())
}

/// Aborts `install` atomically when the current stack's units collide with
/// another stack under the stacks root: the same unit basename, or the same
/// `ContainerName=`, declared by two stacks. Runs before any filesystem change,
/// so an abort leaves the target directory untouched.
async fn ensure_no_global_collision(ctx: &AppContext) -> Result<()> {
    let Some(stack) = current_stack_name(ctx).await else {
        return Ok(());
    };

    let units = scan_stacks_root(&ctx.stacks_root).await?;
    let findings = analyze_units(&units);

    let mut problems = Vec::new();
    for (name, stacks) in &findings.basename_collisions {
        if stacks.iter().any(|s| s == &stack) {
            problems.push(format!(
                "unit basename '{name}' is declared by stacks: {}",
                stacks.join(", ")
            ));
        }
    }
    for (name, stacks) in &findings.containername_collisions {
        if stacks.iter().any(|s| s == &stack) {
            problems.push(format!(
                "ContainerName={name} is declared by stacks: {}",
                stacks.join(", ")
            ));
        }
    }

    if !problems.is_empty() {
        bail!(
            "refusing to install stack '{stack}': global collision(s):\n{}",
            problems.join("\n")
        );
    }

    Ok(())
}

async fn logs(service: String, extra: bool) -> Result<()> {
    let unit = container_unit(&strip_quadlet_extension(&service));
    // `--no-pager` goes last on purpose: a followed stream must not hand the
    // terminal over to a pager that would keep `qctl` alive after journalctl
    // writes nothing more to follow.
    if extra {
        run_command_streaming("journalctl", &["--user", "-xsfu", &unit, "--no-pager"]).await?;
    } else {
        run_command_streaming(
            "journalctl",
            &[
                "--user",
                "-u",
                &unit,
                "-f",
                "--since",
                "1 hour ago",
                "--no-pager",
            ],
        )
        .await?;
    }

    Ok(())
}

async fn recent_logs(service: &str, timeout: Option<Duration>) -> Result<()> {
    let unit = container_unit(&strip_quadlet_extension(service));
    run_command(
        "journalctl",
        &[
            "--user",
            "-u",
            &unit,
            "--since",
            "1 hour ago",
            "-n",
            "80",
            "--no-pager",
        ],
        true,
        timeout,
    )
    .await
}

fn discover_source_dirs(root_dir: &Path) -> Vec<PathBuf> {
    let mut source_dirs = Vec::with_capacity(2);
    let nested_quadlets_dir = root_dir.join("quadlets");

    if nested_quadlets_dir.is_dir() {
        source_dirs.push(nested_quadlets_dir);
    }

    source_dirs.push(root_dir.to_path_buf());
    source_dirs
}

/// True when `<unit>.container` exists as a source quadlet or as an entry in the
/// target directory. The target directory is included so a unit that is already
/// installed keeps working after its project source disappears: `start`/`stop`
/// must still recognise it instead of rejecting the name.
async fn unit_is_known(ctx: &AppContext, unit: &str) -> Result<bool> {
    let file_name = format!("{unit}.container");
    if find_source_quadlet(ctx, &file_name).await?.is_some() {
        return Ok(true);
    }
    path_exists(&ctx.target_dir.join(&file_name)).await
}

/// Rejects a service argument that matches no quadlet, so mutating commands fail
/// with `no such quadlet: <name>` (non-zero exit) before linking, reloading or
/// invoking `systemctl`/`podman`. Read-only commands never call this.
async fn ensure_service_known(ctx: &AppContext, service: &Option<String>) -> Result<()> {
    if let Some(svc) = service {
        let unit = strip_quadlet_extension(svc);
        if !unit_is_known(ctx, &unit).await? {
            bail!("no such quadlet: {unit}");
        }
    }
    Ok(())
}

/// How far `resolve_targets` looks when no service was named.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetScope {
    /// Only the current project's quadlets (the source dirs).
    Project,
    /// The whole target directory as well, to show what any project has installed.
    Wide,
}

async fn resolve_targets(
    ctx: &AppContext,
    service: Option<String>,
    scope: TargetScope,
) -> Result<Vec<String>> {
    if let Some(svc) = service {
        return Ok(vec![strip_quadlet_extension(&svc)]);
    }

    let mut out = Vec::new();
    let files = collect_quadlets(&ctx.source_dirs).await?;
    for path in files {
        if extension_of_path(&path) == Some("container") {
            out.push(strip_dot_extension(&basename(&path)?));
        }
    }

    if scope == TargetScope::Wide && path_exists(&ctx.target_dir).await? {
        let installed_files = collect_quadlets(std::slice::from_ref(&ctx.target_dir)).await?;
        for path in installed_files {
            if extension_of_path(&path) == Some("container") {
                out.push(strip_dot_extension(&basename(&path)?));
            }
        }
    }

    out.sort();
    out.dedup();
    Ok(out)
}

/// Collects every quadlet file found in `source_dirs`, keyed by file name.
///
/// When the same name shows up twice, the canonical targets are compared: entries
/// that resolve to the same file (for example a stale self-link left inside the
/// target directory by an older version) collapse into one, preferring the real
/// file over the symlink pointing at it. Two *different* files that merely share
/// a name still fail, so the user has to disambiguate instead of `qctl` silently
/// picking one.
async fn collect_quadlets(source_dirs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    // Each entry keeps its path and whether it is a symlink, so a collision can
    // be resolved by canonical target and the real file preferred.
    let mut out: Vec<(PathBuf, bool)> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for source_dir in source_dirs {
        let mut entries = fs::read_dir(source_dir)
            .await
            .with_context(|| format!("failed to read {}", source_dir.display()))?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let Some(ext) = extension_of_path(&path) else {
                continue;
            };
            if !QUADLET_EXTENSIONS.contains(&ext) {
                continue;
            }

            let name = basename(&path)?;
            let is_symlink = entry.file_type().await?.is_symlink();

            if let Some(&index) = seen.get(&name) {
                let same_file = {
                    let previous = &out[index].0;
                    let previous_canon = fs::canonicalize(previous).await.ok();
                    let new_canon = fs::canonicalize(&path).await.ok();
                    previous_canon.is_some() && previous_canon == new_canon
                };

                if same_file {
                    // The same file reached twice (for example a stale self-link
                    // left by an older version): collapse to a single entry,
                    // keeping the real file over the symlink pointing at it.
                    let previous_is_symlink = out[index].1;
                    if previous_is_symlink && !is_symlink {
                        out[index] = (path, is_symlink);
                    }
                    continue;
                }

                return Err(anyhow!(
                    "duplicate quadlet file name '{}' found in {} and {}",
                    name,
                    out[index].0.display(),
                    path.display()
                ));
            }

            seen.insert(name, out.len());
            out.push((path, is_symlink));
        }
    }

    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out.into_iter().map(|(path, _)| path).collect())
}

/// Creates `dest` as a symlink to `src`. An existing symlink is replaced, but a
/// regular file (or any other non-symlink entry) is never overwritten: the link
/// is skipped, a warning goes to stderr and `false` is returned. Returns `true`
/// when a link was created.
async fn link_or_replace(src: &Path, dest: &Path) -> Result<bool> {
    if let Ok(meta) = fs::symlink_metadata(dest).await {
        if !meta.file_type().is_symlink() {
            warn!(
                "not replacing {} (not a symlink); leaving it untouched",
                dest.display()
            );
            return Ok(false);
        }
        fs::remove_file(dest)
            .await
            .with_context(|| format!("failed to remove existing {}", dest.display()))?;
    }

    let src_path = src.to_path_buf();
    let dest_path = dest.to_path_buf();
    let src_for_link = src_path.clone();
    let dest_for_link = dest_path.clone();
    tokio::task::spawn_blocking(move || std::os::unix::fs::symlink(&src_for_link, &dest_for_link))
        .await
        .map_err(|e| anyhow!("symlink task join error: {e}"))?
        .with_context(|| {
            format!(
                "failed creating symlink {} -> {}",
                src_path.display(),
                dest_path.display()
            )
        })?;

    Ok(true)
}

async fn path_exists(path: &Path) -> Result<bool> {
    Ok(fs::symlink_metadata(path).await.is_ok())
}

fn basename(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("invalid file name for {}", path.display()))?;
    Ok(name.to_string())
}

fn extension_of(file_name: &str) -> &str {
    file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or_default()
}

fn extension_of_path(path: &Path) -> Option<&str> {
    path.extension().and_then(|e| e.to_str())
}

/// Strips the final dotted extension from a file name, blindly.
///
/// This is correct only for *real file names* whose extension is known to be a
/// quadlet extension (for example a basename ending in `.container`). For a
/// raw user argument use [`strip_quadlet_extension`], which leaves a dotted stem
/// such as `voicebox.prod` intact.
fn strip_dot_extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .to_string()
}

/// Strips the final extension only when it is a known quadlet extension.
///
/// A *user-supplied* service argument is not guaranteed to be a file name: a
/// stem may itself contain dots (`voicebox.prod`) and must survive intact. The
/// blind [`strip_dot_extension`] would turn it into `voicebox`, so callers that
/// handle raw arguments use this variant instead:
///
/// ```text
/// voicebox.prod            -> voicebox.prod   (".prod" is not a quadlet ext)
/// voicebox.prod.container  -> voicebox.prod
/// voicebox                 -> voicebox
/// ```
/// Recorta solo extensiones de quadlet conocidas; deja intacto un stem punteado
/// y no añade sufijos.
fn strip_quadlet_extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if QUADLET_EXTENSIONS.contains(&ext) => stem.to_string(),
        _ => name.to_string(),
    }
}

/// Forms the systemd user unit for a container quadlet stem.
///
/// Unlike the removed dot heuristic, this always appends `.service`, so a stem
/// that contains dots still resolves correctly (`voicebox.prod` ->
/// `"voicebox.prod.service"`). Callers must pass a stem, never a formed unit.
/// Recibe un stem ya resuelto y devuelve la unidad YA formada; no recorta sufijos.
fn container_unit(stem: &str) -> String {
    format!("{stem}.service")
}

/// Returns the trimmed, non-empty value of the first `key=` line in a quadlet,
/// or `None` when the key is absent or blank. Leading indentation is tolerated
/// and an inline `#` comment is stripped, so `  ContainerName=foo # note` reads
/// as `foo`.
fn parse_ini_value(content: &str, key: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let value = line.trim_start().strip_prefix(key)?;
        let value = value.split('#').next().unwrap_or_default().trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn parse_volume_name(content: &str) -> Option<String> {
    parse_ini_value(content, "VolumeName=")
}

fn parse_container_name(content: &str) -> Option<String> {
    parse_ini_value(content, "ContainerName=")
}

fn parse_image_name(content: &str) -> Option<String> {
    parse_ini_value(content, "Image=")
}

/// Names of every secret declared by a quadlet's `Secret=` entries.
///
/// A `.container` may declare several `Secret=` lines and each value may carry
/// options after the name (`Secret=my_api_key,type=env,target=API_KEY`). The
/// name is the text before the first comma; leading indentation and an inline
/// `#` comment are tolerated. Unlike [`parse_ini_value`], which returns only the
/// first match, this returns every declaration so a multi-secret quadlet is not
/// silently truncated. An absent key yields an empty list.
fn parse_secret_names(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in content.lines() {
        let Some(value) = line.trim_start().strip_prefix("Secret=") else {
            continue;
        };
        let value = value.split('#').next().unwrap_or_default();
        let name = value.split(',').next().unwrap_or_default().trim();
        if !name.is_empty() {
            names.push(name.to_string());
        }
    }
    names
}

/// A host-side bind mount declared by a quadlet's `Volume=` entry and eligible
/// for `purge --data`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct VolumeMount {
    /// Absolute, resolved host path of the bind source.
    source: PathBuf,
    /// Whether the declaring quadlet maps a uid (it has a `User=` entry), which
    /// requires `podman unshare rm -rf` instead of a plain `rm -rf`.
    uid_mapped: bool,
}

/// Host-side bind mounts declared by a quadlet's `Volume=` entries.
///
/// Only sources that resolve to an absolute path **strictly below** `$HOME`
/// qualify; the eligible prefixes are `%h/`, `~/`, `$HOME/` and `${HOME}/`.
/// Named volumes (`<name>.volume`), runtime binds under `%t`, sockets, relative
/// paths and absolute paths outside `$HOME` are dropped. `uid_mapped` is derived
/// from the presence of a `User=` entry in the same quadlet.
fn parse_volume_binds(content: &str) -> Vec<VolumeMount> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    resolve_volume_binds(content, home.as_deref())
}

/// Pure core of [`parse_volume_binds`], with `home` injected so it can be tested
/// without reading (or mutating) the process environment.
fn resolve_volume_binds(content: &str, home: Option<&Path>) -> Vec<VolumeMount> {
    let uid_mapped = parse_ini_value(content, "User=").is_some();
    let mut mounts = Vec::new();

    for line in content.lines() {
        let Some(value) = line.trim_start().strip_prefix("Volume=") else {
            continue;
        };
        let value = value.split('#').next().unwrap_or_default().trim();
        // `source:destination[:options]`: the host path is the first field.
        let source = value.split(':').next().unwrap_or_default().trim();
        if let Some(path) = resolve_bind_source(source, home) {
            mounts.push(VolumeMount {
                source: path,
                uid_mapped,
            });
        }
    }

    mounts
}

/// Resolves one `Volume=` source to an eligible absolute bind path strictly
/// under `$HOME`, or `None` for a named volume, a runtime (`%t`) path, a socket,
/// a relative path, a path outside `$HOME`, or `$HOME` itself.
///
/// The path is normalized lexically (`.` and `..` resolved on components, never
/// against the filesystem) **before** the containment check, and the check
/// requires a strict descendant: `Path::starts_with` alone would accept `$HOME`
/// itself and would let `..` escape the home directory.
fn resolve_bind_source(source: &str, home: Option<&Path>) -> Option<PathBuf> {
    if source.is_empty() {
        return None;
    }
    // Named volumes belong to `clean-volumes`, never to `--data`.
    if source.ends_with(".volume") {
        return None;
    }
    // Sockets (and, by policy, files) are never removed.
    if source.ends_with(".sock") {
        return None;
    }
    // The runtime directory (`%t`) holds the podman socket and other ephemeral
    // state that is not ours to delete: everything under it is off limits.
    if source.starts_with("%t") {
        return None;
    }

    // Only the four documented, unambiguous prefixes are accepted. Requiring the
    // trailing `/` rejects `%h`, `~`, `$HOME`, `${HOME}` and, crucially, `~user`
    // (which is not expanded and must never be turned into `$HOME/user`).
    let rest = source
        .strip_prefix("%h/")
        .or_else(|| source.strip_prefix("~/"))
        .or_else(|| source.strip_prefix("$HOME/"))
        .or_else(|| source.strip_prefix("${HOME}/"))?;
    let candidate = home?.join(rest);

    let home = normalize_path(home?);
    let resolved = normalize_path(&candidate);

    // Strict descendant: never `$HOME` itself, never a sibling reached via `..`.
    (resolved != home && resolved.starts_with(&home)).then_some(resolved)
}

/// Lexically normalizes a path: drops `.` components and resolves `..` against
/// the previous component, without touching the filesystem. A leading root is
/// preserved so an absolute path can never be collapsed into a relative one.
fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// Secrets and images declared by one stack, used by the cross-stack guard.
#[derive(Debug, Default, PartialEq, Eq)]
struct StackResources {
    secrets: Vec<String>,
    images: Vec<String>,
}

/// Kind of a resource tracked by the cross-stack guard.
#[derive(Clone, Copy)]
enum ResourceKind {
    Secret,
    Image,
}

/// Scans every stack directly under `root`, collecting the secrets and images it
/// declares across its quadlet files, keyed by stack directory name.
///
/// A missing or unreadable root yields an empty map; individual failures are
/// reported with `warn!` and skipped, so one bad quadlet cannot hide the rest of
/// the guard.
async fn scan_stack_resources(root: &Path) -> Result<HashMap<String, StackResources>> {
    let mut resources: HashMap<String, StackResources> = HashMap::new();
    let mut stacks = match fs::read_dir(root).await {
        Ok(stacks) => stacks,
        Err(e) => {
            // A silently disabled guard is dangerous: the caller would treat
            // every resource as unshared and delete it.
            warn!(
                "cannot read stacks root {} for the shared-resource guard: {e}",
                root.display()
            );
            return Ok(resources);
        }
    };

    while let Some(entry) = stacks.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let stack = entry.file_name().to_string_lossy().to_string();
        let collected = resources.entry(stack).or_default();

        for dir in stack_quadlet_dirs(&entry.path()) {
            let mut files = match fs::read_dir(&dir).await {
                Ok(files) => files,
                Err(e) => {
                    warn!("cannot read stack directory {}: {e}", dir.display());
                    continue;
                }
            };
            while let Some(file) = files.next_entry().await? {
                let path = file.path();
                let Some(ext) = extension_of_path(&path) else {
                    continue;
                };
                if !QUADLET_EXTENSIONS.contains(&ext) {
                    continue;
                }
                let Ok(content) = fs::read_to_string(&path).await else {
                    continue;
                };
                collected.secrets.extend(parse_secret_names(&content));
                if let Some(image) = parse_image_name(&content) {
                    collected.images.push(image);
                }
            }
        }
    }

    Ok(resources)
}

/// The first (sorted) stack other than `current` that still declares `resource`
/// of the given kind, if any. The caller resolves `current` first: an unresolved
/// current stack never reaches this function (it is fail-closed upstream).
fn other_stack_declaring(
    resources: &HashMap<String, StackResources>,
    current: &str,
    resource: &str,
    kind: ResourceKind,
) -> Option<String> {
    let mut stacks: Vec<&String> = resources.keys().collect();
    stacks.sort();

    for stack in stacks {
        if stack.as_str() == current {
            continue;
        }
        let declared = match kind {
            ResourceKind::Secret => resources[stack].secrets.iter().any(|s| s == resource),
            ResourceKind::Image => resources[stack].images.iter().any(|i| i == resource),
        };
        if declared {
            return Some(stack.clone());
        }
    }

    None
}

async fn daemon_reload(ctx: &AppContext) -> Result<()> {
    if ctx.dry_run {
        println!("DRY-RUN systemctl --user daemon-reload");
        return Ok(());
    }

    run_command(
        "systemctl",
        &["--user", "daemon-reload"],
        false,
        ctx.timeout,
    )
    .await
}

/// Reports whether the systemd user unit is active.
///
/// Recibe la unidad YA formada (p. ej. `voicebox.prod.service`); no aplica sufijos.
/// Like every other bounded external call this spawns with `kill_on_drop` and
/// honours the shared deadline, so a wedged `systemctl` cannot hang the status
/// table forever.
async fn is_active(unit: &str, timeout: Option<Duration>) -> Result<bool> {
    let args = ["--user", "is-active", unit];

    let mut command = Command::new("systemctl");
    command.args(args).kill_on_drop(true);

    let output = await_command(command.output(), timeout, "systemctl", &args).await?;
    Ok(String::from_utf8_lossy(&output.stdout).trim() == "active")
}

/// Recibe la unidad YA formada (p. ej. `voicebox.prod.service`); no aplica sufijos.
async fn systemctl_user(action: &str, unit: &str, timeout: Option<Duration>) -> Result<()> {
    run_command("systemctl", &["--user", action, unit], false, timeout).await
}

/// Maps a quadlet file's stem (its name without the extension) and its
/// extension to the systemd user unit the quadlet generator derives from it.
/// The mapping is explicit and never relies on a "already has a dot" heuristic,
/// so a stem that itself contains dots (`voicebox.prod`) still gets the right
/// suffix: `.container` becomes `<stem>.service`, the generator-suffixed types
/// become `<stem>-<ext>.service`, and raw unit quadlets (`.service`, `.socket`,
/// `.mount`) keep their own name.
fn generated_unit(stem: &str, ext: &str) -> String {
    match ext {
        "container" => format!("{stem}.service"),
        "volume" | "network" | "pod" | "kube" | "image" | "build" | "artifact" => {
            format!("{stem}-{ext}.service")
        }
        _ => format!("{stem}.{ext}"),
    }
}

/// Waits for a spawned child, applying `timeout` when it is set.
///
/// `None` waits without limit. On expiry the error names the command together
/// with its arguments — via `format_command`, so the failing unit or file is
/// part of the message — and the caller's `kill_on_drop` child is dropped, which
/// terminates the direct child. Processes that child may have forked are not
/// reaped.
async fn await_command<T, F>(
    future: F,
    timeout: Option<Duration>,
    cmd: &str,
    args: &[&str],
) -> Result<T>
where
    F: Future<Output = io::Result<T>>,
{
    let Some(limit) = timeout else {
        return future
            .await
            .with_context(|| format!("failed to execute {cmd}"));
    };

    match tokio::time::timeout(limit, future).await {
        Ok(result) => result.with_context(|| format!("failed to execute {cmd}")),
        Err(_) => bail!(
            "command timed out after {}s: {}",
            limit.as_secs(),
            format_command(cmd, args)
        ),
    }
}

async fn run_command(
    cmd: &str,
    args: &[&str],
    allow_failure: bool,
    timeout: Option<Duration>,
) -> Result<()> {
    debug!("Running command: {} {}", cmd, args.join(" "));

    if allow_failure {
        let mut command = Command::new(cmd);
        command.args(args).stderr(Stdio::null()).kill_on_drop(true);

        let status = await_command(command.status(), timeout, cmd, args).await?;

        if !status.success() {
            debug!("Allowed command failure: {}", format_command(cmd, args));
        }

        return Ok(());
    }

    let mut command = Command::new(cmd);
    command.args(args).kill_on_drop(true);

    let output = await_command(command.output(), timeout, cmd, args).await?;

    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }

    if !output.status.success() {
        return Err(command_error(cmd, args, output.status, &output.stderr));
    }

    Ok(())
}

async fn run_command_streaming(cmd: &str, args: &[&str]) -> Result<()> {
    debug!("Running command: {} {}", cmd, args.join(" "));

    let status = Command::new(cmd)
        .args(args)
        .status()
        .await
        .with_context(|| format!("failed to execute {cmd}"))?;

    if !status.success() {
        return Err(anyhow!(
            "command failed: {} (exit status: {})",
            format_command(cmd, args),
            exit_status_display(status)
        ));
    }

    Ok(())
}

fn command_error(cmd: &str, args: &[&str], status: ExitStatus, stderr: &[u8]) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if stderr.is_empty() {
        anyhow!(
            "command failed: {} (exit status: {})",
            format_command(cmd, args),
            exit_status_display(status)
        )
    } else {
        anyhow!(
            "command failed: {} (exit status: {})\nstderr:\n{}",
            format_command(cmd, args),
            exit_status_display(status),
            stderr
        )
    }
}

fn format_command(cmd: &str, args: &[&str]) -> String {
    if args.is_empty() {
        cmd.to_string()
    } else {
        format!("{} {}", cmd, args.join(" "))
    }
}

fn exit_status_display(status: ExitStatus) -> String {
    status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "terminated by signal".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_dot_extension_removes_only_the_last_extension() {
        assert_eq!(strip_dot_extension("voicebox.container"), "voicebox");
        assert_eq!(strip_dot_extension("voicebox"), "voicebox");
        assert_eq!(
            strip_dot_extension("voicebox.prod.container"),
            "voicebox.prod"
        );
    }

    #[test]
    fn container_unit_always_appends_service_suffix() {
        assert_eq!(container_unit("voicebox"), "voicebox.service");
        assert_eq!(container_unit("voicebox.prod"), "voicebox.prod.service");
        assert_eq!(container_unit("a.b.c"), "a.b.c.service");
    }

    #[test]
    fn strip_quadlet_extension_only_strips_known_extensions() {
        assert_eq!(strip_quadlet_extension("voicebox.prod"), "voicebox.prod");
        assert_eq!(
            strip_quadlet_extension("voicebox.prod.container"),
            "voicebox.prod"
        );
        assert_eq!(strip_quadlet_extension("voicebox"), "voicebox");
        assert_eq!(strip_quadlet_extension("stack.volume"), "stack");
    }

    #[test]
    fn generated_unit_maps_each_quadlet_type() {
        assert_eq!(generated_unit("voicebox", "container"), "voicebox.service");
        assert_eq!(
            generated_unit("microbin-microbin", "volume"),
            "microbin-microbin-volume.service"
        );
        assert_eq!(generated_unit("stack", "network"), "stack-network.service");
        assert_eq!(generated_unit("raw", "socket"), "raw.socket");
        // A stem with dots must still get the generated suffix.
        assert_eq!(
            generated_unit("voicebox.prod", "container"),
            "voicebox.prod.service"
        );
        assert_eq!(generated_unit("a.b", "volume"), "a.b-volume.service");
    }

    #[test]
    fn parse_volume_name_reads_non_empty_volume_name() {
        let content = "[Volume]\nVolumeName=voicebox-data\n";
        assert_eq!(parse_volume_name(content).as_deref(), Some("voicebox-data"));
    }

    #[test]
    fn parse_volume_name_ignores_missing_or_empty_names() {
        assert_eq!(parse_volume_name("[Volume]\n"), None);
        assert_eq!(parse_volume_name("VolumeName=\n"), None);
        assert_eq!(parse_volume_name("VolumeName=   \n"), None);
    }

    #[test]
    fn parse_ini_value_tolerates_indentation_and_inline_comments() {
        assert_eq!(
            parse_ini_value("  ContainerName=foo\n", "ContainerName=").as_deref(),
            Some("foo")
        );
        assert_eq!(
            parse_ini_value("ContainerName=foo # the name\n", "ContainerName=").as_deref(),
            Some("foo")
        );
        assert_eq!(
            parse_ini_value("\tImage=example:1 # pinned\n", "Image=").as_deref(),
            Some("example:1")
        );
    }

    #[test]
    fn parse_secret_names_reads_every_line_and_names_with_options() {
        let content = "[Container]\n\
            Secret=plain\n\
            \tSecret=my_api_key,type=env,target=API_KEY\n\
            Secret=other # inline note\n\
            Secret= # empty\n";
        assert_eq!(
            parse_secret_names(content),
            vec!["plain", "my_api_key", "other"]
        );
    }

    #[test]
    fn parse_secret_names_is_empty_when_absent_or_blank() {
        assert!(parse_secret_names("[Container]\nImage=example\n").is_empty());
        assert!(parse_secret_names("Secret=\n").is_empty());
        assert!(parse_secret_names("Secret=   \n").is_empty());
    }

    #[test]
    fn parse_volume_binds_resolves_home_and_detects_uid_mapping() {
        let home = PathBuf::from("/home/tester");
        let content = "[Container]\nUser=1000:1000\nVolume=%h/.config/nav/data:/data:Z\n";
        let mounts = resolve_volume_binds(content, Some(home.as_path()));
        assert_eq!(mounts.len(), 1);
        assert_eq!(
            mounts[0].source,
            PathBuf::from("/home/tester/.config/nav/data")
        );
        assert!(mounts[0].uid_mapped);
    }

    #[test]
    fn parse_volume_binds_without_user_is_not_uid_mapped() {
        let home = PathBuf::from("/home/tester");
        let content = "[Container]\nVolume=%h/.config/nav/data:/data:Z\n";
        let mounts = resolve_volume_binds(content, Some(home.as_path()));
        assert_eq!(mounts.len(), 1);
        assert!(!mounts[0].uid_mapped);
    }

    #[test]
    fn parse_volume_binds_expands_home_token() {
        let home = PathBuf::from("/home/tester");
        let mounts =
            resolve_volume_binds("[Container]\nVolume=${HOME}/x:/x\n", Some(home.as_path()));
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].source, PathBuf::from("/home/tester/x"));
    }

    #[test]
    fn parse_volume_binds_rejects_home_itself() {
        let home = PathBuf::from("/home/tester");
        for source in ["%h", "%h/", "~/", "$HOME", "$HOME/", "${HOME}", "${HOME}/"] {
            let content = format!("[Container]\nVolume={source}:/x\n");
            let mounts = resolve_volume_binds(&content, Some(home.as_path()));
            assert!(
                mounts.is_empty(),
                "{source:?} resolves to $HOME and must be rejected: {mounts:?}"
            );
        }
    }

    #[test]
    fn parse_volume_binds_rejects_paths_escaping_home_via_dotdot() {
        let home = PathBuf::from("/home/tester");
        for source in [
            "%h/../../etc",
            "%h/../..",
            "%h/../other",
            "~/../..",
            "$HOME/../../etc",
        ] {
            let content = format!("[Container]\nVolume={source}:/x\n");
            let mounts = resolve_volume_binds(&content, Some(home.as_path()));
            assert!(
                mounts.is_empty(),
                "{source:?} escapes $HOME and must be rejected: {mounts:?}"
            );
        }
    }

    #[test]
    fn parse_volume_binds_rejects_tilde_user() {
        let home = PathBuf::from("/home/tester");
        let mounts =
            resolve_volume_binds("[Container]\nVolume=~other/x:/x\n", Some(home.as_path()));
        assert!(
            mounts.is_empty(),
            "`~user` is not expanded and must be rejected: {mounts:?}"
        );
    }

    #[test]
    fn parse_volume_binds_normalizes_descendant_paths() {
        let home = PathBuf::from("/home/tester");
        let content = "[Container]\nVolume=%h/.config/nav/../nav/data:/data\n";
        let mounts = resolve_volume_binds(content, Some(home.as_path()));
        assert_eq!(mounts.len(), 1);
        assert_eq!(
            mounts[0].source,
            PathBuf::from("/home/tester/.config/nav/data")
        );
    }

    #[test]
    fn parse_volume_binds_ignores_sockets_named_volumes_system_and_relative_paths() {
        let home = PathBuf::from("/home/tester");
        let content = "[Container]\n\
            Volume=%t/podman/podman.sock:/var/run/docker.sock:ro\n\
            Volume=memos-memos.volume:/var/opt/memos:Z\n\
            Volume=/etc/foo:/etc/foo:ro\n\
            Volume=relative/data:/data\n";
        let mounts = resolve_volume_binds(content, Some(home.as_path()));
        assert!(mounts.is_empty(), "unexpected binds: {mounts:?}");
    }

    #[test]
    fn parse_volume_binds_uses_home_from_env() {
        // The test harness always exports HOME; only read it, never mutate it, so
        // this cannot race with the other (parallel) unit tests.
        let home = PathBuf::from(std::env::var("HOME").expect("HOME must be set"));
        let mounts = parse_volume_binds("[Container]\nVolume=%h/.config/nav/data:/data:Z\n");
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].source, home.join(".config/nav/data"));
    }

    #[test]
    fn parse_menu_selection_accepts_number_first_or_action_first() {
        assert_eq!(parse_menu_selection(&["2"]).unwrap(), (2, None));
        assert_eq!(parse_menu_selection(&["2", "r"]).unwrap(), (2, Some("r")));
        assert_eq!(
            parse_menu_selection(&["restart", "3"]).unwrap(),
            (3, Some("restart"))
        );
        assert!(parse_menu_selection(&["restart"]).is_err());
    }

    #[test]
    fn parse_menu_selection_rejects_zero_index() {
        assert!(parse_menu_selection(&["0"]).is_err());
        assert!(parse_menu_selection(&["0", "r"]).is_err());
        assert!(parse_menu_selection(&["r", "0"]).is_err());
    }

    /// A scratch directory for the `find_quadlet_bin` unit tests. It lives under
    /// the OS temp dir and is removed on drop, so the tests never touch the
    /// host's real generator paths.
    struct QuadletBinTestDir {
        path: PathBuf,
    }

    impl QuadletBinTestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "qctl-quadlet-bin-test-{}-{name}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("failed to create quadlet bin test dir");
            Self { path }
        }

        /// Writes an executable-looking stub at `name` with the given mode and
        /// returns its path.
        fn file(&self, name: &str, mode: u32) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;

            let path = self.path.join(name);
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("failed to write quadlet stub");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                .expect("failed to chmod quadlet stub");
            path
        }

        /// A path inside the scratch dir that is intentionally never created.
        fn missing(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for QuadletBinTestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn find_quadlet_bin_prefers_the_env_override() {
        let dir = QuadletBinTestDir::new("prefers-env");
        let candidate = dir.file("candidate", 0o755);
        let override_bin = dir.file("override", 0o755);

        let found = find_quadlet_bin(&[candidate], Some(override_bin.clone()))
            .expect("the env override must win over the candidates");

        assert_eq!(found, override_bin);
    }

    #[test]
    fn find_quadlet_bin_returns_the_first_existing_executable_candidate() {
        let dir = QuadletBinTestDir::new("first-existing");
        let first = dir.file("first", 0o755);
        let missing = dir.missing("missing");

        let found = find_quadlet_bin(&[first.clone(), missing.clone()], None)
            .expect("the first executable candidate must be returned");
        assert_eq!(found, first);

        let second = dir.file("second", 0o755);
        let found = find_quadlet_bin(&[missing, second.clone()], None)
            .expect("a later executable candidate must be returned");
        assert_eq!(found, second);
    }

    #[test]
    fn find_quadlet_bin_skips_non_executable_candidates() {
        let dir = QuadletBinTestDir::new("skips-non-executable");
        let non_executable = dir.file("plain", 0o644);
        let executable = dir.file("exec", 0o755);

        let found = find_quadlet_bin(&[non_executable, executable.clone()], None)
            .expect("a non-executable candidate must be skipped");

        assert_eq!(found, executable);
    }

    #[test]
    fn find_quadlet_bin_skips_a_directory_candidate() {
        let dir = QuadletBinTestDir::new("skips-directory");
        let as_directory = dir.path.join("dir-candidate");
        std::fs::create_dir_all(&as_directory).expect("failed to create directory candidate");
        let executable = dir.file("exec", 0o755);

        let found = find_quadlet_bin(&[as_directory, executable.clone()], None)
            .expect("a directory candidate must be skipped in favour of the next one");

        assert_eq!(found, executable);
    }

    #[test]
    fn find_quadlet_bin_probes_the_distro_candidates_in_order() {
        // Contract: the usrmerge layout (`/usr/libexec/podman/quadlet`, Fedora /
        // openSUSE) is probed before the Debian/Arch layout
        // (`/usr/lib/podman/quadlet`). `quadlet_candidates()` mirrors that order
        // and `find_quadlet_bin` returns the first match in it.
        assert_eq!(
            QUADLET_BIN_CANDIDATES,
            &["/usr/libexec/podman/quadlet", "/usr/lib/podman/quadlet"][..]
        );
        assert_eq!(
            quadlet_candidates().first(),
            Some(&PathBuf::from("/usr/libexec/podman/quadlet")),
            "the Fedora / usrmerge candidate must be probed first"
        );

        // The probe honours that order: with both candidates present, the first
        // one wins. Real paths cannot be created here, so temp stubs mirror them.
        let dir = QuadletBinTestDir::new("candidate-order");
        let first = dir.file("usr-libexec-podman-quadlet", 0o755);
        let second = dir.file("usr-lib-podman-quadlet", 0o755);
        let found = find_quadlet_bin(&[first.clone(), second], None)
            .expect("the first declared candidate must win when both exist");

        assert_eq!(
            found, first,
            "the first of QUADLET_BIN_CANDIDATES must win when both exist"
        );
    }

    #[test]
    fn find_quadlet_bin_ignores_a_missing_env_override() {
        let dir = QuadletBinTestDir::new("missing-env-override");
        let missing_override = dir.missing("no-such-override");
        let candidate = dir.file("candidate", 0o755);

        let found = find_quadlet_bin(std::slice::from_ref(&candidate), Some(missing_override))
            .expect("a broken env override must fall back to the candidates");

        assert_eq!(found, candidate);
    }

    #[test]
    fn find_quadlet_bin_fails_with_probed_paths_when_nothing_matches() {
        let dir = QuadletBinTestDir::new("nothing-matches");
        let x = dir.missing("x");
        let y = dir.missing("y");

        let err = find_quadlet_bin(&[x.clone(), y.clone()], None)
            .expect_err("no matching candidate must yield an error");
        let message = err.to_string();

        assert!(
            message.contains(&x.to_string_lossy().to_string()),
            "the error must list the probed path {x:?}: {message}"
        );
        assert!(
            message.contains(&y.to_string_lossy().to_string()),
            "the error must list the probed path {y:?}: {message}"
        );
        assert!(
            message.contains("QCTL_QUADLET_BIN"),
            "the error must mention the QCTL_QUADLET_BIN override: {message}"
        );
    }
}

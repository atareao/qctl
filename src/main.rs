use std::collections::HashMap;
use std::future::Future;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};
use tokio::fs;
use tokio::process::Command;
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use tracing_subscriber::EnvFilter;

const QUADLET_EXTENSIONS: &[&str] = &[
    "container",
    "network",
    "volume",
    "service",
    "socket",
    "mount",
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
        /// Compact output format for scripts
        #[arg(long)]
        compact: bool,
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
    /// Check one quadlet file with /usr/lib/podman/quadlet
    Check {
        /// Quadlet file path
        quadlet: String,
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

    let ctx = AppContext::new(cli.dry_run, cli.timeout)?;

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
        Commands::Status { service, compact } => {
            status(&ctx, service, compact, TargetScope::Wide).await?
        }
        Commands::Menu { service } => menu(&ctx, service).await?,
        Commands::CleanVolumes { yes } => clean_volumes(&ctx, yes).await?,
        Commands::Check { quadlet } => check_quadlet(&ctx, quadlet).await?,
        Commands::Logs { service } => logs(service, false).await?,
        Commands::Logsf { service } => logs(service, true).await?,
        Commands::Reload => daemon_reload(&ctx).await?,
        Commands::Completions { .. } => unreachable!(),
    }

    Ok(())
}

impl AppContext {
    fn new(dry_run: bool, timeout_secs: u64) -> Result<Self> {
        let root_dir = std::env::current_dir().context("failed to resolve current directory")?;
        let source_dirs = discover_source_dirs(&root_dir);

        let home = std::env::var("HOME").context("HOME is not set")?;
        let target_dir = Path::new(&home).join(".config/containers/systemd");

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

        if ext == "container" {
            if ctx.dry_run {
                println!("DRY-RUN systemctl --user stop {}", systemd_unit(&stem));
            } else {
                match systemctl_user("stop", &stem, ctx.timeout).await {
                    Ok(()) => debug!("Stopped {stem} (if running)"),
                    Err(err) => warn!("failed to stop {stem} (continuing): {err:#}"),
                }
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
            println!("DRY-RUN systemctl --user start {}", systemd_unit(&unit));
            continue;
        }

        if !unit_installed_in_target_dir(ctx, &unit).await? {
            debug!("Skipping {unit} (not linked in target dir)");
            continue;
        }

        println!("Starting {unit}");
        let unit_for_task = unit.clone();
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
            println!("DRY-RUN systemctl --user stop {}", systemd_unit(&unit));
            continue;
        }

        let unit_file = ctx.target_dir.join(format!("{unit}.container"));
        if !path_exists(&unit_file).await? {
            debug!("Skipping {unit} (not linked in target dir)");
            continue;
        }

        let unit_for_task = unit.clone();
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
            println!("DRY-RUN systemctl --user restart {}", systemd_unit(&unit));
            continue;
        }

        let unit_file = ctx.target_dir.join(format!("{unit}.container"));
        if !path_exists(&unit_file).await? {
            debug!("Skipping {unit} (not linked in target dir)");
            continue;
        }

        let unit_for_task = unit.clone();
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
            if unit != strip_dot_extension(svc) {
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
        println!("No container units found in {}", ctx.source_display());
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
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec!["#", "Link", "State", "Unit"]);
    }

    let mut linked_count = 0;
    let mut running_count = 0;
    let mut stopped_count = 0;
    let mut missing_count = 0;

    for (index, unit) in targets.iter().enumerate() {
        let linked = path_exists(&ctx.target_dir.join(format!("{unit}.container"))).await?;
        let running = if linked {
            match is_active(unit, ctx.timeout).await {
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

        let link_color = if linked { Color::Green } else { Color::Red };
        let state_color = if running {
            Color::Green
        } else if linked {
            Color::Yellow
        } else {
            Color::DarkGrey
        };

        if compact {
            println!("{}\t{}\t{}", unit, link_txt, state_txt);
        } else {
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
            match is_active(&unit, ctx.timeout).await {
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
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
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

async fn check_quadlet(ctx: &AppContext, quadlet: String) -> Result<()> {
    debug!("Checking {quadlet}");
    run_command(
        "/usr/lib/podman/quadlet",
        &["-dryrun", "-user", &ctx.user, &quadlet],
        false,
        ctx.timeout,
    )
    .await?;
    Ok(())
}

async fn logs(service: String, extra: bool) -> Result<()> {
    let unit = systemd_unit(&service);
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
    let unit = systemd_unit(service);
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
        let unit = strip_dot_extension(svc);
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
        return Ok(vec![strip_dot_extension(&svc)]);
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

fn strip_dot_extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .to_string()
}

fn parse_volume_name(content: &str) -> Option<String> {
    content
        .lines()
        .find_map(|line| line.strip_prefix("VolumeName="))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn parse_image_name(content: &str) -> Option<String> {
    content
        .lines()
        .find_map(|line| line.strip_prefix("Image="))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
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
/// Like every other bounded external call this spawns with `kill_on_drop` and
/// honours the shared deadline, so a wedged `systemctl` cannot hang the status
/// table forever.
async fn is_active(unit: &str, timeout: Option<Duration>) -> Result<bool> {
    let unit = systemd_unit(unit);
    let args = ["--user", "is-active", unit.as_str()];

    let mut command = Command::new("systemctl");
    command.args(args).kill_on_drop(true);

    let output = await_command(command.output(), timeout, "systemctl", &args).await?;
    Ok(String::from_utf8_lossy(&output.stdout).trim() == "active")
}

async fn systemctl_user(action: &str, unit: &str, timeout: Option<Duration>) -> Result<()> {
    let unit = systemd_unit(unit);
    run_command("systemctl", &["--user", action, &unit], false, timeout).await
}

fn systemd_unit(unit: &str) -> String {
    if unit.contains('.') {
        unit.to_string()
    } else {
        format!("{unit}.service")
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
    fn systemd_unit_adds_service_suffix_only_for_plain_names() {
        assert_eq!(systemd_unit("voicebox"), "voicebox.service");
        assert_eq!(systemd_unit("voicebox.service"), "voicebox.service");
        assert_eq!(systemd_unit("voicebox.container"), "voicebox.container");
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
}

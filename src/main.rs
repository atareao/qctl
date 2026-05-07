use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};
use tokio::fs;
use tokio::process::Command;
use tracing::debug;
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
#[command(name = "qctl")]
#[command(about = "Manage quadlets", long_about = None)]
struct Cli {
    /// Print what would be done without changing files or calling external tools
    #[arg(long, global = true)]
    dry_run: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create symlinks and reload user systemd
    Install,
    /// Stop services, remove symlinks and reload user systemd
    Uninstall,
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
}

struct AppContext {
    source_dirs: Vec<PathBuf>,
    target_dir: PathBuf,
    user: String,
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .without_time()
        .init();

    let cli = Cli::parse();
    let ctx = AppContext::new(cli.dry_run)?;

    match cli.command {
        Commands::Install => {
            install(&ctx).await?;
            status(&ctx, None, false).await?;
        }
        Commands::Uninstall => {
            uninstall(&ctx).await?;
            status(&ctx, None, false).await?;
        }
        Commands::Start { service } => {
            start(&ctx, service.clone()).await?;
            status(&ctx, service, false).await?;
        }
        Commands::Stop { service } => {
            stop(&ctx, service.clone()).await?;
            status(&ctx, service, false).await?;
        }
        Commands::Restart { service } => {
            restart(&ctx, service.clone()).await?;
            status(&ctx, service, false).await?;
        }
        Commands::Status { service, compact } => status(&ctx, service, compact).await?,
        Commands::Menu { service } => menu(&ctx, service).await?,
        Commands::CleanVolumes { yes } => clean_volumes(&ctx, yes).await?,
        Commands::Check { quadlet } => check_quadlet(&ctx, quadlet).await?,
        Commands::Logs { service } => logs(service, false).await?,
        Commands::Logsf { service } => logs(service, true).await?,
    }

    Ok(())
}

impl AppContext {
    fn new(dry_run: bool) -> Result<Self> {
        let root_dir = std::env::current_dir().context("failed to resolve current directory")?;
        let source_dirs = discover_source_dirs(&root_dir);

        let home = std::env::var("HOME").context("HOME is not set")?;
        let target_dir = Path::new(&home).join(".config/containers/systemd");

        let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());

        Ok(Self {
            source_dirs,
            target_dir,
            user,
            dry_run,
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

    let files = collect_quadlets(&ctx.source_dirs).await?;
    for src in files {
        let name = basename(&src)?;
        let dest = ctx.target_dir.join(&name);
        if ctx.dry_run {
            println!("DRY-RUN link {} -> {}", src.display(), dest.display());
        } else {
            link_or_replace(&src, &dest).await?;
        }
        debug!("Linked {} -> {}", src.display(), dest.display());
    }

    daemon_reload(ctx).await?;
    debug!("Install complete");
    Ok(())
}

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
        if !path_exists(&target).await? {
            continue;
        }

        if ext == "container" {
            if ctx.dry_run {
                println!("DRY-RUN systemctl --user stop {}", systemd_unit(&stem));
            } else {
                let _ = systemctl_user("stop", &stem).await;
            }
            debug!("Stopped {stem} (if running)");
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

async fn start(ctx: &AppContext, service: Option<String>) -> Result<()> {
    let targets = resolve_targets(ctx, service).await?;
    if ctx.dry_run {
        install_missing_targets(ctx, &targets).await?;
    } else {
        ensure_targets_installed(ctx, &targets).await?;
    }

    for unit in targets {
        if ctx.dry_run {
            println!("DRY-RUN systemctl --user start {}", systemd_unit(&unit));
            continue;
        }

        let unit_file = ctx.target_dir.join(format!("{unit}.container"));
        if !path_exists(&unit_file).await? {
            debug!("Skipping {unit} (not linked in target dir)");
            continue;
        }

        systemctl_user("start", &unit).await?;
        debug!("Started {unit}");
    }

    Ok(())
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
    let targets = resolve_targets(ctx, service).await?;
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

        systemctl_user("stop", &unit).await?;
        debug!("Stopped {unit}");
    }

    Ok(())
}

async fn restart(ctx: &AppContext, service: Option<String>) -> Result<()> {
    let targets = resolve_targets(ctx, service).await?;
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

        systemctl_user("restart", &unit).await?;
        debug!("Restarted {unit}");
    }

    Ok(())
}

async fn status(ctx: &AppContext, service: Option<String>, compact: bool) -> Result<()> {
    let targets = resolve_targets(ctx, service).await?;

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
            is_active(unit).await.unwrap_or(false)
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
        io::stdin()
            .read_line(&mut input)
            .context("failed to read selection")?;
        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        if matches!(input, "q" | "quit" | "exit") {
            break;
        }

        let parts = input.split_whitespace().collect::<Vec<_>>();
        let (index, action) = parse_menu_selection(&parts)?;
        let Some(row) = rows.get(index.saturating_sub(1)) else {
            println!("Invalid selection: {index}");
            wait_for_enter()?;
            continue;
        };

        let action = action.unwrap_or(if row.running { "p" } else { "s" });
        match action {
            "s" | "start" => start(ctx, Some(row.unit.clone())).await?,
            "p" | "stop" => stop(ctx, Some(row.unit.clone())).await?,
            "r" | "restart" => restart(ctx, Some(row.unit.clone())).await?,
            "l" | "logs" => {
                print!("\x1B[2J\x1B[H");
                println!("Recent logs for {}", row.unit);
                println!();
                recent_logs(&row.unit).await?;
                println!();
                wait_for_enter()?;
            }
            other => {
                println!("Unknown action: {other}");
                wait_for_enter()?;
                continue;
            }
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
    let targets = resolve_targets(ctx, service).await?;
    let mut rows = Vec::with_capacity(targets.len());

    for unit in targets {
        let linked = path_exists(&ctx.target_dir.join(format!("{unit}.container"))).await?;
        let running = if linked {
            is_active(&unit).await.unwrap_or(false)
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
        return Ok((index, parts.get(1).copied()));
    }

    let Some(index) = parts.get(1).and_then(|value| value.parse::<usize>().ok()) else {
        return Err(anyhow!("use: <number> [action] or <action> <number>"));
    };
    Ok((index, Some(parts[0])))
}

fn wait_for_enter() -> Result<()> {
    print!("Press Enter to continue...");
    io::stdout().flush().context("failed to flush stdout")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to wait for enter")?;
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
        run_command("podman", &["volume", "rm", "-f", &volume_name], false).await?;
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
    )
    .await?;
    Ok(())
}

async fn logs(service: String, extra: bool) -> Result<()> {
    let unit = systemd_unit(&service);
    if extra {
        run_command_streaming("journalctl", &["--user", "-xsfu", &unit]).await?;
    } else {
        run_command_streaming(
            "journalctl",
            &["--user", "-u", &unit, "-f", "--since", "1 hour ago"],
        )
        .await?;
    }

    Ok(())
}

async fn recent_logs(service: &str) -> Result<()> {
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

async fn resolve_targets(ctx: &AppContext, service: Option<String>) -> Result<Vec<String>> {
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

    if path_exists(&ctx.target_dir).await? {
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

async fn collect_quadlets(source_dirs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut seen = HashMap::new();

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
            if let Some(previous) = seen.insert(name.clone(), path.clone()) {
                return Err(anyhow!(
                    "duplicate quadlet file name '{}' found in {} and {}",
                    name,
                    previous.display(),
                    path.display()
                ));
            }

            out.push(path);
        }
    }

    out.sort();
    Ok(out)
}

async fn link_or_replace(src: &Path, dest: &Path) -> Result<()> {
    if path_exists(dest).await? {
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

    Ok(())
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

async fn daemon_reload(ctx: &AppContext) -> Result<()> {
    if ctx.dry_run {
        println!("DRY-RUN systemctl --user daemon-reload");
        return Ok(());
    }

    run_command("systemctl", &["--user", "daemon-reload"], false).await
}

async fn is_active(unit: &str) -> Result<bool> {
    let unit = systemd_unit(unit);
    let output = Command::new("systemctl")
        .args(["--user", "is-active", &unit])
        .output()
        .await
        .context("failed to execute systemctl")?;
    Ok(String::from_utf8_lossy(&output.stdout).trim() == "active")
}

async fn systemctl_user(action: &str, unit: &str) -> Result<()> {
    let unit = systemd_unit(unit);
    run_command("systemctl", &["--user", action, &unit], false).await
}

fn systemd_unit(unit: &str) -> String {
    if unit.contains('.') {
        unit.to_string()
    } else {
        format!("{unit}.service")
    }
}

async fn run_command(cmd: &str, args: &[&str], allow_failure: bool) -> Result<()> {
    debug!("Running command: {} {}", cmd, args.join(" "));

    if allow_failure {
        let status = Command::new(cmd)
            .args(args)
            .stderr(Stdio::null())
            .status()
            .await
            .with_context(|| format!("failed to execute {cmd}"))?;

        if !status.success() {
            debug!("Allowed command failure: {}", format_command(cmd, args));
        }

        return Ok(());
    }

    let output = Command::new(cmd)
        .args(args)
        .output()
        .await
        .with_context(|| format!("failed to execute {cmd}"))?;

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
}

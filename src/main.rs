use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};
use tokio::fs;
use tokio::process::Command;

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
    /// Show status for all container units or one specific service
    Status {
        /// Service name, with or without .container extension
        service: Option<String>,
        /// Compact output format for scripts
        #[arg(long)]
        compact: bool,
    },
    /// Remove podman volumes declared in .volume files
    CleanVolumes,
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
    source_dir: PathBuf,
    target_dir: PathBuf,
    user: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let ctx = AppContext::new()?;

    match cli.command {
        Commands::Install => install(&ctx).await?,
        Commands::Uninstall => uninstall(&ctx).await?,
        Commands::Start { service } => start(&ctx, service).await?,
        Commands::Stop { service } => stop(&ctx, service).await?,
        Commands::Status { service, compact } => status(&ctx, service, compact).await?,
        Commands::CleanVolumes => clean_volumes(&ctx).await?,
        Commands::Check { quadlet } => check_quadlet(&ctx, quadlet).await?,
        Commands::Logs { service } => logs(service, false).await?,
        Commands::Logsf { service } => logs(service, true).await?,
    }

    Ok(())
}

impl AppContext {
    fn new() -> Result<Self> {
        let root_dir = std::env::current_dir().context("failed to resolve current directory")?;
        let source_dir = root_dir.join("quadlets");

        let home = std::env::var("HOME").context("HOME is not set")?;
        let target_dir = Path::new(&home).join(".config/containers/systemd");

        let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());

        Ok(Self {
            source_dir,
            target_dir,
            user,
        })
    }
}

async fn install(ctx: &AppContext) -> Result<()> {
    println!(
        "Installing quadlets from {} into {}",
        ctx.source_dir.display(),
        ctx.target_dir.display()
    );

    fs::create_dir_all(&ctx.target_dir)
        .await
        .with_context(|| format!("failed to create {}", ctx.target_dir.display()))?;

    let files = collect_quadlets(&ctx.source_dir).await?;
    for src in files {
        let name = basename(&src)?;
        let dest = ctx.target_dir.join(&name);
        link_or_replace(&src, &dest).await?;
        println!("Linked {} -> {}", src.display(), dest.display());
    }

    daemon_reload().await?;
    println!("Install complete");
    Ok(())
}

async fn uninstall(ctx: &AppContext) -> Result<()> {
    println!(
        "Stopping services and removing symlinks from {}",
        ctx.target_dir.display()
    );

    let files = collect_quadlets(&ctx.source_dir).await?;
    for src in files {
        let name = basename(&src)?;
        let stem = strip_dot_extension(&name);
        let ext = extension_of(&name);

        let target = ctx.target_dir.join(&name);
        if !path_exists(&target).await? {
            continue;
        }

        if ext == "container" {
            let _ = systemctl_user("stop", &stem).await;
            println!("Stopped {stem} (if running)");
        }

        fs::remove_file(&target)
            .await
            .with_context(|| format!("failed to remove {}", target.display()))?;
        println!("Removed {}", target.display());
    }

    daemon_reload().await?;
    println!("Uninstall complete");
    Ok(())
}

async fn start(ctx: &AppContext, service: Option<String>) -> Result<()> {
    let targets = resolve_targets(&ctx.source_dir, service).await?;
    for unit in targets {
        let unit_file = ctx.target_dir.join(format!("{unit}.container"));
        if !path_exists(&unit_file).await? {
            println!("Skipping {unit} (not linked in target dir)");
            continue;
        }

        let _ = systemctl_user("start", &unit).await;
        println!("Started {unit}");
    }

    Ok(())
}

async fn stop(ctx: &AppContext, service: Option<String>) -> Result<()> {
    let targets = resolve_targets(&ctx.source_dir, service).await?;
    for unit in targets {
        let unit_file = ctx.target_dir.join(format!("{unit}.container"));
        if !path_exists(&unit_file).await? {
            println!("Skipping {unit} (not linked in target dir)");
            continue;
        }

        let _ = systemctl_user("stop", &unit).await;
        println!("Stopped {unit}");
    }

    Ok(())
}

async fn status(ctx: &AppContext, service: Option<String>, compact: bool) -> Result<()> {
    let targets = resolve_targets(&ctx.source_dir, service).await?;

    if targets.is_empty() {
        println!("No container units found in {}", ctx.source_dir.display());
        return Ok(());
    }

    if !compact {
        println!("Source: {}", ctx.source_dir.display());
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
            is_active(&unit).await.unwrap_or(false)
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

    if !compact {
        println!("{table}");
    }

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

async fn clean_volumes(ctx: &AppContext) -> Result<()> {
    println!("Removing podman volumes declared in .volume files");

    let mut entries = fs::read_dir(&ctx.source_dir)
        .await
        .with_context(|| format!("failed to read {}", ctx.source_dir.display()))?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if extension_of_path(&path) != Some("volume") {
            continue;
        }

        let content = fs::read_to_string(&path)
            .await
            .with_context(|| format!("failed to read {}", path.display()))?;

        let Some(volume_name) = parse_volume_name(&content) else {
            println!("Skipping {} (VolumeName not found)", path.display());
            continue;
        };

        let _ = run_command("podman", &["volume", "rm", "-f", &volume_name], true).await;
        println!("Removed volume: {volume_name}");
    }

    Ok(())
}

async fn check_quadlet(ctx: &AppContext, quadlet: String) -> Result<()> {
    println!("Checking {quadlet}");
    run_command(
        "/usr/lib/podman/quadlet",
        &["-dryrun", "-user", &ctx.user, &quadlet],
        false,
    )
    .await?;
    Ok(())
}

async fn logs(service: String, extra: bool) -> Result<()> {
    if extra {
        run_command("journalctl", &["--user", "-xsfu", &service], false).await?;
    } else {
        run_command(
            "journalctl",
            &["--user", "-u", &service, "-f", "--since", "1 hour ago"],
            false,
        )
        .await?;
    }

    Ok(())
}

async fn resolve_targets(source_dir: &Path, service: Option<String>) -> Result<Vec<String>> {
    if let Some(svc) = service {
        return Ok(vec![strip_dot_extension(&svc)]);
    }

    let mut out = Vec::new();
    let mut entries = fs::read_dir(source_dir)
        .await
        .with_context(|| format!("failed to read {}", source_dir.display()))?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if extension_of_path(&path) == Some("container") {
            out.push(strip_dot_extension(&basename(&path)?));
        }
    }

    out.sort();
    out.dedup();
    Ok(out)
}

async fn collect_quadlets(source_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = fs::read_dir(source_dir)
        .await
        .with_context(|| format!("failed to read {}", source_dir.display()))?;

    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let Some(ext) = extension_of_path(&path) else {
            continue;
        };
        if QUADLET_EXTENSIONS.contains(&ext) {
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
    name.split('.').next().unwrap_or(name).to_string()
}

fn parse_volume_name(content: &str) -> Option<String> {
    content
        .lines()
        .find_map(|line| line.strip_prefix("VolumeName="))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

async fn daemon_reload() -> Result<()> {
    run_command("systemctl", &["--user", "daemon-reload"], false).await
}

async fn is_active(unit: &str) -> Result<bool> {
    let status = Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", unit])
        .status()
        .await
        .context("failed to execute systemctl")?;
    Ok(status.success())
}

async fn systemctl_user(action: &str, unit: &str) -> Result<()> {
    run_command("systemctl", &["--user", action, unit], true).await
}

async fn run_command(cmd: &str, args: &[&str], allow_failure: bool) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .await
        .with_context(|| format!("failed to execute {cmd}"))?;

    if !status.success() && !allow_failure {
        return Err(anyhow!("command failed: {} {}", cmd, args.join(" ")));
    }

    Ok(())
}

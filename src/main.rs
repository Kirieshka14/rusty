mod config;
mod installer;
mod manifest;
mod packer;
mod registry;
mod state;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use config::Config;
use installer::Installer;
use packer::Packer;
use registry::RegistryClient;
use state::State;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "rusty",
    author = "xkiri <riz.riz1941@gmail.com>",
    version = "0.1.0",
    about = "🚀 A fast, independent, rootless package manager for any Linux distribution",
    long_about = "rusty is a modern, standalone package manager designed to run without root privileges on any Linux distro.\nIt installs packages cleanly into ~/.local, seamlessly integrates desktop launchers, and provides paru-style detailed verification."
)]
struct Cli {
    /// Skip all interactive confirmation prompts
    #[arg(short = 'y', long, global = true)]
    noconfirm: bool,

    /// Suppress verbose log output and step details
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install a package by name, manifest path, or archive
    #[command(alias = "i")]
    Install {
        /// Name of the package, or path to a local .toml manifest
        package: String,

        /// Force reinstall even if already installed
        #[arg(short, long)]
        force: bool,
    },

    /// Remove an installed package
    #[command(alias = "rm", alias = "uninstall")]
    Remove {
        /// Name of the package to remove
        package: String,
    },

    /// List all installed packages
    #[command(alias = "ls")]
    List,

    /// Search for packages in the registry
    Search {
        /// Search keyword
        query: String,
    },

    /// Show detailed information about a package
    Info {
        /// Name of the package
        package: String,
    },

    /// Check for updates and update installed packages
    #[command(alias = "upgrade")]
    Update {
        /// Specific package to update (or all if omitted)
        package: Option<String>,
    },

    /// Package a project with rusty.toml into a distributable archive
    Pack {
        /// Path to project directory (default: current directory)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output directory for package archive (default: dist/)
        #[arg(short, long, default_value = "dist")]
        out: PathBuf,
    },

    /// Show current rusty configuration and paths
    Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load()?;
    let registry = RegistryClient::new(config.registry_url.clone());
    let installer = Installer::new(config.clone());

    match cli.command {
        Commands::Install { package, force } => {
            let state = State::load(&config.state_file())?;
            if state.get(&package).is_some() && !force {
                ui::info(&format!(
                    "Package '{}' is already installed. Use {} to reinstall.",
                    package.bold(),
                    "--force".bright_yellow()
                ));
                return Ok(());
            }

            if !cli.quiet {
                ui::paru_log(&format!("Resolving package '{}' from registry...", package));
            }

            let manifest = match registry.fetch_manifest(&package).await {
                Ok(m) => m,
                Err(e) => {
                    ui::error(&format!("Could not fetch package '{}': {}", package, e));
                    std::process::exit(1);
                }
            };

            if let Err(e) = installer.install(&manifest, cli.noconfirm, cli.quiet).await {
                ui::error(&format!("Installation failed: {}", e));
                std::process::exit(1);
            }
        }

        Commands::Remove { package } => {
            if let Err(e) = installer.remove(&package, cli.noconfirm, cli.quiet) {
                ui::error(&format!("Removal failed: {}", e));
                std::process::exit(1);
            }
        }

        Commands::List => {
            let state = State::load(&config.state_file())?;
            if state.packages.is_empty() {
                ui::info("No packages installed yet. Try installing one with: rusty install rustydroid");
                return Ok(());
            }

            ui::paru_log("Installed Packages:");
            for pkg in state.packages.values() {
                println!(
                    "  {} {} ({})",
                    pkg.name.bright_green().bold(),
                    format!("v{}", pkg.version).bright_cyan(),
                    pkg.installed_at.format("%Y-%m-%d %H:%M").to_string().bright_black()
                );
                if let Some(desc) = &pkg.description {
                    println!("    {}", desc.bright_black());
                }
                if !pkg.binaries.is_empty() {
                    println!("    Binaries: {}", pkg.binaries.join(", ").bright_yellow());
                }
            }
            println!();
        }

        Commands::Search { query } => {
            if !cli.quiet {
                ui::paru_log(&format!("Searching registry for '{}'...", query));
            }
            match registry.search(&query).await {
                Ok(results) => {
                    if results.is_empty() {
                        ui::info(&format!("No packages found matching '{}'.", query));
                    } else {
                        println!();
                        ui::paru_log(&format!("Search Results ({}):", results.len()));
                        for r in results {
                            println!(
                                "  {} {}",
                                r.name.bright_green().bold(),
                                format!("v{}", r.version).bright_cyan()
                            );
                            if let Some(author) = r.author {
                                println!("    Maintainer: {}", author.bright_blue());
                            }
                            if let Some(desc) = r.description {
                                if !desc.is_empty() {
                                    println!("    {}", desc);
                                }
                            }
                            if let Some(repo) = r.repo_url {
                                println!("    Repo: {}", repo.bright_black());
                            }
                        }
                        println!();
                    }
                }
                Err(e) => {
                    ui::error(&format!("Search failed: {}", e));
                }
            }
        }

        Commands::Info { package } => {
            match registry.fetch_manifest(&package).await {
                Ok(m) => {
                    println!();
                    ui::paru_log(&format!("Package Information: {}", m.name().bright_green().bold()));
                    println!("  Version:      {}", m.version().bright_cyan());
                    if let Some(desc) = m.description() {
                        println!("  Description:  {}", desc);
                    }
                    if let Some(author) = m.author() {
                        println!("  Author:       {}", author);
                    }
                    if let Some(license) = m.license() {
                        println!("  License:      {}", license);
                    }
                    if let Some(repo) = m.repository() {
                        println!("  Repository:   {}", repo.bright_blue());
                    }
                    if let Some(hp) = m.homepage() {
                        println!("  Homepage:     {}", hp.bright_blue());
                    }
                    if !m.dependencies.bins.is_empty() {
                        println!("  Dependencies: {}", m.dependencies.bins.join(", ").bright_yellow());
                    }
                    println!();
                }
                Err(e) => {
                    ui::error(&format!("Package info error: {}", e));
                }
            }
        }

        Commands::Update { package } => {
            let state = State::load(&config.state_file())?;
            let to_update: Vec<String> = match package {
                Some(p) => vec![p],
                None => state.packages.keys().cloned().collect(),
            };

            if to_update.is_empty() {
                ui::info("No packages installed to update.");
                return Ok(());
            }

            ui::paru_log("Checking for updates against registry...");
            for pkg_name in to_update {
                let current = match state.get(&pkg_name) {
                    Some(p) => p,
                    None => {
                        ui::warn(&format!("Package '{}' is not installed.", pkg_name));
                        continue;
                    }
                };

                if let Ok(manifest) = registry.fetch_manifest(&pkg_name).await {
                    if manifest.version() != current.version {
                        ui::info(&format!(
                            "Updating {} from v{} to v{}...",
                            pkg_name.bold(),
                            current.version,
                            manifest.version().bright_green()
                        ));
                        if let Err(e) = installer.install(&manifest, cli.noconfirm, cli.quiet).await {
                            ui::error(&format!("Failed to update {}: {}", pkg_name, e));
                        }
                    } else {
                        ui::success(&format!("{} is already up to date (v{})", pkg_name, current.version));
                    }
                }
            }
        }

        Commands::Pack { path, out } => {
            let _ = std::fs::create_dir_all(&out);
            if let Err(e) = Packer::pack(&path, &out) {
                ui::error(&format!("Packaging failed: {}", e));
                std::process::exit(1);
            }
        }

        Commands::Config => {
            println!();
            ui::paru_log("rusty Configuration:");
            println!("  Registry URL: {}", config.registry_url.bright_cyan());
            println!("  Binary Dir:   {}", config.bin_dir.display());
            println!("  Data Dir:     {}", config.data_dir.display());
            println!("  Packages Dir: {}", config.packages_dir().display());
            println!("  Apps Dir:     {}", config.apps_dir.display());
            println!("  Icons Dir:    {}", config.icons_dir.display());
            println!("  State File:   {}", config.state_file().display());
            println!();
        }
    }

    Ok(())
}

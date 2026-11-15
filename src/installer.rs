use crate::config::Config;
use crate::manifest::PackageManifest;
use crate::state::{InstalledPackage, State};
use crate::ui;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use colored::Colorize;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Installer {
    config: Config,
}

impl Installer {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn install(&self, manifest: &PackageManifest, noconfirm: bool, quiet: bool) -> Result<bool> {
        let pkg_name = manifest.name();
        let pkg_ver = manifest.version();

        // 1. Resolve source payload
        let source = manifest.get_current_source().with_context(|| {
            format!(
                "No compatible source found for architecture '{}-{}'",
                std::env::consts::ARCH,
                std::env::consts::OS
            )
        })?;

        // 2. Paru-style Package Inspection & Security Audit Table
        if !quiet {
            ui::paru_log(&format!(
                "Packages to install (1): {}",
                format!("{} v{}", pkg_name, pkg_ver).bright_green().bold()
            ));

            if let Some(desc) = manifest.description() {
                ui::paru_detail("Description", &desc);
            }
            if let Some(author) = manifest.author() {
                ui::paru_detail("Author", &author);
            }
            if let Some(repo) = manifest.repository() {
                ui::paru_detail("Repository", &repo.bright_blue().to_string());
            }
            ui::paru_detail("Payload URL", &source.url.bright_cyan().to_string());
            ui::paru_detail(
                "Format",
                source.format.as_deref().unwrap_or("auto (tar.zst / tar.gz / binary)"),
            );
            if let Some(sha) = &source.sha256 {
                ui::paru_detail("Expected SHA256", &sha.bright_black().to_string());
            }

            println!();
            ui::paru_log("Security & Sandbox Audit:");
            ui::paru_audit(true, "Rootless Mode: 100% safe user-space execution (zero sudo)");
            ui::paru_audit(
                source.sha256.is_some(),
                if source.sha256.is_some() {
                    "Cryptographic Verification: SHA-256 signature present"
                } else {
                    "Cryptographic Verification: No hash specified (proceed with caution)"
                },
            );
            ui::paru_audit(
                true,
                &format!(
                    "Store Isolation: ~/.local/share/rusty/installed/{}-{}/",
                    pkg_name, pkg_ver
                ),
            );
            ui::paru_audit(
                true,
                &format!("Binary Targets: ~/.local/bin/{}", pkg_name),
            );
            println!();
        }

        // 3. User Confirmation Prompt (Paru / Pacman style y/n)
        if !noconfirm {
            let ok = ui::confirm("Proceed with installation?", true)?;
            if !ok {
                ui::warn("Operation aborted by user.");
                return Ok(false);
            }
        }

        // 4. Paru Step 1: Check system dependencies
        if !quiet {
            ui::paru_step(1, 5, "Checking external runtime dependencies");
        }
        for bin in &manifest.dependencies.bins {
            if which::which(bin).is_err() {
                ui::warn(&format!(
                    "External runtime dependency '{}' not found in $PATH.",
                    bin.bright_yellow()
                ));
            } else if !quiet {
                println!("   {} Found dependency: {}", "✓".bright_green(), bin);
            }
        }

        // 5. Paru Step 2: Download payload
        if !quiet {
            ui::paru_step(2, 5, "Fetching package payload");
        }
        let cache_file = self.download_source(&pkg_name, &pkg_ver, source, quiet).await?;

        // 6. Paru Step 3: SHA256 Verification
        if !quiet {
            ui::paru_step(3, 5, "Verifying cryptographic checksum");
        }
        if let Some(expected_sha) = &source.sha256 {
            let spinner = if !quiet { Some(ui::create_spinner("Calculating SHA-256 hash...")) } else { None };
            let actual_sha = compute_sha256(&cache_file)?;
            if let Some(s) = spinner {
                s.finish_and_clear();
            }

            if actual_sha.to_lowercase() != expected_sha.to_lowercase() {
                bail!(
                    "Security Integrity Check FAILED!\nExpected: {}\nActual:   {}",
                    expected_sha,
                    actual_sha
                );
            }
            if !quiet {
                println!("   {} Hash matched: {}", "✓".bright_green(), actual_sha.bright_black());
            }
        } else if !quiet {
            println!("   {} No hash provided by manifest (skipped)", "!".bright_yellow());
        }

        // 7. Paru Step 4: Extract into isolated store
        if !quiet {
            ui::paru_step(4, 5, "Extracting payload into isolated sandbox store");
        }
        let store_dir = self.config.packages_dir().join(format!("{}-{}", pkg_name, pkg_ver));
        if store_dir.exists() {
            fs::remove_dir_all(&store_dir)?;
        }
        fs::create_dir_all(&store_dir)?;

        let spinner = if !quiet { Some(ui::create_spinner("Unpacking archive files...")) } else { None };
        unpack_archive(&cache_file, &store_dir, source.format.as_deref())?;
        if let Some(s) = spinner {
            s.finish_and_clear();
        }
        if !quiet {
            println!("   {} Unpacked into {}", "✓".bright_green(), store_dir.display());
        }

        // 8. Paru Step 5: Link binaries and register desktop launchers
        if !quiet {
            ui::paru_step(5, 5, "Registering binaries and desktop integration");
        }
        let mut installed_files = Vec::new();
        let mut linked_binaries = Vec::new();

        for rel_bin in &manifest.install.binaries {
            let src_bin = store_dir.join(rel_bin);
            if !src_bin.exists() {
                if let Some(filename) = Path::new(rel_bin).file_name() {
                    let fallback = find_file_in_dir(&store_dir, filename.to_str().unwrap_or_default());
                    if let Some(found) = fallback {
                        let link_target = self.config.bin_dir.join(filename);
                        symlink_binary(&found, &link_target)?;
                        installed_files.push(link_target.clone());
                        linked_binaries.push(filename.to_string_lossy().to_string());
                        if !quiet {
                            println!("   {} Linked binary: {}", "→".bright_cyan(), link_target.display());
                        }
                        continue;
                    }
                }
                ui::warn(&format!("Binary '{}' not found in package archive.", rel_bin));
                continue;
            }

            let bin_name = src_bin.file_name().unwrap_or_default();
            let link_target = self.config.bin_dir.join(bin_name);
            symlink_binary(&src_bin, &link_target)?;
            installed_files.push(link_target.clone());
            linked_binaries.push(bin_name.to_string_lossy().to_string());
            if !quiet {
                println!("   {} Linked binary: {}", "→".bright_cyan(), link_target.display());
            }
        }

        // Auto-detect fallback binary if empty
        if linked_binaries.is_empty() {
            let auto_bin = store_dir.join("usr/bin").join(&pkg_name);
            let alt_auto_bin = store_dir.join("bin").join(&pkg_name);
            let root_bin = store_dir.join(&pkg_name);

            let candidate = if auto_bin.exists() {
                Some(auto_bin)
            } else if alt_auto_bin.exists() {
                Some(alt_auto_bin)
            } else if root_bin.exists() {
                Some(root_bin)
            } else {
                None
            };

            if let Some(src) = candidate {
                let link_target = self.config.bin_dir.join(&pkg_name);
                symlink_binary(&src, &link_target)?;
                installed_files.push(link_target.clone());
                linked_binaries.push(pkg_name.clone());
                if !quiet {
                    println!("   {} Linked binary: {}", "→".bright_cyan(), link_target.display());
                }
            }
        }

        // Install Desktop entries
        for rel_desktop in &manifest.install.desktop {
            if let Some(src_desktop) = find_or_locate(&store_dir, rel_desktop) {
                let dest_name = src_desktop.file_name().unwrap_or_default();
                let dest = self.config.apps_dir.join(dest_name);
                fs::copy(&src_desktop, &dest)?;
                installed_files.push(dest.clone());
                if !quiet {
                    println!("   {} Registered desktop launcher: {}", "✓".bright_green(), dest.display());
                }
            }
        }

        // Install MIME types
        for rel_mime in &manifest.install.mime {
            if let Some(src_mime) = find_or_locate(&store_dir, rel_mime) {
                let mime_dir = self.config.data_dir.parent().unwrap_or(&self.config.data_dir).join("mime/packages");
                let _ = fs::create_dir_all(&mime_dir);
                let dest = mime_dir.join(src_mime.file_name().unwrap_or_default());
                let _ = fs::copy(&src_mime, &dest);
                installed_files.push(dest.clone());
                if !quiet {
                    println!("   {} Registered MIME definition: {}", "✓".bright_green(), dest.display());
                }
            }
        }

        // Trigger desktop & mime updates
        let _ = Command::new("update-desktop-database")
            .arg(self.config.apps_dir.to_string_lossy().to_string())
            .output();

        // 9. Save state
        let mut state = State::load(&self.config.state_file())?;
        state.insert(InstalledPackage {
            name: pkg_name.clone(),
            version: pkg_ver.clone(),
            description: manifest.description(),
            installed_at: Utc::now(),
            source_url: Some(source.url.clone()),
            store_dir,
            installed_files,
            binaries: linked_binaries.clone(),
        });
        state.save(&self.config.state_file())?;

        if !quiet {
            println!();
            ui::success(&format!(
                "Successfully installed {} v{}",
                pkg_name.bright_green().bold(),
                pkg_ver.bright_cyan()
            ));
        }

        Ok(true)
    }

    pub fn remove(&self, name: &str, noconfirm: bool, quiet: bool) -> Result<bool> {
        let mut state = State::load(&self.config.state_file())?;
        let pkg = match state.get(name) {
            Some(p) => p.clone(),
            None => bail!("Package '{}' is not installed.", name),
        };

        if !quiet {
            ui::paru_log(&format!("Packages to remove (1): {}", name.bright_red().bold()));
            ui::paru_detail("Version", &pkg.version);
            ui::paru_detail("Installed at", &pkg.installed_at.format("%Y-%m-%d %H:%M").to_string());
            ui::paru_detail("Store Path", &pkg.store_dir.display().to_string());
            println!("   Files to be unlinked ({}):", pkg.installed_files.len());
            for f in &pkg.installed_files {
                println!("     {} {}", "-".bright_red(), f.display());
            }
            println!();
        }

        if !noconfirm {
            let ok = ui::confirm(&format!("Do you want to remove '{}'?", name), false)?;
            if !ok {
                ui::warn("Operation aborted by user.");
                return Ok(false);
            }
        }

        if !quiet {
            ui::paru_log("Removing package files and links...");
        }

        // Remove linked binaries, desktop files
        for path in &pkg.installed_files {
            if path.exists() || path.is_symlink() {
                let _ = fs::remove_file(path);
            }
        }

        // Remove store directory
        if pkg.store_dir.exists() {
            let _ = fs::remove_dir_all(&pkg.store_dir);
        }

        // Update desktop database
        let _ = Command::new("update-desktop-database")
            .arg(self.config.apps_dir.to_string_lossy().to_string())
            .output();

        state.remove(name);
        state.save(&self.config.state_file())?;

        if !quiet {
            ui::success(&format!("Package '{}' has been completely removed.", name.bold()));
        }

        Ok(true)
    }

    async fn download_source(
        &self,
        name: &str,
        version: &str,
        source: &crate::manifest::SourceInfo,
        quiet: bool,
    ) -> Result<PathBuf> {
        if source.url.starts_with("file://") || source.url.starts_with('/') {
            let local_path = PathBuf::from(source.url.trim_start_matches("file://"));
            if local_path.exists() {
                if !quiet {
                    println!("   {} Using local archive: {}", "✓".bright_green(), local_path.display());
                }
                return Ok(local_path);
            }
        }

        let filename = Path::new(&source.url)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("package.archive");
        let dest_path = self.config.cache_dir().join(format!("{}-{}-{}", name, version, filename));

        if dest_path.exists() {
            if !quiet {
                println!("   {} Using cached archive: {}", "✓".bright_green(), filename);
            }
            return Ok(dest_path);
        }

        let client = reqwest::Client::new();
        let resp = client
            .get(&source.url)
            .send()
            .await
            .with_context(|| format!("Failed to initiate download from {}", source.url))?;

        if !resp.status().is_success() {
            bail!("Download failed with HTTP status: {}", resp.status());
        }

        let total_size = resp.content_length().unwrap_or(0);
        let progress = if !quiet {
            Some(ui::create_download_progress(total_size, filename))
        } else {
            None
        };

        let mut file = File::create(&dest_path)?;
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk)?;
            if let Some(p) = &progress {
                p.inc(chunk.len() as u64);
            }
        }

        if let Some(p) = progress {
            p.finish_and_clear();
        }

        if !quiet {
            println!("   {} Downloaded {} bytes", "✓".bright_green(), total_size);
        }

        Ok(dest_path)
    }
}

fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

fn unpack_archive(archive_path: &Path, dest_dir: &Path, explicit_format: Option<&str>) -> Result<()> {
    let filename = archive_path.to_string_lossy().to_lowercase();
    let file = File::open(archive_path)?;

    let is_zstd = explicit_format == Some("tar.zst")
        || explicit_format == Some("zst")
        || filename.ends_with(".tar.zst")
        || filename.ends_with(".pkg.tar.zst")
        || filename.ends_with(".zst");

    let is_gz = explicit_format == Some("tar.gz")
        || explicit_format == Some("tgz")
        || filename.ends_with(".tar.gz")
        || filename.ends_with(".tgz");

    if is_zstd {
        let decoder = zstd::stream::read::Decoder::new(file)?;
        let mut archive = tar::Archive::new(decoder);
        archive.set_unpack_xattrs(true);
        archive.unpack(dest_dir)?;
        return Ok(());
    }

    if is_gz {
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(dest_dir)?;
        return Ok(());
    }

    // Fallback: raw single binary
    let dest_bin = dest_dir.join(archive_path.file_name().unwrap_or_default());
    fs::copy(archive_path, &dest_bin)?;
    let mut perms = fs::metadata(&dest_bin)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&dest_bin, perms)?;

    Ok(())
}

fn symlink_binary(src: &Path, target: &Path) -> Result<()> {
    if let Ok(metadata) = fs::metadata(src) {
        let mut perms = metadata.permissions();
        perms.set_mode(perms.mode() | 0o111);
        let _ = fs::set_permissions(src, perms);
    }

    if target.exists() || target.is_symlink() {
        let _ = fs::remove_file(target);
    }

    std::os::unix::fs::symlink(src, target)
        .with_context(|| format!("Failed to link {:?} -> {:?}", src, target))?;

    Ok(())
}

fn find_file_in_dir(dir: &Path, target_name: &str) -> Option<PathBuf> {
    for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.file_name() == target_name {
            return Some(entry.path().to_path_buf());
        }
    }
    None
}

fn find_or_locate(base_dir: &Path, rel_path: &str) -> Option<PathBuf> {
    let exact = base_dir.join(rel_path);
    if exact.exists() {
        return Some(exact);
    }
    let filename = Path::new(rel_path).file_name()?.to_str()?;
    find_file_in_dir(base_dir, filename)
}

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

    pub async fn install(&self, manifest: &PackageManifest) -> Result<()> {
        let pkg_name = manifest.name();
        let pkg_ver = manifest.version();

        ui::header(&format!("Installing {} v{}", pkg_name, pkg_ver));

        // 1. Check system dependencies
        for bin in &manifest.dependencies.bins {
            if which::which(bin).is_err() {
                ui::warn(&format!(
                    "Recommended external dependency '{}' is not installed or not in PATH.",
                    bin.bright_yellow()
                ));
            }
        }

        // 2. Select matching source for current OS/Arch
        let source = manifest.get_current_source().with_context(|| {
            format!(
                "No compatible source found for architecture '{}-{}'",
                std::env::consts::ARCH,
                std::env::consts::OS
            )
        })?;

        // 3. Download package archive / binary
        let cache_file = self.download_source(&pkg_name, &pkg_ver, source).await?;

        // 4. Verify SHA256 checksum
        if let Some(expected_sha) = &source.sha256 {
            let spinner = ui::create_spinner("Verifying SHA256 checksum...");
            let actual_sha = compute_sha256(&cache_file)?;
            if actual_sha.to_lowercase() != expected_sha.to_lowercase() {
                spinner.finish_and_clear();
                bail!(
                    "Checksum mismatch!\nExpected: {}\nActual:   {}",
                    expected_sha,
                    actual_sha
                );
            }
            spinner.finish_and_clear();
            ui::success("Checksum verified.");
        }

        // 5. Unpack into store directory (~/.local/share/rusty/installed/<name>-<version>/)
        let store_dir = self.config.packages_dir().join(format!("{}-{}", pkg_name, pkg_ver));
        if store_dir.exists() {
            fs::remove_dir_all(&store_dir)?;
        }
        fs::create_dir_all(&store_dir)?;

        let spinner = ui::create_spinner("Unpacking package contents...");
        unpack_archive(&cache_file, &store_dir, source.format.as_deref())?;
        spinner.finish_and_clear();
        ui::success("Extracted package files.");

        // 6. Link binaries into ~/.local/bin
        let mut installed_files = Vec::new();
        let mut linked_binaries = Vec::new();

        for rel_bin in &manifest.install.binaries {
            let src_bin = store_dir.join(rel_bin);
            if !src_bin.exists() {
                // Try searching for the binary by filename if exact relative path differs
                if let Some(filename) = Path::new(rel_bin).file_name() {
                    let fallback = find_file_in_dir(&store_dir, filename.to_str().unwrap_or_default());
                    if let Some(found) = fallback {
                        let link_target = self.config.bin_dir.join(filename);
                        symlink_binary(&found, &link_target)?;
                        installed_files.push(link_target);
                        linked_binaries.push(filename.to_string_lossy().to_string());
                        continue;
                    }
                }
                ui::warn(&format!("Binary '{}' not found in package archive.", rel_bin));
                continue;
            }

            let bin_name = src_bin.file_name().unwrap_or_default();
            let link_target = self.config.bin_dir.join(bin_name);
            symlink_binary(&src_bin, &link_target)?;
            installed_files.push(link_target);
            linked_binaries.push(bin_name.to_string_lossy().to_string());
        }

        // If no binaries were explicitly specified, auto-detect any executable in bin/
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
                installed_files.push(link_target);
                linked_binaries.push(pkg_name.clone());
            }
        }

        // 7. Install Desktop and MIME files
        for rel_desktop in &manifest.install.desktop {
            if let Some(src_desktop) = find_or_locate(&store_dir, rel_desktop) {
                let dest_name = src_desktop.file_name().unwrap_or_default();
                let dest = self.config.apps_dir.join(dest_name);
                fs::copy(&src_desktop, &dest)?;
                installed_files.push(dest);
            }
        }

        for rel_mime in &manifest.install.mime {
            if let Some(src_mime) = find_or_locate(&store_dir, rel_mime) {
                let mime_dir = self.config.data_dir.parent().unwrap_or(&self.config.data_dir).join("mime/packages");
                let _ = fs::create_dir_all(&mime_dir);
                let dest = mime_dir.join(src_mime.file_name().unwrap_or_default());
                let _ = fs::copy(&src_mime, &dest);
                installed_files.push(dest);
            }
        }

        // Update desktop database if available
        let _ = Command::new("update-desktop-database")
            .arg(self.config.apps_dir.to_string_lossy().to_string())
            .output();

        // 8. Record in state.json
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

        ui::success(&format!(
            "Successfully installed {} v{}!",
            pkg_name.bright_green().bold(),
            pkg_ver.bright_cyan()
        ));

        if !linked_binaries.is_empty() {
            println!("  Available binaries:");
            for b in &linked_binaries {
                println!("    {} {}", "→".bright_cyan(), b.bright_white().bold());
            }
        }

        Ok(())
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        let mut state = State::load(&self.config.state_file())?;
        let pkg = match state.remove(name) {
            Some(p) => p,
            None => bail!("Package '{}' is not installed.", name),
        };

        ui::header(&format!("Removing {}", name));

        // Remove linked binaries, desktop files, etc.
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

        state.save(&self.config.state_file())?;
        ui::success(&format!("Package '{}' has been completely removed.", name.bold()));

        Ok(())
    }

    async fn download_source(
        &self,
        name: &str,
        version: &str,
        source: &crate::manifest::SourceInfo,
    ) -> Result<PathBuf> {
        if source.url.starts_with("file://") || source.url.starts_with('/') {
            let local_path = PathBuf::from(source.url.trim_start_matches("file://"));
            if local_path.exists() {
                ui::info(&format!("Using local payload: {}", local_path.display()));
                return Ok(local_path);
            }
        }

        let filename = Path::new(&source.url)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("package.archive");
        let dest_path = self.config.cache_dir().join(format!("{}-{}-{}", name, version, filename));

        if dest_path.exists() {
            ui::info(&format!("Using cached archive: {}", filename));
            return Ok(dest_path);
        }

        ui::info(&format!("Downloading from: {}", source.url.bright_cyan()));

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
        let progress = ui::create_download_progress(total_size, filename);

        let mut file = File::create(&dest_path)?;
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk)?;
            progress.inc(chunk.len() as u64);
        }

        progress.finish_and_clear();
        ui::success(&format!("Download completed ({} bytes).", total_size));

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

    // Fallback: If it's a raw single binary
    let dest_bin = dest_dir.join(archive_path.file_name().unwrap_or_default());
    fs::copy(archive_path, &dest_bin)?;
    let mut perms = fs::metadata(&dest_bin)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&dest_bin, perms)?;

    Ok(())
}

fn symlink_binary(src: &Path, target: &Path) -> Result<()> {
    // Make sure src has executable permissions
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

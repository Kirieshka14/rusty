use crate::manifest::PackageManifest;
use crate::ui;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub struct Packer;

impl Packer {
    pub fn pack(project_dir: &Path, output_dir: &Path) -> Result<PathBuf> {
        let manifest_path = project_dir.join("rusty.toml");
        if !manifest_path.exists() {
            bail!("No 'rusty.toml' manifest found in {:?}", project_dir);
        }

        let content = fs::read_to_string(&manifest_path)?;
        let manifest: PackageManifest = toml::from_str(&content)
            .context("Failed to parse rusty.toml")?;

        let arch = std::env::consts::ARCH;
        let os = std::env::consts::OS;
        let archive_name = format!("{}-{}-{}-{}.tar.zst", manifest.name, manifest.version, arch, os);
        let archive_path = output_dir.join(&archive_name);

        ui::header(&format!("Packaging {} v{}", manifest.name, manifest.version));

        let file = File::create(&archive_path)?;
        let encoder = zstd::stream::write::Encoder::new(file, 9)?.auto_finish();
        let mut tar_builder = tar::Builder::new(encoder);

        // Pack files defined in manifest install rules
        for bin in &manifest.install.binaries {
            let bin_path = project_dir.join(bin);
            if bin_path.exists() {
                tar_builder.append_path_with_name(&bin_path, bin)?;
            } else {
                ui::warn(&format!("Binary path '{}' does not exist yet.", bin));
            }
        }

        for desktop in &manifest.install.desktop {
            let p = project_dir.join(desktop);
            if p.exists() {
                tar_builder.append_path_with_name(&p, desktop)?;
            }
        }

        for mime in &manifest.install.mime {
            let p = project_dir.join(mime);
            if p.exists() {
                tar_builder.append_path_with_name(&p, mime)?;
            }
        }

        // Also include rusty.toml in archive
        tar_builder.append_path_with_name(&manifest_path, "rusty.toml")?;

        tar_builder.finish()?;
        drop(tar_builder);

        // Compute sha256
        let mut hasher = Sha256::new();
        let mut f = File::open(&archive_path)?;
        std::io::copy(&mut f, &mut hasher)?;
        let sha256 = hex::encode(hasher.finalize());

        ui::success(&format!("Successfully created package: {}", archive_name));
        println!("  Size:   {} bytes", fs::metadata(&archive_path)?.len());
        println!("  SHA256: {}", sha256);

        Ok(archive_path)
    }
}

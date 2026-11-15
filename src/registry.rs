use crate::manifest::PackageManifest;
use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

pub struct RegistryClient {
    client: Client,
    base_url: String,
}

impl RegistryClient {
    pub fn new(base_url: String) -> Self {
        let client = Client::builder()
            .user_agent("rusty-package-manager/0.1.0")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self { client, base_url }
    }

    pub async fn fetch_manifest(&self, name: &str) -> Result<PackageManifest> {
        // 1. Direct path to a local manifest file
        if name.ends_with(".toml") || name.ends_with(".json") {
            let path = std::path::Path::new(name);
            if path.exists() {
                let content = std::fs::read_to_string(path)?;
                if name.ends_with(".json") {
                    return Ok(serde_json::from_str(&content)?);
                } else {
                    return Ok(toml::from_str(&content)?);
                }
            }
        }

        // 2. Local recipe cache/override (~/.local/share/rusty/recipes/<name>.toml)
        let home = std::env::var("HOME").unwrap_or_default();
        let local_recipe = std::path::PathBuf::from(&home)
            .join(".local/share/rusty/recipes")
            .join(format!("{}.toml", name));
        if local_recipe.exists() {
            let content = std::fs::read_to_string(&local_recipe)?;
            return Ok(toml::from_str(&content)?);
        }

        // 3. Built-in seed packages (for immediate zero-configuration out-of-the-box experience)
        if name.to_lowercase() == "rustydroid" {
            let manifest: PackageManifest = toml::from_str(include_str!("../seed/rustydroid.toml"))
                .context("Failed to parse seed rustydroid manifest")?;
            return Ok(manifest);
        }

        // 4. Remote Registry API
        let url = format!("{}/packages/{}", self.base_url.trim_end_matches('/'), name);
        let resp = self.client.get(&url).send().await;

        match resp {
            Ok(r) if r.status().is_success() => {
                let text = r.text().await?;
                if let Ok(manifest) = serde_json::from_str::<PackageManifest>(&text) {
                    return Ok(manifest);
                }
                if let Ok(manifest) = toml::from_str::<PackageManifest>(&text) {
                    return Ok(manifest);
                }
                bail!("Invalid manifest format received from registry for '{}'", name);
            }
            Ok(r) if r.status().as_u16() == 404 => {
                bail!("Package '{}' not found in registry at {}", name, self.base_url);
            }
            Ok(r) => {
                bail!("Registry returned error {}: {}", r.status(), url);
            }
            Err(e) => {
                bail!("Failed to connect to registry ({}): {}", self.base_url, e);
            }
        }
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();

        // Include seed packages in search
        if "rustydroid".contains(&query.to_lowercase()) {
            results.push(SearchResult {
                name: "rustydroid".to_string(),
                version: "0.1.0".to_string(),
                description: Some("Waydroid-based Android runner with Hyprland window sync".to_string()),
            });
        }

        let url = format!("{}/search?q={}", self.base_url.trim_end_matches('/'), query);
        if let Ok(resp) = self.client.get(&url).send().await {
            if resp.status().is_success() {
                if let Ok(remote_results) = resp.json::<Vec<SearchResult>>().await {
                    for r in remote_results {
                        if !results.iter().any(|existing| existing.name == r.name) {
                            results.push(r);
                        }
                    }
                }
            }
        }

        Ok(results)
    }
}

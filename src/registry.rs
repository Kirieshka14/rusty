use crate::manifest::PackageManifest;
use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub repo_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RegistryPackageResponse {
    package: Option<RegistryPackageData>,
}

#[derive(Debug, Deserialize)]
struct RegistrySearchResponse {
    packages: Option<Vec<RegistryPackageData>>,
}

#[derive(Debug, Clone, Deserialize)]
struct RegistryPackageData {
    name: String,
    repo_url: Option<String>,
    description: Option<String>,
    author: Option<String>,
}

pub struct RegistryClient {
    client: Client,
    base_url: String,
}

impl RegistryClient {
    pub fn new(base_url: String) -> Self {
        let client = Client::builder()
            .user_agent("rusty-package-manager/0.1.0")
            .timeout(std::time::Duration::from_secs(10))
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

        // 3. Query Registry API
        let url = format!("{}/packages/{}", self.base_url.trim_end_matches('/'), name);
        let resp = self.client.get(&url).send().await;

        if let Ok(r) = resp {
            if r.status().is_success() {
                let text = r.text().await.unwrap_or_default();

                // Check if response is { "package": { ... } } from local Next.js registry
                if let Ok(reg_resp) = serde_json::from_str::<RegistryPackageResponse>(&text) {
                    if let Some(pkg_data) = reg_resp.package {
                        // If it has repo_url, try to fetch rusty.toml from the repo
                        if let Some(repo_url) = &pkg_data.repo_url {
                            if let Some(manifest) = self.try_fetch_from_repo(repo_url).await {
                                return Ok(manifest);
                            }
                        }

                        // If it's rustydroid, fallback to built-in seed manifest enriched with registry info
                        if pkg_data.name.to_lowercase() == "rustydroid" {
                            let mut seed_manifest: PackageManifest = toml::from_str(include_str!("../seed/rustydroid.toml"))
                                .context("Failed to parse seed manifest")?;
                            if let Some(desc) = pkg_data.description {
                                if !desc.is_empty() {
                                    seed_manifest.description = Some(desc);
                                }
                            }
                            return Ok(seed_manifest);
                        }
                    }
                }

                // Try direct PackageManifest deserialization (TOML or JSON)
                if let Ok(manifest) = serde_json::from_str::<PackageManifest>(&text) {
                    return Ok(manifest);
                }
                if let Ok(manifest) = toml::from_str::<PackageManifest>(&text) {
                    return Ok(manifest);
                }
            }
        }

        // 4. Built-in seed fallback
        if name.to_lowercase() == "rustydroid" {
            let manifest: PackageManifest = toml::from_str(include_str!("../seed/rustydroid.toml"))
                .context("Failed to parse seed rustydroid manifest")?;
            return Ok(manifest);
        }

        bail!("Package '{}' not found in registry ({})", name, self.base_url);
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();

        // 1. Query remote registry
        let url = format!("{}/packages?q={}", self.base_url.trim_end_matches('/'), query);
        if let Ok(resp) = self.client.get(&url).send().await {
            if resp.status().is_success() {
                if let Ok(search_resp) = resp.json::<RegistrySearchResponse>().await {
                    if let Some(pkgs) = search_resp.packages {
                        for p in pkgs {
                            results.push(SearchResult {
                                name: p.name,
                                version: "0.1.0".to_string(),
                                description: p.description,
                                author: p.author,
                                repo_url: p.repo_url,
                            });
                        }
                    }
                }
            }
        }

        // 2. Add built-in seed package if matching and not already present
        if "rustydroid".contains(&query.to_lowercase()) && !results.iter().any(|r| r.name == "rustydroid") {
            results.push(SearchResult {
                name: "rustydroid".to_string(),
                version: "0.1.0".to_string(),
                description: Some("Waydroid-based Android runner with Hyprland window sync".to_string()),
                author: Some("kiri".to_string()),
                repo_url: Some("https://github.com/Kirieshka14/rustydroid".to_string()),
            });
        }

        Ok(results)
    }

    async fn try_fetch_from_repo(&self, repo_url: &str) -> Option<PackageManifest> {
        // Handle github.com/<owner>/<repo>
        if repo_url.contains("github.com/") {
            let clean = repo_url
                .trim_end_matches('/')
                .trim_end_matches(".git")
                .replace("https://github.com/", "")
                .replace("http://github.com/", "");

            let raw_urls = [
                format!("https://raw.githubusercontent.com/{}/master/rusty.toml", clean),
                format!("https://raw.githubusercontent.com/{}/main/rusty.toml", clean),
                format!("https://raw.githubusercontent.com/{}/master/packaging/rusty.toml", clean),
            ];

            for url in &raw_urls {
                if let Ok(resp) = self.client.get(url).timeout(std::time::Duration::from_secs(2)).send().await {
                    if resp.status().is_success() {
                        if let Ok(text) = resp.text().await {
                            if let Ok(manifest) = toml::from_str::<PackageManifest>(&text) {
                                return Some(manifest);
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

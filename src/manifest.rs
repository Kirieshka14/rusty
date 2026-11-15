use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    #[serde(default)]
    pub package: Option<PackageMeta>,

    // Flat fallbacks (for JSON API or flat manifests)
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,

    #[serde(default)]
    pub sources: HashMap<String, SourceInfo>,
    #[serde(default)]
    pub install: InstallRules,
    #[serde(default)]
    pub dependencies: Dependencies,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceInfo {
    pub url: String,
    pub sha256: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstallRules {
    #[serde(default)]
    pub binaries: Vec<String>,
    #[serde(default)]
    pub desktop: Vec<String>,
    #[serde(default)]
    pub icons: Vec<String>,
    #[serde(default)]
    pub mime: Vec<String>,
    #[serde(default)]
    pub post_install: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Dependencies {
    #[serde(default)]
    pub bins: Vec<String>,
}

impl PackageManifest {
    pub fn name(&self) -> String {
        self.package.as_ref().map(|p| p.name.clone())
            .or_else(|| self.name.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn version(&self) -> String {
        self.package.as_ref().map(|p| p.version.clone())
            .or_else(|| self.version.clone())
            .unwrap_or_else(|| "0.0.0".to_string())
    }

    pub fn description(&self) -> Option<String> {
        self.package.as_ref().and_then(|p| p.description.clone())
            .or_else(|| self.description.clone())
    }

    pub fn author(&self) -> Option<String> {
        self.package.as_ref().and_then(|p| p.author.clone())
            .or_else(|| self.author.clone())
    }

    pub fn license(&self) -> Option<String> {
        self.package.as_ref().and_then(|p| p.license.clone())
            .or_else(|| self.license.clone())
    }

    pub fn homepage(&self) -> Option<String> {
        self.package.as_ref().and_then(|p| p.homepage.clone())
            .or_else(|| self.homepage.clone())
    }

    pub fn repository(&self) -> Option<String> {
        self.package.as_ref().and_then(|p| p.repository.clone())
            .or_else(|| self.repository.clone())
    }

    pub fn get_current_source(&self) -> Option<&SourceInfo> {
        let arch = std::env::consts::ARCH;
        let os = std::env::consts::OS;

        let keys_to_try = [
            format!("{}-{}", arch, os),
            arch.to_string(),
            "default".to_string(),
            "universal".to_string(),
        ];

        for key in &keys_to_try {
            if let Some(src) = self.sources.get(key) {
                return Some(src);
            }
        }

        if self.sources.len() == 1 {
            return self.sources.values().next();
        }

        None
    }
}

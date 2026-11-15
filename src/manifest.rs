use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
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
    pub fn get_current_source(&self) -> Option<&SourceInfo> {
        let arch = std::env::consts::ARCH; // e.g. "x86_64", "aarch64"
        let os = std::env::consts::OS;     // e.g. "linux"

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

        // Fallback: if only 1 source exists, use it
        if self.sources.len() == 1 {
            return self.sources.values().next();
        }

        None
    }
}

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub registry_url: String,
    pub bin_dir: PathBuf,
    pub data_dir: PathBuf,
    pub apps_dir: PathBuf,
    pub icons_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/tmp"));
        Self {
            registry_url: "https://rusty-registry.pages.dev/api/v1".to_string(),
            bin_dir: home.join(".local/bin"),
            data_dir: home.join(".local/share/rusty"),
            apps_dir: home.join(".local/share/applications"),
            icons_dir: home.join(".local/share/icons"),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let mut config = Self::default();

        if let Some(proj_dirs) = ProjectDirs::from("org", "rusty", "rusty") {
            let config_path = proj_dirs.config_dir().join("config.toml");
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)
                    .with_context(|| format!("Failed to read config from {:?}", config_path))?;
                if let Ok(loaded) = toml::from_str::<Config>(&content) {
                    config = loaded;
                }
            }
        }

        // Environment variable override
        if let Ok(url) = std::env::var("RUSTY_REGISTRY_URL") {
            config.registry_url = url;
        }

        // Ensure critical directories exist
        std::fs::create_dir_all(&config.bin_dir)?;
        std::fs::create_dir_all(&config.data_dir)?;
        std::fs::create_dir_all(&config.apps_dir)?;
        std::fs::create_dir_all(&config.icons_dir)?;
        std::fs::create_dir_all(config.packages_dir())?;
        std::fs::create_dir_all(config.cache_dir())?;

        Ok(config)
    }

    pub fn packages_dir(&self) -> PathBuf {
        self.data_dir.join("installed")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache")
    }

    pub fn state_file(&self) -> PathBuf {
        self.data_dir.join("state.json")
    }
}

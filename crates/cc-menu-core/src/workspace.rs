use crate::config::{APP_NAME, AppConfig};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default() -> Result<Self> {
        let base = dirs::data_local_dir()
            .or_else(dirs::config_dir)
            .context("could not resolve user data directory")?;
        Ok(Self::new(base.join(APP_NAME)))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.json")
    }

    pub fn menu_cache_path(&self) -> PathBuf {
        self.root.join("menu-cache.json")
    }

    pub fn launch_log_path(&self) -> PathBuf {
        self.root.join("launch-log.jsonl")
    }

    pub fn session_db_path(&self) -> PathBuf {
        self.root.join("sessions.sqlite3")
    }

    pub fn registry_dir(&self) -> PathBuf {
        self.root.join("platform")
    }

    pub fn standard_context_root(&self) -> PathBuf {
        self.root.join("sessions")
    }

    pub fn init(&self) -> Result<AppConfig> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create workspace {}", self.root.display()))?;
        let config_path = self.config_path();
        if config_path.exists() {
            let config = AppConfig::load(&config_path)?;
            config.validate()?;
            return Ok(config);
        }
        let config = AppConfig::default_with_roots(&self.root);
        config.save(&config_path)?;
        Ok(config)
    }

    pub fn load_config(&self) -> Result<AppConfig> {
        let config = AppConfig::load(&self.config_path())?;
        config.validate()?;
        Ok(config)
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<()> {
        config.validate()?;
        config.save(&self.config_path())
    }
}

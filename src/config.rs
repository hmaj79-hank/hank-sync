//! Configuration management

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub server: ServerConfig,
    pub client: ClientConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    pub root: String,
    pub bind: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            root: "/backup/incoming".to_string(),
            bind: "0.0.0.0:4433".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientConfig {
    pub default_server: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            default_server: "192.168.178.20:4433".to_string(),
        }
    }
}

fn default_config_dir() -> Result<PathBuf> {
    Ok(directories::ProjectDirs::from("", "", "hank-sync")
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
        .config_dir()
        .to_path_buf())
}

pub fn config_dir() -> Result<PathBuf> {
    default_config_dir()
}

pub fn resolve_server(override_server: Option<String>) -> Result<String> {
    if let Some(server) = override_server {
        return Ok(server);
    }

    let config = load(None)?;
    Ok(config.client.default_server)
}

pub fn load(config_dir: Option<&Path>) -> Result<Config> {
    let dir = match config_dir {
        Some(d) => d.to_path_buf(),
        None => default_config_dir()?,
    };

    let config_path = dir.join("config.toml");
    if !config_path.exists() {
        anyhow::bail!("Config not found: {:?}. Run 'hank-sync init' first.", config_path);
    }

    let content = std::fs::read_to_string(&config_path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}

pub fn init(config_dir: Option<&Path>) -> Result<()> {
    let dir = match config_dir {
        Some(d) => d.to_path_buf(),
        None => default_config_dir()?,
    };
    
    std::fs::create_dir_all(&dir)?;
    
    let config_path = dir.join("config.toml");
    
    if config_path.exists() {
        println!("⚠️  Config already exists: {:?}", config_path);
        return Ok(());
    }
    
    let config = Config::default();
    let content = toml::to_string_pretty(&config)?;
    std::fs::write(&config_path, content)?;
    
    println!("✅ Created config: {:?}", config_path);
    
    Ok(())
}

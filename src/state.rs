//! Client navigation state (cwd + previous)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct State {
    pub cwd: String,
    pub prev: String,
}

impl Default for State {
    fn default() -> Self {
        Self { cwd: "/".to_string(), prev: "/".to_string() }
    }
}

fn state_path() -> Result<PathBuf> {
    let dir = crate::config::config_dir()?;
    Ok(dir.join("state.json"))
}

pub fn load() -> Result<State> {
    let path = state_path()?;
    if !path.exists() {
        return Ok(State::default());
    }
    let content = std::fs::read_to_string(path)?;
    let state: State = serde_json::from_str(&content)?;
    Ok(state)
}

pub fn save(state: &State) -> Result<()> {
    let path = state_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(state)?;
    std::fs::write(path, content)?;
    Ok(())
}

pub fn normalize(path: &str) -> String {
    if path.is_empty() {
        return "/".to_string();
    }
    let mut p = path.replace("//", "/");
    if !p.starts_with('/') {
        p = format!("/{}", p);
    }
    if p.len() > 1 && p.ends_with('/') {
        p = p.trim_end_matches('/').to_string();
    }
    p
}

pub fn join(cwd: &str, dir: &str) -> String {
    if dir.starts_with('/') {
        return normalize(dir);
    }
    if cwd == "/" {
        normalize(&format!("/{}", dir))
    } else {
        normalize(&format!("{}/{}", cwd.trim_end_matches('/'), dir))
    }
}

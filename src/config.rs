use std::fs;
use std::path::{Path, PathBuf};

use crate::models::Config;

pub struct ConfigStore {
    pub path: PathBuf,
}

impl ConfigStore {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let path = home.join(".ssh").join("tunnels.json");
        Self { path }
    }

    #[cfg(test)]
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Config {
        if !self.path.exists() {
            return Config::empty();
        }
        match fs::read_to_string(&self.path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| {
                eprintln!("Warning: could not parse {}, using empty config", self.path.display());
                Config::empty()
            }),
            Err(e) => {
                eprintln!("Warning: could not read {}: {}", self.path.display(), e);
                Config::empty()
            }
        }
    }

    pub fn save(&self, config: &Config) -> Result<(), String> {
        let dir = self
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        if let Err(e) = fs::create_dir_all(dir) {
            return Err(format!("could not create config dir: {e}"));
        }
        let json = serde_json::to_string_pretty(config)
            .map_err(|e| format!("could not serialize config: {e}"))?;
        fs::write(&self.path, json).map_err(|e| format!("could not write config: {e}"))
    }
}

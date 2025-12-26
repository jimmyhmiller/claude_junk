use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Not initialized. Run 'agile init' first.")]
    NotInitialized,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

pub type Result<T> = std::result::Result<T, StorageError>;

const AGILE_DIR: &str = ".agile";

pub fn get_agile_dir() -> PathBuf {
    PathBuf::from(AGILE_DIR)
}

pub fn ensure_initialized() -> Result<PathBuf> {
    let dir = get_agile_dir();
    if !dir.exists() {
        return Err(StorageError::NotInitialized);
    }
    Ok(dir)
}

pub fn init_storage() -> Result<()> {
    let dir = get_agile_dir();
    fs::create_dir_all(&dir)?;

    // Create subdirectories for each type
    let subdirs = ["standups", "bugs", "retros", "tasks", "decisions", "notes", "reviews", "kudos"];
    for subdir in subdirs {
        fs::create_dir_all(dir.join(subdir))?;
    }

    // Create config file
    let config_path = dir.join("config.yaml");
    if !config_path.exists() {
        fs::write(&config_path, "# Agile CLI configuration\nversion: 1\n")?;
    }

    Ok(())
}

pub fn get_author() -> String {
    // Try to get from git config
    std::process::Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| whoami::username())
}

pub fn load_yaml<T: DeserializeOwned>(path: &PathBuf) -> Result<T> {
    let content = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&content)?)
}

pub fn save_yaml<T: Serialize>(path: &PathBuf, data: &T) -> Result<()> {
    let content = serde_yaml::to_string(data)?;
    fs::write(path, content)?;
    Ok(())
}

pub fn load_all_from_dir<T: DeserializeOwned>(subdir: &str) -> Result<Vec<(String, T)>> {
    let dir = ensure_initialized()?.join(subdir);
    let mut items = Vec::new();

    if dir.exists() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "yaml").unwrap_or(false) {
                if let Ok(item) = load_yaml::<T>(&path) {
                    let id = path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    items.push((id, item));
                }
            }
        }
    }

    Ok(items)
}

fn whoami_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

mod whoami {
    pub fn username() -> String {
        super::whoami_username()
    }
}

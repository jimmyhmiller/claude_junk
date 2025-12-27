use crate::user::Session;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_DIR: &str = ".agile";
const CONFIG_FILE: &str = "config.yaml";
const SESSION_FILE: &str = "session.yaml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub sync: SyncConfig,
    pub team_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub enabled: bool,
    pub backend: SyncBackendType,
    pub server_url: Option<String>,
    pub git_remote: Option<String>,
    pub git_branch: Option<String>,
    pub auto_sync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SyncBackendType {
    Local,
    Http,
    Git,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            sync: SyncConfig {
                enabled: false,
                backend: SyncBackendType::Local,
                server_url: None,
                git_remote: None,
                git_branch: None,
                auto_sync: false,
            },
            team_id: None,
        }
    }
}

impl Config {
    pub fn config_dir() -> PathBuf {
        PathBuf::from(CONFIG_DIR)
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join(CONFIG_FILE)
    }

    pub fn session_path() -> PathBuf {
        Self::config_dir().join(SESSION_FILE)
    }

    pub fn data_dir(entity_type: &str) -> PathBuf {
        Self::config_dir().join(entity_type)
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            Ok(serde_yaml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        let content = serde_yaml::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn load_session() -> Result<Option<Session>> {
        let path = Self::session_path();
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let session: Session = serde_yaml::from_str(&content)?;
            if session.is_expired() {
                Ok(None)
            } else {
                Ok(Some(session))
            }
        } else {
            Ok(None)
        }
    }

    pub fn save_session(session: &Session) -> Result<()> {
        let path = Self::session_path();
        let content = serde_yaml::to_string(session)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn clear_session() -> Result<()> {
        let path = Self::session_path();
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn ensure_initialized() -> Result<PathBuf> {
        let dir = Self::config_dir();
        if !dir.exists() {
            return Err(crate::Error::NotInitialized);
        }
        Ok(dir)
    }

    pub fn init() -> Result<()> {
        let dir = Self::config_dir();
        fs::create_dir_all(&dir)?;

        // Create config if not exists
        let config_path = Self::config_path();
        if !config_path.exists() {
            let config = Self::default();
            config.save()?;
        }

        Ok(())
    }

    pub fn init_data_dir(entity_type: &str) -> Result<PathBuf> {
        let dir = Self::data_dir(entity_type);
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

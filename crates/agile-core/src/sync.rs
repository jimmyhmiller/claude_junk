mod git_backend;
mod http_backend;
mod local_backend;

pub use git_backend::GitSyncBackend;
pub use http_backend::HttpSyncBackend;
pub use local_backend::LocalSyncBackend;

use crate::config::{Config, SyncBackendType};
use crate::types::SyncChange;
use crate::user::Session;
use crate::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Status of a sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: Vec<SyncConflict>,
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            pushed: 0,
            pulled: 0,
            conflicts: Vec::new(),
            last_sync: None,
        }
    }
}

/// A sync conflict that needs resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConflict {
    pub entity_type: String,
    pub entity_id: String,
    pub local_version: u64,
    pub remote_version: u64,
    pub message: String,
}

/// Trait for pluggable sync backends
#[async_trait]
pub trait SyncBackend: Send + Sync {
    /// Push local changes to remote
    async fn push(&self, changes: Vec<SyncChange>, session: &Session) -> Result<Vec<SyncChange>>;

    /// Pull remote changes since last sync
    async fn pull(&self, entity_type: &str, since_version: u64, session: &Session) -> Result<Vec<SyncChange>>;

    /// Get the current server version for an entity type
    async fn get_version(&self, entity_type: &str, session: &Session) -> Result<u64>;

    /// Check if the backend is available
    async fn health_check(&self) -> Result<bool>;

    /// Backend type identifier
    fn backend_type(&self) -> SyncBackendType;
}

/// Manages sync operations across backends
pub struct SyncManager {
    backend: Box<dyn SyncBackend>,
}

impl SyncManager {
    pub fn new(backend: Box<dyn SyncBackend>) -> Self {
        Self { backend }
    }

    pub fn from_config(config: &Config) -> Result<Self> {
        let backend: Box<dyn SyncBackend> = match config.sync.backend {
            SyncBackendType::Local => Box::new(LocalSyncBackend::new()),
            SyncBackendType::Http => {
                let server_url = config.sync.server_url.as_ref()
                    .ok_or_else(|| Error::Other("Server URL not configured".into()))?;
                Box::new(HttpSyncBackend::new(server_url.clone()))
            }
            SyncBackendType::Git => {
                let remote = config.sync.git_remote.clone().unwrap_or_else(|| "origin".into());
                let branch = config.sync.git_branch.clone();
                Box::new(GitSyncBackend::new(remote, branch))
            }
        };
        Ok(Self::new(backend))
    }

    pub async fn sync(&self, entity_type: &str, session: &Session) -> Result<SyncStatus> {
        let storage = crate::Storage::new(entity_type)?;
        let mut status = SyncStatus::default();

        // Get pending local changes
        let local_changes = storage.pending_changes()?;

        // Push local changes
        if !local_changes.is_empty() {
            let pushed = self.backend.push(local_changes, session).await?;
            status.pushed = pushed.len();

            // Clear pending changes and mark entities as synced
            storage.clear_pending_changes()?;
        }

        // Get last synced version (simplified - in production would track per-entity)
        let since_version = 0; // TODO: Track properly

        // Pull remote changes
        let remote_changes = self.backend.pull(entity_type, since_version, session).await?;
        status.pulled = remote_changes.len();

        // Apply remote changes
        for change in remote_changes {
            self.apply_remote_change(&storage, change, &mut status)?;
        }

        status.last_sync = Some(chrono::Utc::now());
        Ok(status)
    }

    fn apply_remote_change(
        &self,
        storage: &crate::Storage,
        change: SyncChange,
        status: &mut SyncStatus,
    ) -> Result<()> {
        use crate::types::SyncOperation;

        match change.operation {
            SyncOperation::Create | SyncOperation::Update => {
                // Write the remote data
                let path = Config::data_dir(storage.entity_type()).join(format!("{}.yaml", change.id));
                let sync_entity = crate::types::SyncEntity {
                    data: change.data,
                    sync_version: change.version,
                    last_synced_at: Some(chrono::Utc::now()),
                    local_only: false,
                };
                let content = serde_yaml::to_string(&sync_entity)?;
                std::fs::write(path, content)?;
            }
            SyncOperation::Delete => {
                let path = Config::data_dir(storage.entity_type()).join(format!("{}.yaml", change.id));
                if path.exists() {
                    std::fs::remove_file(path)?;
                }
            }
        }

        Ok(())
    }

    pub async fn health_check(&self) -> Result<bool> {
        self.backend.health_check().await
    }

    pub fn backend_type(&self) -> SyncBackendType {
        self.backend.backend_type()
    }
}

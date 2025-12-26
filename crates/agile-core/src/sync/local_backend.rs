use crate::config::SyncBackendType;
use crate::sync::SyncBackend;
use crate::types::SyncChange;
use crate::user::Session;
use crate::Result;
use async_trait::async_trait;

/// Local-only sync backend (no-op, just for offline mode)
pub struct LocalSyncBackend;

impl LocalSyncBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalSyncBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyncBackend for LocalSyncBackend {
    async fn push(&self, changes: Vec<SyncChange>, _session: &Session) -> Result<Vec<SyncChange>> {
        // Local backend doesn't push anywhere - just return the changes as "synced"
        Ok(changes)
    }

    async fn pull(&self, _entity_type: &str, _since_version: u64, _session: &Session) -> Result<Vec<SyncChange>> {
        // Local backend has no remote to pull from
        Ok(Vec::new())
    }

    async fn get_version(&self, _entity_type: &str, _session: &Session) -> Result<u64> {
        Ok(0)
    }

    async fn health_check(&self) -> Result<bool> {
        // Local is always "healthy"
        Ok(true)
    }

    fn backend_type(&self) -> SyncBackendType {
        SyncBackendType::Local
    }
}

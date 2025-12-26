use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Common trait for all syncable entities
pub trait Syncable: Serialize + for<'de> Deserialize<'de> + Clone + Send + Sync {
    fn id(&self) -> &str;
    fn created_at(&self) -> DateTime<Utc>;
    fn updated_at(&self) -> DateTime<Utc>;
    fn set_updated_at(&mut self, time: DateTime<Utc>);
    fn entity_type() -> &'static str;
    fn deleted(&self) -> bool { false }
}

/// Wrapper for sync metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEntity<T> {
    pub data: T,
    pub sync_version: u64,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub local_only: bool,
}

impl<T: Syncable> SyncEntity<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            sync_version: 0,
            last_synced_at: None,
            local_only: true,
        }
    }

    pub fn mark_synced(&mut self, version: u64) {
        self.sync_version = version;
        self.last_synced_at = Some(Utc::now());
        self.local_only = false;
    }
}

/// Sync operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncOperation {
    Create,
    Update,
    Delete,
}

/// A change to be synced
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncChange {
    pub id: String,
    pub entity_type: String,
    pub operation: SyncOperation,
    pub data: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub version: u64,
}

/// Team/Organization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub owner_id: String,
}

/// Team membership
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub team_id: String,
    pub user_id: String,
    pub role: TeamRole,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TeamRole {
    Owner,
    Admin,
    Member,
}

/// Generate a short ID
pub fn generate_id() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

/// Get current UTC timestamp
pub fn now() -> DateTime<Utc> {
    Utc::now()
}

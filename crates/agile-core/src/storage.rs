use crate::config::Config;
use crate::types::{SyncChange, SyncEntity, SyncOperation, Syncable};
use crate::Result;
use chrono::Utc;
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::PathBuf;

/// Local file storage for entities
pub struct Storage {
    entity_type: String,
    data_dir: PathBuf,
}

impl Storage {
    pub fn new(entity_type: &str) -> Result<Self> {
        let data_dir = Config::init_data_dir(entity_type)?;
        Ok(Self {
            entity_type: entity_type.to_string(),
            data_dir,
        })
    }

    pub fn entity_type(&self) -> &str {
        &self.entity_type
    }

    fn entity_path(&self, id: &str) -> PathBuf {
        self.data_dir.join(format!("{}.yaml", id))
    }

    fn changelog_path(&self) -> PathBuf {
        self.data_dir.join("_changelog.yaml")
    }

    pub fn save<T: Syncable>(&self, entity: &T) -> Result<()> {
        let sync_entity = SyncEntity::new(entity.clone());
        let path = self.entity_path(entity.id());
        let content = serde_yaml::to_string(&sync_entity)?;
        fs::write(&path, content)?;

        // Record change for sync
        self.record_change(entity.id(), SyncOperation::Create, entity)?;

        Ok(())
    }

    pub fn update<T: Syncable>(&self, entity: &T) -> Result<()> {
        let path = self.entity_path(entity.id());

        // Load existing to preserve sync metadata
        let mut sync_entity: SyncEntity<T> = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_yaml::from_str(&content)?
        } else {
            SyncEntity::new(entity.clone())
        };

        sync_entity.data = entity.clone();
        sync_entity.local_only = true;

        let content = serde_yaml::to_string(&sync_entity)?;
        fs::write(&path, content)?;

        // Record change for sync
        self.record_change(entity.id(), SyncOperation::Update, entity)?;

        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.entity_path(id);
        if path.exists() {
            fs::remove_file(&path)?;
        }

        // Record deletion for sync
        self.record_change_raw(id, SyncOperation::Delete, serde_json::Value::Null)?;

        Ok(())
    }

    pub fn load<T: DeserializeOwned>(&self, id: &str) -> Result<T> {
        let path = self.entity_path(id);
        if !path.exists() {
            return Err(crate::Error::NotFound(format!("{} {}", self.entity_type, id)));
        }
        let content = fs::read_to_string(&path)?;
        let sync_entity: SyncEntity<T> = serde_yaml::from_str(&content)?;
        Ok(sync_entity.data)
    }

    pub fn load_with_sync<T: DeserializeOwned>(&self, id: &str) -> Result<SyncEntity<T>> {
        let path = self.entity_path(id);
        if !path.exists() {
            return Err(crate::Error::NotFound(format!("{} {}", self.entity_type, id)));
        }
        let content = fs::read_to_string(&path)?;
        Ok(serde_yaml::from_str(&content)?)
    }

    pub fn load_all<T: DeserializeOwned>(&self) -> Result<Vec<T>> {
        let mut items = Vec::new();

        if self.data_dir.exists() {
            for entry in fs::read_dir(&self.data_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map(|e| e == "yaml").unwrap_or(false) {
                    // Skip changelog
                    if path.file_stem().map(|s| s == "_changelog").unwrap_or(false) {
                        continue;
                    }
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(sync_entity) = serde_yaml::from_str::<SyncEntity<T>>(&content) {
                            items.push(sync_entity.data);
                        }
                    }
                }
            }
        }

        Ok(items)
    }

    pub fn load_all_with_sync<T: DeserializeOwned>(&self) -> Result<Vec<SyncEntity<T>>> {
        let mut items = Vec::new();

        if self.data_dir.exists() {
            for entry in fs::read_dir(&self.data_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map(|e| e == "yaml").unwrap_or(false) {
                    if path.file_stem().map(|s| s == "_changelog").unwrap_or(false) {
                        continue;
                    }
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(sync_entity) = serde_yaml::from_str::<SyncEntity<T>>(&content) {
                            items.push(sync_entity);
                        }
                    }
                }
            }
        }

        Ok(items)
    }

    pub fn exists(&self, id: &str) -> bool {
        self.entity_path(id).exists()
    }

    fn record_change<T: Serialize>(&self, id: &str, operation: SyncOperation, data: &T) -> Result<()> {
        let value = serde_json::to_value(data)?;
        self.record_change_raw(id, operation, value)
    }

    fn record_change_raw(&self, id: &str, operation: SyncOperation, data: serde_json::Value) -> Result<()> {
        let change = SyncChange {
            id: id.to_string(),
            entity_type: self.entity_type.clone(),
            operation,
            data,
            timestamp: Utc::now(),
            version: 0, // Will be set by server
        };

        let path = self.changelog_path();
        let mut changes: Vec<SyncChange> = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_yaml::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        changes.push(change);
        let content = serde_yaml::to_string(&changes)?;
        fs::write(path, content)?;

        Ok(())
    }

    pub fn pending_changes(&self) -> Result<Vec<SyncChange>> {
        let path = self.changelog_path();
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            Ok(serde_yaml::from_str(&content).unwrap_or_default())
        } else {
            Ok(Vec::new())
        }
    }

    pub fn clear_pending_changes(&self) -> Result<()> {
        let path = self.changelog_path();
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn mark_synced<T: Syncable>(&self, id: &str, version: u64) -> Result<()> {
        let path = self.entity_path(id);
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let mut sync_entity: SyncEntity<T> = serde_yaml::from_str(&content)?;
            sync_entity.mark_synced(version);
            let content = serde_yaml::to_string(&sync_entity)?;
            fs::write(path, content)?;
        }
        Ok(())
    }
}

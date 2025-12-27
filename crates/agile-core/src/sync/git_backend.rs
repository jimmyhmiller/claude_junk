use crate::config::SyncBackendType;
use crate::sync::SyncBackend;
use crate::types::SyncChange;
use crate::user::Session;
use crate::{Error, Result};
use async_trait::async_trait;
use std::process::Command;

/// Git-based sync backend - syncs via git push/pull
pub struct GitSyncBackend {
    remote: String,
    branch: String,
}

impl GitSyncBackend {
    pub fn new(remote: String, branch: Option<String>) -> Self {
        Self {
            remote,
            branch: branch.unwrap_or_else(|| "main".to_string()),
        }
    }

    fn git_cmd(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .output()
            .map_err(|e| Error::Sync(format!("Failed to run git: {}", e)))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(Error::Sync(format!("Git error: {}", stderr)))
        }
    }

    fn ensure_git_repo(&self) -> Result<()> {
        // Check if we're in a git repo
        if self.git_cmd(&["rev-parse", "--git-dir"]).is_err() {
            return Err(Error::Sync("Not in a git repository".into()));
        }
        Ok(())
    }

    fn stage_agile_files(&self) -> Result<()> {
        // Stage all files in .agile/
        self.git_cmd(&["add", ".agile/"])?;
        Ok(())
    }

    fn has_staged_changes(&self) -> Result<bool> {
        let output = self.git_cmd(&["diff", "--cached", "--name-only", ".agile/"])?;
        Ok(!output.trim().is_empty())
    }

    fn commit_changes(&self, message: &str) -> Result<()> {
        self.git_cmd(&["commit", "-m", message])?;
        Ok(())
    }

    fn push_changes(&self) -> Result<()> {
        self.git_cmd(&["push", &self.remote, &self.branch])?;
        Ok(())
    }

    fn pull_changes(&self) -> Result<()> {
        self.git_cmd(&["pull", &self.remote, &self.branch])?;
        Ok(())
    }

    fn get_changed_files_since(&self, commit: &str) -> Result<Vec<String>> {
        let output = self.git_cmd(&["diff", "--name-only", commit, "HEAD", "--", ".agile/"])?;
        Ok(output.lines().map(|s| s.to_string()).collect())
    }

    fn get_current_commit(&self) -> Result<String> {
        let output = self.git_cmd(&["rev-parse", "HEAD"])?;
        Ok(output.trim().to_string())
    }

    fn get_remote_commit(&self) -> Result<String> {
        // Fetch first to get latest remote refs
        let _ = self.git_cmd(&["fetch", &self.remote, &self.branch]);
        let output = self.git_cmd(&["rev-parse", &format!("{}/{}", self.remote, self.branch)])?;
        Ok(output.trim().to_string())
    }
}

#[async_trait]
impl SyncBackend for GitSyncBackend {
    async fn push(&self, changes: Vec<SyncChange>, _session: &Session) -> Result<Vec<SyncChange>> {
        self.ensure_git_repo()?;

        // Stage .agile/ files
        self.stage_agile_files()?;

        // Check if there are changes to commit
        if !self.has_staged_changes()? {
            return Ok(changes);
        }

        // Create commit message
        let entity_types: std::collections::HashSet<_> = changes.iter()
            .map(|c| c.entity_type.as_str())
            .collect();
        let message = format!(
            "sync: update {} ({} changes)",
            entity_types.into_iter().collect::<Vec<_>>().join(", "),
            changes.len()
        );

        // Commit and push
        self.commit_changes(&message)?;
        self.push_changes()?;

        Ok(changes)
    }

    async fn pull(&self, entity_type: &str, _since_version: u64, _session: &Session) -> Result<Vec<SyncChange>> {
        self.ensure_git_repo()?;

        // Get current commit before pull
        let before_commit = self.get_current_commit()?;

        // Pull changes
        self.pull_changes()?;

        // Get list of changed files
        let changed_files = self.get_changed_files_since(&before_commit)?;

        // Filter to the entity type we care about
        let entity_dir = format!(".agile/{}/", entity_type);
        let relevant_files: Vec<_> = changed_files
            .into_iter()
            .filter(|f| f.starts_with(&entity_dir) && f.ends_with(".yaml"))
            .collect();

        // For git sync, we don't return individual changes - the files are already updated
        // The caller should reload from disk
        // Return empty but success indicates files may have changed
        if !relevant_files.is_empty() {
            // Return a synthetic change to indicate updates happened
            Ok(vec![SyncChange {
                id: "git-pull".to_string(),
                entity_type: entity_type.to_string(),
                operation: crate::types::SyncOperation::Update,
                data: serde_json::json!({"files_updated": relevant_files.len()}),
                timestamp: chrono::Utc::now(),
                version: 0,
            }])
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_version(&self, _entity_type: &str, _session: &Session) -> Result<u64> {
        // For git, we use commit count as a rough version
        self.ensure_git_repo()?;
        let output = self.git_cmd(&["rev-list", "--count", "HEAD"])?;
        output.trim().parse().map_err(|_| Error::Sync("Invalid commit count".into()))
    }

    async fn health_check(&self) -> Result<bool> {
        // Check if git is available and we're in a repo
        self.ensure_git_repo()?;

        // Check if remote is configured
        let output = self.git_cmd(&["remote", "get-url", &self.remote]);
        Ok(output.is_ok())
    }

    fn backend_type(&self) -> SyncBackendType {
        SyncBackendType::Git
    }
}

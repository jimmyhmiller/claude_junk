use crate::config::SyncBackendType;
use crate::sync::SyncBackend;
use crate::types::SyncChange;
use crate::user::Session;
use crate::{Error, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// HTTP sync backend for SaaS
pub struct HttpSyncBackend {
    client: Client,
    server_url: String,
}

impl HttpSyncBackend {
    pub fn new(server_url: String) -> Self {
        Self {
            client: Client::new(),
            server_url: server_url.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.server_url, path)
    }

    fn auth_header(session: &Session) -> String {
        format!("Bearer {}", session.token.access_token)
    }
}

#[derive(Debug, Serialize)]
struct PushRequest {
    changes: Vec<SyncChange>,
    team_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PushResponse {
    synced: Vec<SyncChange>,
}

#[derive(Debug, Deserialize)]
struct PullResponse {
    changes: Vec<SyncChange>,
}

#[derive(Debug, Deserialize)]
struct VersionResponse {
    version: u64,
}

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
}

#[async_trait]
impl SyncBackend for HttpSyncBackend {
    async fn push(&self, changes: Vec<SyncChange>, session: &Session) -> Result<Vec<SyncChange>> {
        let request = PushRequest {
            changes,
            team_id: session.team_id.clone(),
        };

        let response = self.client
            .post(self.url("/api/v1/sync/push"))
            .header("Authorization", Self::auth_header(session))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Server(format!("{}: {}", status, text)));
        }

        let result: PushResponse = response.json().await?;
        Ok(result.synced)
    }

    async fn pull(&self, entity_type: &str, since_version: u64, session: &Session) -> Result<Vec<SyncChange>> {
        let mut url = self.url("/api/v1/sync/pull");
        url.push_str(&format!("?entity_type={}&since={}", entity_type, since_version));

        if let Some(team_id) = &session.team_id {
            url.push_str(&format!("&team_id={}", team_id));
        }

        let response = self.client
            .get(&url)
            .header("Authorization", Self::auth_header(session))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::Server(format!("{}: {}", status, text)));
        }

        let result: PullResponse = response.json().await?;
        Ok(result.changes)
    }

    async fn get_version(&self, entity_type: &str, session: &Session) -> Result<u64> {
        let mut url = self.url("/api/v1/sync/version");
        url.push_str(&format!("?entity_type={}", entity_type));

        if let Some(team_id) = &session.team_id {
            url.push_str(&format!("&team_id={}", team_id));
        }

        let response = self.client
            .get(&url)
            .header("Authorization", Self::auth_header(session))
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(0);
        }

        let result: VersionResponse = response.json().await?;
        Ok(result.version)
    }

    async fn health_check(&self) -> Result<bool> {
        let response = self.client
            .get(self.url("/health"))
            .send()
            .await?;

        if response.status().is_success() {
            let result: HealthResponse = response.json().await?;
            Ok(result.status == "ok")
        } else {
            Ok(false)
        }
    }

    fn backend_type(&self) -> SyncBackendType {
        SyncBackendType::Http
    }
}

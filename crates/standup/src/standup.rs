use agile_core::{
    config::{Config, SyncBackendType, SyncConfig},
    generate_id, now,
    storage::Storage,
    sync::SyncManager,
    user::{get_local_author, AuthToken, Session, User},
    Error, Result, Syncable,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

const ENTITY_TYPE: &str = "standups";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Standup {
    pub id: String,
    pub author: String,
    pub date: NaiveDate,
    pub yesterday: Vec<String>,
    pub today: Vec<String>,
    pub blockers: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Syncable for Standup {
    fn id(&self) -> &str {
        &self.id
    }

    fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    fn set_updated_at(&mut self, time: DateTime<Utc>) {
        self.updated_at = time;
    }

    fn entity_type() -> &'static str {
        ENTITY_TYPE
    }
}

pub fn init(json: bool) -> Result<()> {
    Config::init()?;
    Config::init_data_dir(ENTITY_TYPE)?;

    if json {
        println!(r#"{{"status": "initialized", "path": ".agile"}}"#);
    } else {
        println!("Initialized standup tracking in .agile/");
    }

    Ok(())
}

pub fn add(yesterday: Vec<String>, today: Vec<String>, blockers: Vec<String>, json: bool) -> Result<()> {
    let storage = Storage::new(ENTITY_TYPE)?;
    let now = now();

    let standup = Standup {
        id: generate_id(),
        author: get_local_author(),
        date: now.date_naive(),
        yesterday,
        today,
        blockers,
        created_at: now,
        updated_at: now,
    };

    storage.save(&standup)?;

    if json {
        println!("{}", serde_json::to_string(&standup).unwrap());
    } else {
        println!("Standup recorded ({})", standup.id);
        if !standup.yesterday.is_empty() {
            println!("  Yesterday: {}", standup.yesterday.join(", "));
        }
        if !standup.today.is_empty() {
            println!("  Today: {}", standup.today.join(", "));
        }
        if !standup.blockers.is_empty() {
            println!("  Blockers: {}", standup.blockers.join(", "));
        }
    }

    Ok(())
}

pub fn list(date: Option<String>, author: Option<String>, limit: usize, json: bool) -> Result<()> {
    let storage = Storage::new(ENTITY_TYPE)?;
    let mut standups: Vec<Standup> = storage.load_all()?;

    // Filter by date
    if let Some(date_str) = date {
        if let Ok(filter_date) = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
            standups.retain(|s| s.date == filter_date);
        }
    }

    // Filter by author
    if let Some(author_filter) = author {
        standups.retain(|s| s.author.to_lowercase().contains(&author_filter.to_lowercase()));
    }

    // Sort by date descending
    standups.sort_by(|a, b| b.date.cmp(&a.date));
    standups.truncate(limit);

    if json {
        println!("{}", serde_json::to_string(&standups).unwrap());
    } else if standups.is_empty() {
        println!("No standups found.");
    } else {
        for standup in standups {
            println!("[{}] {} - {}", standup.date, standup.author, standup.id);
            if !standup.yesterday.is_empty() {
                println!("  Yesterday: {}", standup.yesterday.join("; "));
            }
            if !standup.today.is_empty() {
                println!("  Today: {}", standup.today.join("; "));
            }
            if !standup.blockers.is_empty() {
                println!("  Blockers: {}", standup.blockers.join("; "));
            }
            println!();
        }
    }

    Ok(())
}

pub fn today(json: bool) -> Result<()> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    list(Some(today), None, 50, json)
}

pub async fn login(email: &str, password: &str, server: &str, json: bool) -> Result<()> {
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/api/v1/auth/login", server))
        .json(&serde_json::json!({
            "email": email,
            "password": password
        }))
        .send()
        .await
        .map_err(|e| Error::Http(e))?;

    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(Error::AuthFailed(text));
    }

    #[derive(Deserialize)]
    struct AuthResponse {
        user: UserResponse,
        access_token: String,
        refresh_token: String,
        expires_at: DateTime<Utc>,
    }

    #[derive(Deserialize)]
    struct UserResponse {
        id: String,
        email: String,
        name: String,
        created_at: DateTime<Utc>,
    }

    let auth: AuthResponse = response.json().await.map_err(|e| Error::Http(e))?;

    let session = Session {
        user: User {
            id: auth.user.id,
            email: auth.user.email,
            name: auth.user.name,
            created_at: auth.user.created_at,
        },
        token: AuthToken {
            access_token: auth.access_token,
            refresh_token: Some(auth.refresh_token),
            expires_at: auth.expires_at,
        },
        team_id: None,
    };

    Config::save_session(&session)?;

    // Update config with sync settings
    let mut config = Config::load()?;
    config.sync = SyncConfig {
        enabled: true,
        backend: SyncBackendType::Http,
        server_url: Some(server.to_string()),
        git_remote: None,
        git_branch: None,
        auto_sync: false,
    };
    config.save()?;

    if json {
        println!("{}", serde_json::to_string(&session.user).unwrap());
    } else {
        println!("Logged in as {}", session.user.email);
    }

    Ok(())
}

pub fn logout(json: bool) -> Result<()> {
    Config::clear_session()?;

    if json {
        println!(r#"{{"status": "logged_out"}}"#);
    } else {
        println!("Logged out");
    }

    Ok(())
}

pub async fn sync(json: bool) -> Result<()> {
    let config = Config::load()?;
    let session = Config::load_session()?.ok_or(Error::NotAuthenticated)?;

    let manager = SyncManager::from_config(&config)?;
    let status = manager.sync(ENTITY_TYPE, &session).await?;

    if json {
        println!("{}", serde_json::to_string(&status).unwrap());
    } else {
        println!("Sync complete:");
        println!("  Pushed: {}", status.pushed);
        println!("  Pulled: {}", status.pulled);
        if !status.conflicts.is_empty() {
            println!("  Conflicts: {}", status.conflicts.len());
        }
    }

    Ok(())
}

pub async fn status(json: bool) -> Result<()> {
    let config = Config::load()?;

    let session = Config::load_session()?;
    let storage = Storage::new(ENTITY_TYPE)?;
    let pending = storage.pending_changes()?;

    #[derive(Serialize)]
    struct StatusInfo {
        sync_enabled: bool,
        backend: String,
        logged_in: bool,
        user_email: Option<String>,
        pending_changes: usize,
    }

    let status = StatusInfo {
        sync_enabled: config.sync.enabled,
        backend: format!("{:?}", config.sync.backend),
        logged_in: session.is_some(),
        user_email: session.as_ref().map(|s| s.user.email.clone()),
        pending_changes: pending.len(),
    };

    if json {
        println!("{}", serde_json::to_string(&status).unwrap());
    } else {
        println!("Sync status:");
        println!("  Enabled: {}", status.sync_enabled);
        println!("  Backend: {}", status.backend);
        if let Some(email) = &status.user_email {
            println!("  Logged in as: {}", email);
        } else {
            println!("  Not logged in");
        }
        println!("  Pending changes: {}", status.pending_changes);
    }

    Ok(())
}

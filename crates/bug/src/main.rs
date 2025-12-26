use agile_core::{
    config::Config, generate_id, now, storage::Storage, sync::SyncManager,
    user::get_local_author, Error, Result, Syncable,
};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

const ENTITY_TYPE: &str = "bugs";

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BugStatus {
    Open,
    InProgress,
    Fixed,
    Closed,
    WontFix,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bug {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Priority,
    pub status: BugStatus,
    pub reporter: String,
    pub assignee: Option<String>,
    pub labels: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Syncable for Bug {
    fn id(&self) -> &str { &self.id }
    fn created_at(&self) -> DateTime<Utc> { self.created_at }
    fn updated_at(&self) -> DateTime<Utc> { self.updated_at }
    fn set_updated_at(&mut self, time: DateTime<Utc>) { self.updated_at = time; }
    fn entity_type() -> &'static str { ENTITY_TYPE }
}

#[derive(Parser)]
#[command(name = "bug")]
#[command(about = "Bug tracking for teams")]
#[command(version)]
pub struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    New {
        title: String,
        #[arg(short, long)]
        description: Option<String>,
        #[arg(short, long, default_value = "medium")]
        priority: Priority,
        #[arg(short, long)]
        assignee: Option<String>,
        #[arg(short, long)]
        label: Vec<String>,
    },
    List {
        #[arg(short, long)]
        status: Option<BugStatus>,
        #[arg(short, long)]
        priority: Option<Priority>,
    },
    Show { id: String },
    Update {
        id: String,
        #[arg(short, long)]
        status: Option<BugStatus>,
        #[arg(short, long)]
        priority: Option<Priority>,
        #[arg(short, long)]
        assignee: Option<String>,
    },
    Close { id: String },
    Sync,
    Status,
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            Config::init()?;
            Config::init_data_dir(ENTITY_TYPE)?;
            if cli.json {
                println!(r#"{{"status": "initialized"}}"#);
            } else {
                println!("Initialized bug tracking");
            }
        }
        Commands::New { title, description, priority, assignee, label } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let now = now();
            let bug = Bug {
                id: generate_id(),
                title: title.clone(),
                description,
                priority,
                status: BugStatus::Open,
                reporter: get_local_author(),
                assignee,
                labels: label,
                created_at: now,
                updated_at: now,
            };
            storage.save(&bug)?;
            if cli.json {
                println!("{}", serde_json::to_string(&bug)?);
            } else {
                println!("Bug created: {} ({})", bug.id, title);
            }
        }
        Commands::List { status, priority } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut bugs: Vec<Bug> = storage.load_all()?;
            if let Some(s) = status { bugs.retain(|b| b.status == s); }
            if let Some(p) = priority { bugs.retain(|b| b.priority == p); }
            if cli.json {
                println!("{}", serde_json::to_string(&bugs)?);
            } else {
                for bug in bugs {
                    println!("[{}] {:?} {:?} - {}", bug.id, bug.status, bug.priority, bug.title);
                }
            }
        }
        Commands::Show { id } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let bug: Bug = storage.load(&id)?;
            if cli.json {
                println!("{}", serde_json::to_string(&bug)?);
            } else {
                println!("Bug: {} - {}", bug.id, bug.title);
                println!("Status: {:?}, Priority: {:?}", bug.status, bug.priority);
            }
        }
        Commands::Update { id, status, priority, assignee } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut bug: Bug = storage.load(&id)?;
            if let Some(s) = status { bug.status = s; }
            if let Some(p) = priority { bug.priority = p; }
            if assignee.is_some() { bug.assignee = assignee; }
            bug.updated_at = now();
            storage.update(&bug)?;
            if cli.json {
                println!("{}", serde_json::to_string(&bug)?);
            } else {
                println!("Bug {} updated", id);
            }
        }
        Commands::Close { id } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut bug: Bug = storage.load(&id)?;
            bug.status = BugStatus::Closed;
            bug.updated_at = now();
            storage.update(&bug)?;
            println!("Bug {} closed", id);
        }
        Commands::Sync => {
            let config = Config::load()?;
            let session = Config::load_session()?.ok_or(Error::NotAuthenticated)?;
            let manager = SyncManager::from_config(&config)?;
            let status = manager.sync(ENTITY_TYPE, &session).await?;
            println!("Synced: {} pushed, {} pulled", status.pushed, status.pulled);
        }
        Commands::Status => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let pending = storage.pending_changes()?;
            println!("Pending changes: {}", pending.len());
        }
    }
    Ok(())
}

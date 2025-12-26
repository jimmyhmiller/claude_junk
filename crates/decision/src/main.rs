use agile_core::{
    config::Config, generate_id, now, storage::Storage, sync::SyncManager,
    user::get_local_author, Error, Syncable,
};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

const ENTITY_TYPE: &str = "decisions";

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DecisionStatus { Proposed, Accepted, Deprecated, Superseded }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub title: String,
    pub context: Option<String>,
    pub decision: String,
    pub consequences: Vec<String>,
    pub status: DecisionStatus,
    pub author: String,
    pub superseded_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Syncable for Decision {
    fn id(&self) -> &str { &self.id }
    fn created_at(&self) -> DateTime<Utc> { self.created_at }
    fn updated_at(&self) -> DateTime<Utc> { self.updated_at }
    fn set_updated_at(&mut self, time: DateTime<Utc>) { self.updated_at = time; }
    fn entity_type() -> &'static str { ENTITY_TYPE }
}

#[derive(Parser)]
#[command(name = "decision")]
#[command(about = "Architectural decision records")]
pub struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Record { title: String, #[arg(short, long)] decision: String, #[arg(short, long)] context: Option<String> },
    List { #[arg(short, long)] status: Option<DecisionStatus> },
    Show { id: String },
    Accept { id: String },
    Deprecate { id: String },
    Sync,
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            Config::init()?;
            Config::init_data_dir(ENTITY_TYPE)?;
            println!("Initialized decision records");
        }
        Commands::Record { title, decision, context } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let now = now();
            let dec = Decision {
                id: generate_id(), title: title.clone(), context, decision,
                consequences: vec![], status: DecisionStatus::Proposed,
                author: get_local_author(), superseded_by: None,
                created_at: now, updated_at: now,
            };
            storage.save(&dec)?;
            if cli.json { println!("{}", serde_json::to_string(&dec)?); }
            else { println!("Decision recorded: {} - {}", dec.id, title); }
        }
        Commands::List { status } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut decisions: Vec<Decision> = storage.load_all()?;
            if let Some(s) = status { decisions.retain(|d| d.status == s); }
            if cli.json { println!("{}", serde_json::to_string(&decisions)?); }
            else { for d in decisions { println!("[{}] {:?} - {}", d.id, d.status, d.title); } }
        }
        Commands::Show { id } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let dec: Decision = storage.load(&id)?;
            if cli.json { println!("{}", serde_json::to_string(&dec)?); }
            else { println!("{}: {}\n{}", dec.id, dec.title, dec.decision); }
        }
        Commands::Accept { id } => update_status(&id, DecisionStatus::Accepted)?,
        Commands::Deprecate { id } => update_status(&id, DecisionStatus::Deprecated)?,
        Commands::Sync => {
            let config = Config::load()?;
            let session = Config::load_session()?.ok_or(Error::NotAuthenticated)?;
            let manager = SyncManager::from_config(&config)?;
            let status = manager.sync(ENTITY_TYPE, &session).await?;
            println!("Synced: {} pushed, {} pulled", status.pushed, status.pulled);
        }
    }
    Ok(())
}

fn update_status(id: &str, status: DecisionStatus) -> agile_core::Result<()> {
    let storage = Storage::new(ENTITY_TYPE)?;
    let mut dec: Decision = storage.load(id)?;
    dec.status = status;
    dec.updated_at = now();
    storage.update(&dec)?;
    println!("Decision {} status: {:?}", id, dec.status);
    Ok(())
}

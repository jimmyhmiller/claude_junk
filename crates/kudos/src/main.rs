use agile_core::{
    config::Config, generate_id, now, storage::Storage, sync::SyncManager,
    user::get_local_author, Error, Syncable,
};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const ENTITY_TYPE: &str = "kudos";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kudos {
    pub id: String,
    pub from: String,
    pub to: String,
    pub message: String,
    pub category: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Syncable for Kudos {
    fn id(&self) -> &str { &self.id }
    fn created_at(&self) -> DateTime<Utc> { self.created_at }
    fn updated_at(&self) -> DateTime<Utc> { self.updated_at }
    fn set_updated_at(&mut self, time: DateTime<Utc>) { self.updated_at = time; }
    fn entity_type() -> &'static str { ENTITY_TYPE }
}

#[derive(Parser)]
#[command(name = "kudos")]
#[command(about = "Team kudos and appreciation")]
pub struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Give { to: String, message: String, #[arg(short, long)] category: Option<String> },
    List { #[arg(short, long)] to: Option<String>, #[arg(short, long)] from: Option<String>, #[arg(short = 'n', long, default_value = "20")] limit: usize },
    Leaderboard { #[arg(short = 'n', long, default_value = "10")] top: usize },
    Sync,
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            Config::init()?;
            Config::init_data_dir(ENTITY_TYPE)?;
            println!("Initialized kudos");
        }
        Commands::Give { to, message, category } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let now = now();
            let kudos = Kudos {
                id: generate_id(), from: get_local_author(), to: to.clone(),
                message: message.clone(), category, created_at: now, updated_at: now,
            };
            storage.save(&kudos)?;
            if cli.json { println!("{}", serde_json::to_string(&kudos)?); }
            else { println!("Kudos sent to {}!\n  \"{}\"", to, message); }
        }
        Commands::List { to, from, limit } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut kudos_list: Vec<Kudos> = storage.load_all()?;
            if let Some(t) = to { kudos_list.retain(|k| k.to.to_lowercase().contains(&t.to_lowercase())); }
            if let Some(f) = from { kudos_list.retain(|k| k.from.to_lowercase().contains(&f.to_lowercase())); }
            kudos_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            kudos_list.truncate(limit);
            if cli.json { println!("{}", serde_json::to_string(&kudos_list)?); }
            else { for k in kudos_list { println!("{} -> {}: \"{}\"", k.from, k.to, k.message); } }
        }
        Commands::Leaderboard { top } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let kudos_list: Vec<Kudos> = storage.load_all()?;
            let mut counts: HashMap<String, usize> = HashMap::new();
            for k in &kudos_list { *counts.entry(k.to.clone()).or_insert(0) += 1; }
            let mut leaderboard: Vec<_> = counts.into_iter().collect();
            leaderboard.sort_by(|a, b| b.1.cmp(&a.1));
            leaderboard.truncate(top);
            if cli.json {
                let result: Vec<_> = leaderboard.iter().map(|(n, c)| serde_json::json!({"name": n, "count": c})).collect();
                println!("{}", serde_json::to_string(&result)?);
            } else {
                println!("=== KUDOS LEADERBOARD ===");
                for (i, (name, count)) in leaderboard.iter().enumerate() {
                    let medal = match i { 0 => " [1st]", 1 => " [2nd]", 2 => " [3rd]", _ => "" };
                    println!("  {}: {} kudos{}", name, count, medal);
                }
            }
        }
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

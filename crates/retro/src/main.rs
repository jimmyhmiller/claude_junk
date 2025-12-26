use agile_core::{
    config::Config, generate_id, now, storage::Storage, sync::SyncManager,
    user::get_local_author, Error, Result, Syncable,
};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

const ENTITY_TYPE: &str = "retros";

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Category { Good, Bad, Action }

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ActionStatus { Pending, InProgress, Done }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetroItem {
    pub id: String,
    pub category: Category,
    pub content: String,
    pub author: String,
    pub sprint: Option<String>,
    pub action_status: Option<ActionStatus>,
    pub action_owner: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Syncable for RetroItem {
    fn id(&self) -> &str { &self.id }
    fn created_at(&self) -> DateTime<Utc> { self.created_at }
    fn updated_at(&self) -> DateTime<Utc> { self.updated_at }
    fn set_updated_at(&mut self, time: DateTime<Utc>) { self.updated_at = time; }
    fn entity_type() -> &'static str { ENTITY_TYPE }
}

#[derive(Parser)]
#[command(name = "retro")]
#[command(about = "Retrospective tracking for teams")]
pub struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Good { content: String, #[arg(short, long)] sprint: Option<String> },
    Bad { content: String, #[arg(short, long)] sprint: Option<String> },
    Action { content: String, #[arg(short, long)] owner: Option<String>, #[arg(short, long)] sprint: Option<String> },
    List { #[arg(short, long)] category: Option<Category>, #[arg(short, long)] sprint: Option<String> },
    Done { id: String },
    Sync,
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            Config::init()?;
            Config::init_data_dir(ENTITY_TYPE)?;
            println!("Initialized retro tracking");
        }
        Commands::Good { content, sprint } => add_item(Category::Good, content, sprint, None, cli.json)?,
        Commands::Bad { content, sprint } => add_item(Category::Bad, content, sprint, None, cli.json)?,
        Commands::Action { content, owner, sprint } => add_item(Category::Action, content, sprint, owner, cli.json)?,
        Commands::List { category, sprint } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut items: Vec<RetroItem> = storage.load_all()?;
            if let Some(c) = category { items.retain(|i| i.category == c); }
            if let Some(s) = sprint { items.retain(|i| i.sprint.as_ref() == Some(&s)); }
            if cli.json {
                println!("{}", serde_json::to_string(&items)?);
            } else {
                for item in items {
                    let mark = match item.category { Category::Good => "+", Category::Bad => "-", Category::Action => "!" };
                    println!("[{}] {} {}", mark, item.id, item.content);
                }
            }
        }
        Commands::Done { id } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut item: RetroItem = storage.load(&id)?;
            item.action_status = Some(ActionStatus::Done);
            item.updated_at = now();
            storage.update(&item)?;
            println!("Action {} marked done", id);
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

fn add_item(category: Category, content: String, sprint: Option<String>, owner: Option<String>, json: bool) -> Result<()> {
    let storage = Storage::new(ENTITY_TYPE)?;
    let now = now();
    let item = RetroItem {
        id: generate_id(),
        category: category.clone(),
        content: content.clone(),
        author: get_local_author(),
        sprint,
        action_status: if category == Category::Action { Some(ActionStatus::Pending) } else { None },
        action_owner: owner,
        created_at: now,
        updated_at: now,
    };
    storage.save(&item)?;
    if json {
        println!("{}", serde_json::to_string(&item)?);
    } else {
        println!("[{}] {}", item.id, content);
    }
    Ok(())
}

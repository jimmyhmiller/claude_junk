use agile_core::{
    config::Config, generate_id, now, storage::Storage, sync::SyncManager,
    user::get_local_author, Error, Syncable,
};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

const ENTITY_TYPE: &str = "tasks";

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus { Todo, InProgress, Review, Done }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub assignee: Option<String>,
    pub labels: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Syncable for Task {
    fn id(&self) -> &str { &self.id }
    fn created_at(&self) -> DateTime<Utc> { self.created_at }
    fn updated_at(&self) -> DateTime<Utc> { self.updated_at }
    fn set_updated_at(&mut self, time: DateTime<Utc>) { self.updated_at = time; }
    fn entity_type() -> &'static str { ENTITY_TYPE }
}

#[derive(Parser)]
#[command(name = "task")]
#[command(about = "Task management for teams")]
pub struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Add { title: String, #[arg(short, long)] description: Option<String>, #[arg(short, long)] assignee: Option<String> },
    List { #[arg(short, long)] status: Option<TaskStatus> },
    Board,
    Start { id: String },
    Done { id: String },
    Move { id: String, status: TaskStatus },
    Assign { id: String, assignee: String },
    Sync,
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            Config::init()?;
            Config::init_data_dir(ENTITY_TYPE)?;
            println!("Initialized task tracking");
        }
        Commands::Add { title, description, assignee } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let now = now();
            let task = Task {
                id: generate_id(), title: title.clone(), description, status: TaskStatus::Todo,
                assignee, labels: vec![], created_at: now, updated_at: now,
            };
            storage.save(&task)?;
            if cli.json { println!("{}", serde_json::to_string(&task)?); }
            else { println!("Task created: {} ({})", task.id, title); }
        }
        Commands::List { status } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut tasks: Vec<Task> = storage.load_all()?;
            if let Some(s) = status { tasks.retain(|t| t.status == s); }
            if cli.json { println!("{}", serde_json::to_string(&tasks)?); }
            else { for t in tasks { println!("[{}] {:?} - {}", t.id, t.status, t.title); } }
        }
        Commands::Board => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let tasks: Vec<Task> = storage.load_all()?;
            println!("TODO: {}", tasks.iter().filter(|t| t.status == TaskStatus::Todo).count());
            println!("IN PROGRESS: {}", tasks.iter().filter(|t| t.status == TaskStatus::InProgress).count());
            println!("REVIEW: {}", tasks.iter().filter(|t| t.status == TaskStatus::Review).count());
            println!("DONE: {}", tasks.iter().filter(|t| t.status == TaskStatus::Done).count());
        }
        Commands::Start { id } => move_task(&id, TaskStatus::InProgress, cli.json)?,
        Commands::Done { id } => move_task(&id, TaskStatus::Done, cli.json)?,
        Commands::Move { id, status } => move_task(&id, status, cli.json)?,
        Commands::Assign { id, assignee } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut task: Task = storage.load(&id)?;
            task.assignee = Some(assignee.clone());
            task.updated_at = now();
            storage.update(&task)?;
            println!("Task {} assigned to {}", id, assignee);
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

fn move_task(id: &str, status: TaskStatus, json: bool) -> agile_core::Result<()> {
    let storage = Storage::new(ENTITY_TYPE)?;
    let mut task: Task = storage.load(id)?;
    task.status = status;
    task.updated_at = now();
    storage.update(&task)?;
    if json { println!("{}", serde_json::to_string(&task)?); }
    else { println!("Task {} moved to {:?}", id, task.status); }
    Ok(())
}

use agile_core::{
    config::Config, generate_id, now, storage::Storage, sync::SyncManager,
    user::get_local_author, Error, Syncable,
};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

const ENTITY_TYPE: &str = "notes";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub author: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Syncable for Note {
    fn id(&self) -> &str { &self.id }
    fn created_at(&self) -> DateTime<Utc> { self.created_at }
    fn updated_at(&self) -> DateTime<Utc> { self.updated_at }
    fn set_updated_at(&mut self, time: DateTime<Utc>) { self.updated_at = time; }
    fn entity_type() -> &'static str { ENTITY_TYPE }
}

#[derive(Parser)]
#[command(name = "note")]
#[command(about = "Note taking for teams")]
pub struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Add { title: String, #[arg(short, long)] content: String, #[arg(short, long)] tag: Vec<String> },
    List { #[arg(short, long)] tag: Option<String>, #[arg(short, long)] search: Option<String> },
    Show { id: String },
    Append { id: String, content: String },
    Delete { id: String },
    Sync,
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            Config::init()?;
            Config::init_data_dir(ENTITY_TYPE)?;
            println!("Initialized notes");
        }
        Commands::Add { title, content, tag } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let now = now();
            let note = Note {
                id: generate_id(), title: title.clone(), content, author: get_local_author(),
                tags: tag, created_at: now, updated_at: now,
            };
            storage.save(&note)?;
            if cli.json { println!("{}", serde_json::to_string(&note)?); }
            else { println!("Note created: {} - {}", note.id, title); }
        }
        Commands::List { tag, search } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut notes: Vec<Note> = storage.load_all()?;
            if let Some(t) = tag { notes.retain(|n| n.tags.contains(&t)); }
            if let Some(s) = search {
                let s = s.to_lowercase();
                notes.retain(|n| n.title.to_lowercase().contains(&s) || n.content.to_lowercase().contains(&s));
            }
            if cli.json { println!("{}", serde_json::to_string(&notes)?); }
            else { for n in notes { println!("[{}] {}", n.id, n.title); } }
        }
        Commands::Show { id } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let note: Note = storage.load(&id)?;
            if cli.json { println!("{}", serde_json::to_string(&note)?); }
            else { println!("{}: {}\n\n{}", note.id, note.title, note.content); }
        }
        Commands::Append { id, content } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut note: Note = storage.load(&id)?;
            note.content = format!("{}\n\n{}", note.content, content);
            note.updated_at = now();
            storage.update(&note)?;
            println!("Note {} updated", id);
        }
        Commands::Delete { id } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            storage.delete(&id)?;
            println!("Note {} deleted", id);
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

use agile_core::{
    config::Config, generate_id, now, storage::Storage, sync::SyncManager,
    user::get_local_author, Error, Syncable,
};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

const ENTITY_TYPE: &str = "reviews";

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus { Pending, InReview, Approved, ChangesRequested, Merged, Closed }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    pub author: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub id: String,
    pub branch: String,
    pub title: String,
    pub description: Option<String>,
    pub author: String,
    pub reviewers: Vec<String>,
    pub status: ReviewStatus,
    pub comments: Vec<ReviewComment>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Syncable for Review {
    fn id(&self) -> &str { &self.id }
    fn created_at(&self) -> DateTime<Utc> { self.created_at }
    fn updated_at(&self) -> DateTime<Utc> { self.updated_at }
    fn set_updated_at(&mut self, time: DateTime<Utc>) { self.updated_at = time; }
    fn entity_type() -> &'static str { ENTITY_TYPE }
}

fn get_current_branch() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[derive(Parser)]
#[command(name = "review")]
#[command(about = "Code review tracking")]
pub struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Request { title: String, #[arg(short, long)] branch: Option<String>, #[arg(short, long)] description: Option<String>, #[arg(short, long)] reviewer: Vec<String> },
    List { #[arg(short, long)] status: Option<ReviewStatus> },
    Show { id: String },
    Approve { id: String, #[arg(short, long)] comment: Option<String> },
    RequestChanges { id: String, comment: String },
    Comment { id: String, content: String },
    Merge { id: String },
    Close { id: String },
    Sync,
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            Config::init()?;
            Config::init_data_dir(ENTITY_TYPE)?;
            println!("Initialized review tracking");
        }
        Commands::Request { title, branch, description, reviewer } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let now = now();
            let review = Review {
                id: generate_id(), branch: branch.unwrap_or_else(get_current_branch),
                title: title.clone(), description, author: get_local_author(),
                reviewers: reviewer, status: ReviewStatus::Pending, comments: vec![],
                created_at: now, updated_at: now,
            };
            storage.save(&review)?;
            if cli.json { println!("{}", serde_json::to_string(&review)?); }
            else { println!("Review requested: {} - {}", review.id, title); }
        }
        Commands::List { status } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut reviews: Vec<Review> = storage.load_all()?;
            if let Some(s) = status { reviews.retain(|r| r.status == s); }
            if cli.json { println!("{}", serde_json::to_string(&reviews)?); }
            else { for r in reviews { println!("[{}] {:?} {} - {}", r.id, r.status, r.branch, r.title); } }
        }
        Commands::Show { id } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let review: Review = storage.load(&id)?;
            if cli.json { println!("{}", serde_json::to_string(&review)?); }
            else { println!("{}: {} ({})\nStatus: {:?}", review.id, review.title, review.branch, review.status); }
        }
        Commands::Approve { id, comment } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut review: Review = storage.load(&id)?;
            review.status = ReviewStatus::Approved;
            review.updated_at = now();
            if let Some(c) = comment {
                review.comments.push(ReviewComment { author: get_local_author(), content: format!("Approved: {}", c), created_at: now() });
            }
            storage.update(&review)?;
            println!("Review {} approved", id);
        }
        Commands::RequestChanges { id, comment } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut review: Review = storage.load(&id)?;
            review.status = ReviewStatus::ChangesRequested;
            review.updated_at = now();
            review.comments.push(ReviewComment { author: get_local_author(), content: comment, created_at: now() });
            storage.update(&review)?;
            println!("Changes requested on review {}", id);
        }
        Commands::Comment { id, content } => {
            let storage = Storage::new(ENTITY_TYPE)?;
            let mut review: Review = storage.load(&id)?;
            review.updated_at = now();
            review.comments.push(ReviewComment { author: get_local_author(), content, created_at: now() });
            storage.update(&review)?;
            println!("Comment added to review {}", id);
        }
        Commands::Merge { id } => update_status(&id, ReviewStatus::Merged)?,
        Commands::Close { id } => update_status(&id, ReviewStatus::Closed)?,
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

fn update_status(id: &str, status: ReviewStatus) -> agile_core::Result<()> {
    let storage = Storage::new(ENTITY_TYPE)?;
    let mut review: Review = storage.load(id)?;
    review.status = status;
    review.updated_at = now();
    storage.update(&review)?;
    println!("Review {} status: {:?}", id, review.status);
    Ok(())
}

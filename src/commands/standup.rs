use chrono::{Local, NaiveDate};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{self, Result};

#[derive(Debug, Serialize, Deserialize)]
pub struct Standup {
    pub id: String,
    pub author: String,
    pub date: NaiveDate,
    pub yesterday: Vec<String>,
    pub today: Vec<String>,
    pub blockers: Vec<String>,
    pub created_at: String,
}

#[derive(Subcommand)]
pub enum StandupAction {
    /// Add a new standup entry
    Add {
        /// What you did yesterday
        #[arg(short = 'y', long)]
        yesterday: Vec<String>,
        /// What you're doing today
        #[arg(short = 't', long)]
        today: Vec<String>,
        /// Any blockers
        #[arg(short = 'b', long)]
        blocker: Vec<String>,
    },
    /// List standup entries
    List {
        /// Filter by date (YYYY-MM-DD)
        #[arg(short, long)]
        date: Option<String>,
        /// Filter by author
        #[arg(short, long)]
        author: Option<String>,
        /// Number of entries to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },
    /// Show today's standups
    Today,
}

pub fn run(action: StandupAction, json: bool) -> Result<()> {
    match action {
        StandupAction::Add { yesterday, today, blocker } => {
            add_standup(yesterday, today, blocker, json)
        }
        StandupAction::List { date, author, limit } => {
            list_standups(date, author, limit, json)
        }
        StandupAction::Today => {
            let today = Local::now().format("%Y-%m-%d").to_string();
            list_standups(Some(today), None, 50, json)
        }
    }
}

fn add_standup(yesterday: Vec<String>, today: Vec<String>, blockers: Vec<String>, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;

    let standup = Standup {
        id: Uuid::new_v4().to_string()[..8].to_string(),
        author: storage::get_author(),
        date: Local::now().date_naive(),
        yesterday,
        today,
        blockers,
        created_at: Local::now().to_rfc3339(),
    };

    let filename = format!("{}_{}.yaml", standup.date, standup.id);
    let path = dir.join("standups").join(&filename);
    storage::save_yaml(&path, &standup)?;

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

fn list_standups(date: Option<String>, author: Option<String>, limit: usize, json: bool) -> Result<()> {
    let mut standups: Vec<Standup> = storage::load_all_from_dir::<Standup>("standups")?
        .into_iter()
        .map(|(_, s)| s)
        .collect();

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

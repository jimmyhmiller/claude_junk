use chrono::Local;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{self, Result};

#[derive(Debug, Serialize, Deserialize)]
pub struct Kudos {
    pub id: String,
    pub from: String,
    pub to: String,
    pub message: String,
    pub category: Option<String>,
    pub created_at: String,
}

#[derive(Subcommand)]
pub enum KudosAction {
    /// Give kudos to a team member
    Give {
        /// Who you're giving kudos to
        to: String,
        /// The kudos message
        message: String,
        /// Category (e.g., "teamwork", "innovation", "helpfulness")
        #[arg(short, long)]
        category: Option<String>,
    },
    /// List kudos
    List {
        /// Filter by recipient
        #[arg(short, long)]
        to: Option<String>,
        /// Filter by sender
        #[arg(short, long)]
        from: Option<String>,
        /// Number of entries to show
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },
    /// Show kudos leaderboard
    Leaderboard {
        /// Number of top recipients to show
        #[arg(short = 'n', long, default_value = "10")]
        top: usize,
    },
}

pub fn run(action: KudosAction, json: bool) -> Result<()> {
    match action {
        KudosAction::Give { to, message, category } => give_kudos(to, message, category, json),
        KudosAction::List { to, from, limit } => list_kudos(to, from, limit, json),
        KudosAction::Leaderboard { top } => show_leaderboard(top, json),
    }
}

fn give_kudos(to: String, message: String, category: Option<String>, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;

    let kudos = Kudos {
        id: Uuid::new_v4().to_string()[..8].to_string(),
        from: storage::get_author(),
        to,
        message,
        category,
        created_at: Local::now().to_rfc3339(),
    };

    let path = dir.join("kudos").join(format!("{}.yaml", kudos.id));
    storage::save_yaml(&path, &kudos)?;

    if json {
        println!("{}", serde_json::to_string(&kudos).unwrap());
    } else {
        println!("Kudos sent to {}!", kudos.to);
        println!("  \"{}\"", kudos.message);
    }

    Ok(())
}

fn list_kudos(to: Option<String>, from: Option<String>, limit: usize, json: bool) -> Result<()> {
    let mut kudos_list: Vec<Kudos> = storage::load_all_from_dir::<Kudos>("kudos")?
        .into_iter()
        .map(|(_, k)| k)
        .collect();

    if let Some(t) = to {
        kudos_list.retain(|k| k.to.to_lowercase().contains(&t.to_lowercase()));
    }

    if let Some(f) = from {
        kudos_list.retain(|k| k.from.to_lowercase().contains(&f.to_lowercase()));
    }

    kudos_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    kudos_list.truncate(limit);

    if json {
        println!("{}", serde_json::to_string(&kudos_list).unwrap());
    } else if kudos_list.is_empty() {
        println!("No kudos found.");
    } else {
        for k in kudos_list {
            let cat = k.category.map(|c| format!(" [{}]", c)).unwrap_or_default();
            println!("{} -> {}{}", k.from, k.to, cat);
            println!("  \"{}\"", k.message);
            println!();
        }
    }

    Ok(())
}

fn show_leaderboard(top: usize, json: bool) -> Result<()> {
    let kudos_list: Vec<Kudos> = storage::load_all_from_dir::<Kudos>("kudos")?
        .into_iter()
        .map(|(_, k)| k)
        .collect();

    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for k in &kudos_list {
        *counts.entry(k.to.clone()).or_insert(0) += 1;
    }

    let mut leaderboard: Vec<_> = counts.into_iter().collect();
    leaderboard.sort_by(|a, b| b.1.cmp(&a.1));
    leaderboard.truncate(top);

    if json {
        let result: Vec<_> = leaderboard.iter()
            .map(|(name, count)| serde_json::json!({"name": name, "count": count}))
            .collect();
        println!("{}", serde_json::to_string(&result).unwrap());
    } else if leaderboard.is_empty() {
        println!("No kudos yet.");
    } else {
        println!("=== KUDOS LEADERBOARD ===\n");
        for (i, (name, count)) in leaderboard.iter().enumerate() {
            let medal = match i {
                0 => " [1st]",
                1 => " [2nd]",
                2 => " [3rd]",
                _ => "",
            };
            println!("  {}: {} kudos{}", name, count, medal);
        }
    }

    Ok(())
}

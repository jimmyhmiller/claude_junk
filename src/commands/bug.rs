use chrono::Local;
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{self, Result};

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

#[derive(Debug, Serialize, Deserialize)]
pub struct Bug {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Priority,
    pub status: BugStatus,
    pub reporter: String,
    pub assignee: Option<String>,
    pub labels: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Subcommand)]
pub enum BugAction {
    /// Report a new bug
    New {
        /// Bug title/summary
        title: String,
        /// Detailed description
        #[arg(short, long)]
        description: Option<String>,
        /// Priority level
        #[arg(short, long, default_value = "medium")]
        priority: Priority,
        /// Assign to someone
        #[arg(short, long)]
        assignee: Option<String>,
        /// Labels/tags
        #[arg(short, long)]
        label: Vec<String>,
    },
    /// List bugs
    List {
        /// Filter by status
        #[arg(short, long)]
        status: Option<BugStatus>,
        /// Filter by priority
        #[arg(short, long)]
        priority: Option<Priority>,
        /// Filter by assignee
        #[arg(short, long)]
        assignee: Option<String>,
    },
    /// Update bug status
    Update {
        /// Bug ID
        id: String,
        /// New status
        #[arg(short, long)]
        status: Option<BugStatus>,
        /// New priority
        #[arg(short, long)]
        priority: Option<Priority>,
        /// Assign to someone
        #[arg(short, long)]
        assignee: Option<String>,
    },
    /// Show bug details
    Show {
        /// Bug ID
        id: String,
    },
    /// Close a bug
    Close {
        /// Bug ID
        id: String,
    },
}

pub fn run(action: BugAction, json: bool) -> Result<()> {
    match action {
        BugAction::New { title, description, priority, assignee, label } => {
            new_bug(title, description, priority, assignee, label, json)
        }
        BugAction::List { status, priority, assignee } => {
            list_bugs(status, priority, assignee, json)
        }
        BugAction::Update { id, status, priority, assignee } => {
            update_bug(id, status, priority, assignee, json)
        }
        BugAction::Show { id } => show_bug(id, json),
        BugAction::Close { id } => close_bug(id, json),
    }
}

fn new_bug(title: String, description: Option<String>, priority: Priority, assignee: Option<String>, labels: Vec<String>, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let now = Local::now().to_rfc3339();

    let bug = Bug {
        id: Uuid::new_v4().to_string()[..8].to_string(),
        title,
        description,
        priority,
        status: BugStatus::Open,
        reporter: storage::get_author(),
        assignee,
        labels,
        created_at: now.clone(),
        updated_at: now,
    };

    let path = dir.join("bugs").join(format!("{}.yaml", bug.id));
    storage::save_yaml(&path, &bug)?;

    if json {
        println!("{}", serde_json::to_string(&bug).unwrap());
    } else {
        println!("Bug created: {} ({})", bug.id, bug.title);
    }

    Ok(())
}

fn list_bugs(status: Option<BugStatus>, priority: Option<Priority>, assignee: Option<String>, json: bool) -> Result<()> {
    let mut bugs: Vec<Bug> = storage::load_all_from_dir::<Bug>("bugs")?
        .into_iter()
        .map(|(_, b)| b)
        .collect();

    if let Some(s) = status {
        bugs.retain(|b| b.status == s);
    }
    if let Some(p) = priority {
        bugs.retain(|b| b.priority == p);
    }
    if let Some(a) = assignee {
        bugs.retain(|b| b.assignee.as_ref().map(|x| x.to_lowercase().contains(&a.to_lowercase())).unwrap_or(false));
    }

    // Sort by priority (critical first), then by date
    bugs.sort_by(|a, b| {
        let priority_order = |p: &Priority| match p {
            Priority::Critical => 0,
            Priority::High => 1,
            Priority::Medium => 2,
            Priority::Low => 3,
        };
        priority_order(&a.priority).cmp(&priority_order(&b.priority))
            .then_with(|| b.created_at.cmp(&a.created_at))
    });

    if json {
        println!("{}", serde_json::to_string(&bugs).unwrap());
    } else if bugs.is_empty() {
        println!("No bugs found.");
    } else {
        for bug in bugs {
            let assignee_str = bug.assignee.as_deref().unwrap_or("unassigned");
            println!("[{}] {:?} {:?} - {} ({})",
                bug.id, bug.status, bug.priority, bug.title, assignee_str);
        }
    }

    Ok(())
}

fn update_bug(id: String, status: Option<BugStatus>, priority: Option<Priority>, assignee: Option<String>, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("bugs").join(format!("{}.yaml", id));

    let mut bug: Bug = storage::load_yaml(&path)?;

    if let Some(s) = status {
        bug.status = s;
    }
    if let Some(p) = priority {
        bug.priority = p;
    }
    if assignee.is_some() {
        bug.assignee = assignee;
    }
    bug.updated_at = Local::now().to_rfc3339();

    storage::save_yaml(&path, &bug)?;

    if json {
        println!("{}", serde_json::to_string(&bug).unwrap());
    } else {
        println!("Bug {} updated", id);
    }

    Ok(())
}

fn show_bug(id: String, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("bugs").join(format!("{}.yaml", id));

    let bug: Bug = storage::load_yaml(&path)?;

    if json {
        println!("{}", serde_json::to_string(&bug).unwrap());
    } else {
        println!("Bug: {} - {}", bug.id, bug.title);
        println!("Status: {:?}", bug.status);
        println!("Priority: {:?}", bug.priority);
        println!("Reporter: {}", bug.reporter);
        println!("Assignee: {}", bug.assignee.as_deref().unwrap_or("unassigned"));
        if let Some(desc) = &bug.description {
            println!("Description: {}", desc);
        }
        if !bug.labels.is_empty() {
            println!("Labels: {}", bug.labels.join(", "));
        }
        println!("Created: {}", bug.created_at);
        println!("Updated: {}", bug.updated_at);
    }

    Ok(())
}

fn close_bug(id: String, json: bool) -> Result<()> {
    update_bug(id, Some(BugStatus::Closed), None, None, json)
}

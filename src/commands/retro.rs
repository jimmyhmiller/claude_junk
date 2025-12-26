use chrono::Local;
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{self, Result};

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RetroCategory {
    Good,
    Bad,
    Action,
}

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ActionStatus {
    Pending,
    InProgress,
    Done,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RetroItem {
    pub id: String,
    pub category: RetroCategory,
    pub content: String,
    pub author: String,
    pub sprint: Option<String>,
    pub action_status: Option<ActionStatus>,
    pub action_owner: Option<String>,
    pub created_at: String,
}

#[derive(Subcommand)]
pub enum RetroAction {
    /// Add something that went well
    Good {
        /// What went well
        content: String,
        /// Sprint/iteration identifier
        #[arg(short, long)]
        sprint: Option<String>,
    },
    /// Add something that didn't go well
    Bad {
        /// What didn't go well
        content: String,
        /// Sprint/iteration identifier
        #[arg(short, long)]
        sprint: Option<String>,
    },
    /// Add an action item
    Action {
        /// Action item description
        content: String,
        /// Who owns this action
        #[arg(short, long)]
        owner: Option<String>,
        /// Sprint/iteration identifier
        #[arg(short, long)]
        sprint: Option<String>,
    },
    /// List retro items
    List {
        /// Filter by category
        #[arg(short, long)]
        category: Option<RetroCategory>,
        /// Filter by sprint
        #[arg(short, long)]
        sprint: Option<String>,
        /// Show only open action items
        #[arg(long)]
        actions_only: bool,
    },
    /// Update action item status
    Done {
        /// Action item ID
        id: String,
    },
}

pub fn run(action: RetroAction, json: bool) -> Result<()> {
    match action {
        RetroAction::Good { content, sprint } => {
            add_item(RetroCategory::Good, content, sprint, None, json)
        }
        RetroAction::Bad { content, sprint } => {
            add_item(RetroCategory::Bad, content, sprint, None, json)
        }
        RetroAction::Action { content, owner, sprint } => {
            add_item(RetroCategory::Action, content, sprint, owner, json)
        }
        RetroAction::List { category, sprint, actions_only } => {
            list_items(category, sprint, actions_only, json)
        }
        RetroAction::Done { id } => mark_done(id, json),
    }
}

fn add_item(category: RetroCategory, content: String, sprint: Option<String>, owner: Option<String>, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;

    let item = RetroItem {
        id: Uuid::new_v4().to_string()[..8].to_string(),
        category: category.clone(),
        content,
        author: storage::get_author(),
        sprint,
        action_status: if category == RetroCategory::Action { Some(ActionStatus::Pending) } else { None },
        action_owner: owner,
        created_at: Local::now().to_rfc3339(),
    };

    let path = dir.join("retros").join(format!("{}.yaml", item.id));
    storage::save_yaml(&path, &item)?;

    if json {
        println!("{}", serde_json::to_string(&item).unwrap());
    } else {
        let emoji = match item.category {
            RetroCategory::Good => "+",
            RetroCategory::Bad => "-",
            RetroCategory::Action => "!",
        };
        println!("[{}] {} {}", emoji, item.id, item.content);
    }

    Ok(())
}

fn list_items(category: Option<RetroCategory>, sprint: Option<String>, actions_only: bool, json: bool) -> Result<()> {
    let mut items: Vec<RetroItem> = storage::load_all_from_dir::<RetroItem>("retros")?
        .into_iter()
        .map(|(_, i)| i)
        .collect();

    if let Some(c) = category {
        items.retain(|i| i.category == c);
    }
    if let Some(s) = sprint {
        items.retain(|i| i.sprint.as_ref().map(|x| x == &s).unwrap_or(false));
    }
    if actions_only {
        items.retain(|i| i.category == RetroCategory::Action &&
            i.action_status.as_ref().map(|s| *s != ActionStatus::Done).unwrap_or(false));
    }

    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    if json {
        println!("{}", serde_json::to_string(&items).unwrap());
    } else if items.is_empty() {
        println!("No retro items found.");
    } else {
        let mut current_category = None;
        for item in items {
            if current_category.as_ref() != Some(&item.category) {
                current_category = Some(item.category.clone());
                let header = match &item.category {
                    RetroCategory::Good => "\nWhat went well:",
                    RetroCategory::Bad => "\nWhat didn't go well:",
                    RetroCategory::Action => "\nAction items:",
                };
                println!("{}", header);
            }

            let status = if item.category == RetroCategory::Action {
                match item.action_status {
                    Some(ActionStatus::Done) => " [done]",
                    Some(ActionStatus::InProgress) => " [in-progress]",
                    _ => " [pending]",
                }
            } else {
                ""
            };

            println!("  [{}] {}{}", item.id, item.content, status);
        }
    }

    Ok(())
}

fn mark_done(id: String, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("retros").join(format!("{}.yaml", id));

    let mut item: RetroItem = storage::load_yaml(&path)?;
    item.action_status = Some(ActionStatus::Done);
    storage::save_yaml(&path, &item)?;

    if json {
        println!("{}", serde_json::to_string(&item).unwrap());
    } else {
        println!("Action item {} marked as done", id);
    }

    Ok(())
}

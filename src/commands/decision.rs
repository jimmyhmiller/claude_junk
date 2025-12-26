use chrono::Local;
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{self, Result};

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DecisionStatus {
    Proposed,
    Accepted,
    Deprecated,
    Superseded,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub title: String,
    pub context: Option<String>,
    pub decision: String,
    pub consequences: Vec<String>,
    pub status: DecisionStatus,
    pub author: String,
    pub superseded_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Subcommand)]
pub enum DecisionAction {
    /// Record a new decision
    Record {
        /// Decision title
        title: String,
        /// The decision made
        #[arg(short, long)]
        decision: String,
        /// Context/background for the decision
        #[arg(short, long)]
        context: Option<String>,
        /// Consequences of this decision
        #[arg(long)]
        consequence: Vec<String>,
    },
    /// List all decisions
    List {
        /// Filter by status
        #[arg(short, long)]
        status: Option<DecisionStatus>,
    },
    /// Show decision details
    Show {
        /// Decision ID
        id: String,
    },
    /// Accept a proposed decision
    Accept {
        /// Decision ID
        id: String,
    },
    /// Deprecate a decision
    Deprecate {
        /// Decision ID
        id: String,
    },
    /// Supersede a decision with a new one
    Supersede {
        /// Decision ID to supersede
        id: String,
        /// New decision ID that supersedes this one
        #[arg(short, long)]
        by: String,
    },
}

pub fn run(action: DecisionAction, json: bool) -> Result<()> {
    match action {
        DecisionAction::Record { title, decision, context, consequence } => {
            record_decision(title, decision, context, consequence, json)
        }
        DecisionAction::List { status } => list_decisions(status, json),
        DecisionAction::Show { id } => show_decision(id, json),
        DecisionAction::Accept { id } => update_status(id, DecisionStatus::Accepted, json),
        DecisionAction::Deprecate { id } => update_status(id, DecisionStatus::Deprecated, json),
        DecisionAction::Supersede { id, by } => supersede_decision(id, by, json),
    }
}

fn record_decision(title: String, decision: String, context: Option<String>, consequences: Vec<String>, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let now = Local::now().to_rfc3339();

    let dec = Decision {
        id: Uuid::new_v4().to_string()[..8].to_string(),
        title,
        context,
        decision,
        consequences,
        status: DecisionStatus::Proposed,
        author: storage::get_author(),
        superseded_by: None,
        created_at: now.clone(),
        updated_at: now,
    };

    let path = dir.join("decisions").join(format!("{}.yaml", dec.id));
    storage::save_yaml(&path, &dec)?;

    if json {
        println!("{}", serde_json::to_string(&dec).unwrap());
    } else {
        println!("Decision recorded: {} - {}", dec.id, dec.title);
        println!("Status: {:?}", dec.status);
    }

    Ok(())
}

fn list_decisions(status: Option<DecisionStatus>, json: bool) -> Result<()> {
    let mut decisions: Vec<Decision> = storage::load_all_from_dir::<Decision>("decisions")?
        .into_iter()
        .map(|(_, d)| d)
        .collect();

    if let Some(s) = status {
        decisions.retain(|d| d.status == s);
    }

    decisions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    if json {
        println!("{}", serde_json::to_string(&decisions).unwrap());
    } else if decisions.is_empty() {
        println!("No decisions found.");
    } else {
        for dec in decisions {
            let status_mark = match dec.status {
                DecisionStatus::Proposed => "?",
                DecisionStatus::Accepted => "+",
                DecisionStatus::Deprecated => "-",
                DecisionStatus::Superseded => "~",
            };
            println!("[{}] {} {} - {}", status_mark, dec.id, dec.title, dec.decision);
        }
    }

    Ok(())
}

fn show_decision(id: String, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("decisions").join(format!("{}.yaml", id));

    let dec: Decision = storage::load_yaml(&path)?;

    if json {
        println!("{}", serde_json::to_string(&dec).unwrap());
    } else {
        println!("Decision: {} - {}", dec.id, dec.title);
        println!("Status: {:?}", dec.status);
        println!("Author: {}", dec.author);
        if let Some(ctx) = &dec.context {
            println!("\nContext: {}", ctx);
        }
        println!("\nDecision: {}", dec.decision);
        if !dec.consequences.is_empty() {
            println!("\nConsequences:");
            for c in &dec.consequences {
                println!("  - {}", c);
            }
        }
        if let Some(sup) = &dec.superseded_by {
            println!("\nSuperseded by: {}", sup);
        }
        println!("\nCreated: {}", dec.created_at);
    }

    Ok(())
}

fn update_status(id: String, status: DecisionStatus, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("decisions").join(format!("{}.yaml", id));

    let mut dec: Decision = storage::load_yaml(&path)?;
    dec.status = status;
    dec.updated_at = Local::now().to_rfc3339();
    storage::save_yaml(&path, &dec)?;

    if json {
        println!("{}", serde_json::to_string(&dec).unwrap());
    } else {
        println!("Decision {} status: {:?}", id, dec.status);
    }

    Ok(())
}

fn supersede_decision(id: String, by: String, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("decisions").join(format!("{}.yaml", id));

    let mut dec: Decision = storage::load_yaml(&path)?;
    dec.status = DecisionStatus::Superseded;
    dec.superseded_by = Some(by.clone());
    dec.updated_at = Local::now().to_rfc3339();
    storage::save_yaml(&path, &dec)?;

    if json {
        println!("{}", serde_json::to_string(&dec).unwrap());
    } else {
        println!("Decision {} superseded by {}", id, by);
    }

    Ok(())
}

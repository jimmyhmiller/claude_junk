use chrono::Local;
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{self, Result};

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Todo,
    InProgress,
    Review,
    Done,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub assignee: Option<String>,
    pub labels: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Subcommand)]
pub enum TaskAction {
    /// Add a new task
    Add {
        /// Task title
        title: String,
        /// Task description
        #[arg(short, long)]
        description: Option<String>,
        /// Initial status
        #[arg(short, long, default_value = "todo")]
        status: TaskStatus,
        /// Assign to someone
        #[arg(short, long)]
        assignee: Option<String>,
        /// Labels/tags
        #[arg(short, long)]
        label: Vec<String>,
    },
    /// List tasks
    List {
        /// Filter by status
        #[arg(short, long)]
        status: Option<TaskStatus>,
        /// Filter by assignee
        #[arg(short, long)]
        assignee: Option<String>,
    },
    /// Show kanban board view
    Board,
    /// Move task to different status
    Move {
        /// Task ID
        id: String,
        /// New status
        status: TaskStatus,
    },
    /// Start working on a task (move to in-progress)
    Start {
        /// Task ID
        id: String,
    },
    /// Mark task as done
    Done {
        /// Task ID
        id: String,
    },
    /// Assign task to someone
    Assign {
        /// Task ID
        id: String,
        /// Assignee name
        assignee: String,
    },
}

pub fn run(action: TaskAction, json: bool) -> Result<()> {
    match action {
        TaskAction::Add { title, description, status, assignee, label } => {
            add_task(title, description, status, assignee, label, json)
        }
        TaskAction::List { status, assignee } => list_tasks(status, assignee, json),
        TaskAction::Board => show_board(json),
        TaskAction::Move { id, status } => move_task(id, status, json),
        TaskAction::Start { id } => move_task(id, TaskStatus::InProgress, json),
        TaskAction::Done { id } => move_task(id, TaskStatus::Done, json),
        TaskAction::Assign { id, assignee } => assign_task(id, assignee, json),
    }
}

fn add_task(title: String, description: Option<String>, status: TaskStatus, assignee: Option<String>, labels: Vec<String>, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let now = Local::now().to_rfc3339();

    let task = Task {
        id: Uuid::new_v4().to_string()[..8].to_string(),
        title,
        description,
        status,
        assignee,
        labels,
        created_at: now.clone(),
        updated_at: now,
    };

    let path = dir.join("tasks").join(format!("{}.yaml", task.id));
    storage::save_yaml(&path, &task)?;

    if json {
        println!("{}", serde_json::to_string(&task).unwrap());
    } else {
        println!("Task created: {} ({})", task.id, task.title);
    }

    Ok(())
}

fn list_tasks(status: Option<TaskStatus>, assignee: Option<String>, json: bool) -> Result<()> {
    let mut tasks: Vec<Task> = storage::load_all_from_dir::<Task>("tasks")?
        .into_iter()
        .map(|(_, t)| t)
        .collect();

    if let Some(s) = status {
        tasks.retain(|t| t.status == s);
    }
    if let Some(a) = assignee {
        tasks.retain(|t| t.assignee.as_ref().map(|x| x.to_lowercase().contains(&a.to_lowercase())).unwrap_or(false));
    }

    tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    if json {
        println!("{}", serde_json::to_string(&tasks).unwrap());
    } else if tasks.is_empty() {
        println!("No tasks found.");
    } else {
        for task in tasks {
            let assignee_str = task.assignee.as_deref().unwrap_or("-");
            println!("[{}] {:?} - {} ({})", task.id, task.status, task.title, assignee_str);
        }
    }

    Ok(())
}

fn show_board(json: bool) -> Result<()> {
    let tasks: Vec<Task> = storage::load_all_from_dir::<Task>("tasks")?
        .into_iter()
        .map(|(_, t)| t)
        .collect();

    if json {
        println!("{}", serde_json::to_string(&tasks).unwrap());
        return Ok(());
    }

    let todo: Vec<_> = tasks.iter().filter(|t| t.status == TaskStatus::Todo).collect();
    let in_progress: Vec<_> = tasks.iter().filter(|t| t.status == TaskStatus::InProgress).collect();
    let review: Vec<_> = tasks.iter().filter(|t| t.status == TaskStatus::Review).collect();
    let done: Vec<_> = tasks.iter().filter(|t| t.status == TaskStatus::Done).collect();

    println!("=== KANBAN BOARD ===\n");

    println!("TODO ({}):", todo.len());
    for t in &todo {
        println!("  [{}] {}", t.id, t.title);
    }

    println!("\nIN PROGRESS ({}):", in_progress.len());
    for t in &in_progress {
        println!("  [{}] {}", t.id, t.title);
    }

    println!("\nREVIEW ({}):", review.len());
    for t in &review {
        println!("  [{}] {}", t.id, t.title);
    }

    println!("\nDONE ({}):", done.len());
    for t in &done {
        println!("  [{}] {}", t.id, t.title);
    }

    Ok(())
}

fn move_task(id: String, status: TaskStatus, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("tasks").join(format!("{}.yaml", id));

    let mut task: Task = storage::load_yaml(&path)?;
    task.status = status;
    task.updated_at = Local::now().to_rfc3339();
    storage::save_yaml(&path, &task)?;

    if json {
        println!("{}", serde_json::to_string(&task).unwrap());
    } else {
        println!("Task {} moved to {:?}", id, task.status);
    }

    Ok(())
}

fn assign_task(id: String, assignee: String, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("tasks").join(format!("{}.yaml", id));

    let mut task: Task = storage::load_yaml(&path)?;
    task.assignee = Some(assignee);
    task.updated_at = Local::now().to_rfc3339();
    storage::save_yaml(&path, &task)?;

    if json {
        println!("{}", serde_json::to_string(&task).unwrap());
    } else {
        println!("Task {} assigned to {}", id, task.assignee.as_ref().unwrap());
    }

    Ok(())
}

use chrono::Local;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{self, Result};

#[derive(Debug, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub author: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Subcommand)]
pub enum NoteAction {
    /// Create a new note
    Add {
        /// Note title
        title: String,
        /// Note content
        #[arg(short, long)]
        content: String,
        /// Tags for organization
        #[arg(short, long)]
        tag: Vec<String>,
    },
    /// List all notes
    List {
        /// Filter by tag
        #[arg(short, long)]
        tag: Option<String>,
        /// Search in title/content
        #[arg(short, long)]
        search: Option<String>,
    },
    /// Show note content
    Show {
        /// Note ID
        id: String,
    },
    /// Append to an existing note
    Append {
        /// Note ID
        id: String,
        /// Content to append
        content: String,
    },
    /// Delete a note
    Delete {
        /// Note ID
        id: String,
    },
}

pub fn run(action: NoteAction, json: bool) -> Result<()> {
    match action {
        NoteAction::Add { title, content, tag } => add_note(title, content, tag, json),
        NoteAction::List { tag, search } => list_notes(tag, search, json),
        NoteAction::Show { id } => show_note(id, json),
        NoteAction::Append { id, content } => append_note(id, content, json),
        NoteAction::Delete { id } => delete_note(id, json),
    }
}

fn add_note(title: String, content: String, tags: Vec<String>, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let now = Local::now().to_rfc3339();

    let note = Note {
        id: Uuid::new_v4().to_string()[..8].to_string(),
        title,
        content,
        author: storage::get_author(),
        tags,
        created_at: now.clone(),
        updated_at: now,
    };

    let path = dir.join("notes").join(format!("{}.yaml", note.id));
    storage::save_yaml(&path, &note)?;

    if json {
        println!("{}", serde_json::to_string(&note).unwrap());
    } else {
        println!("Note created: {} - {}", note.id, note.title);
    }

    Ok(())
}

fn list_notes(tag: Option<String>, search: Option<String>, json: bool) -> Result<()> {
    let mut notes: Vec<Note> = storage::load_all_from_dir::<Note>("notes")?
        .into_iter()
        .map(|(_, n)| n)
        .collect();

    if let Some(t) = tag {
        notes.retain(|n| n.tags.iter().any(|x| x.to_lowercase() == t.to_lowercase()));
    }

    if let Some(s) = search {
        let s = s.to_lowercase();
        notes.retain(|n| {
            n.title.to_lowercase().contains(&s) || n.content.to_lowercase().contains(&s)
        });
    }

    notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    if json {
        println!("{}", serde_json::to_string(&notes).unwrap());
    } else if notes.is_empty() {
        println!("No notes found.");
    } else {
        for note in notes {
            let tags = if note.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", note.tags.join(", "))
            };
            println!("[{}] {}{}", note.id, note.title, tags);
        }
    }

    Ok(())
}

fn show_note(id: String, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("notes").join(format!("{}.yaml", id));

    let note: Note = storage::load_yaml(&path)?;

    if json {
        println!("{}", serde_json::to_string(&note).unwrap());
    } else {
        println!("Note: {} - {}", note.id, note.title);
        println!("Author: {}", note.author);
        if !note.tags.is_empty() {
            println!("Tags: {}", note.tags.join(", "));
        }
        println!("Created: {}", note.created_at);
        println!("Updated: {}", note.updated_at);
        println!("\n{}", note.content);
    }

    Ok(())
}

fn append_note(id: String, content: String, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("notes").join(format!("{}.yaml", id));

    let mut note: Note = storage::load_yaml(&path)?;
    note.content = format!("{}\n\n{}", note.content, content);
    note.updated_at = Local::now().to_rfc3339();
    storage::save_yaml(&path, &note)?;

    if json {
        println!("{}", serde_json::to_string(&note).unwrap());
    } else {
        println!("Note {} updated", id);
    }

    Ok(())
}

fn delete_note(id: String, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("notes").join(format!("{}.yaml", id));

    std::fs::remove_file(&path)?;

    if json {
        println!(r#"{{"deleted": "{}"}}"#, id);
    } else {
        println!("Note {} deleted", id);
    }

    Ok(())
}

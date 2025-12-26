use chrono::Local;
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{self, Result};

#[derive(Debug, Clone, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    Pending,
    InReview,
    Approved,
    ChangesRequested,
    Merged,
    Closed,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Review {
    pub id: String,
    pub branch: String,
    pub title: String,
    pub description: Option<String>,
    pub author: String,
    pub reviewers: Vec<String>,
    pub status: ReviewStatus,
    pub comments: Vec<ReviewComment>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReviewComment {
    pub author: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Subcommand)]
pub enum ReviewAction {
    /// Request a code review for a branch
    Request {
        /// Branch name (defaults to current branch)
        #[arg(short, long)]
        branch: Option<String>,
        /// Review title
        title: String,
        /// Description of changes
        #[arg(short, long)]
        description: Option<String>,
        /// Request specific reviewers
        #[arg(short, long)]
        reviewer: Vec<String>,
    },
    /// List review requests
    List {
        /// Filter by status
        #[arg(short, long)]
        status: Option<ReviewStatus>,
        /// Filter by author
        #[arg(short, long)]
        author: Option<String>,
        /// Show reviews where you are a reviewer
        #[arg(long)]
        mine: bool,
    },
    /// Show review details
    Show {
        /// Review ID
        id: String,
    },
    /// Approve a review
    Approve {
        /// Review ID
        id: String,
        /// Optional approval comment
        #[arg(short, long)]
        comment: Option<String>,
    },
    /// Request changes on a review
    RequestChanges {
        /// Review ID
        id: String,
        /// Reason for requesting changes
        comment: String,
    },
    /// Add a comment to a review
    Comment {
        /// Review ID
        id: String,
        /// Comment content
        content: String,
    },
    /// Mark a review as merged
    Merge {
        /// Review ID
        id: String,
    },
    /// Close a review without merging
    Close {
        /// Review ID
        id: String,
    },
}

pub fn run(action: ReviewAction, json: bool) -> Result<()> {
    match action {
        ReviewAction::Request { branch, title, description, reviewer } => {
            request_review(branch, title, description, reviewer, json)
        }
        ReviewAction::List { status, author, mine } => list_reviews(status, author, mine, json),
        ReviewAction::Show { id } => show_review(id, json),
        ReviewAction::Approve { id, comment } => approve_review(id, comment, json),
        ReviewAction::RequestChanges { id, comment } => request_changes(id, comment, json),
        ReviewAction::Comment { id, content } => add_comment(id, content, json),
        ReviewAction::Merge { id } => update_status(id, ReviewStatus::Merged, json),
        ReviewAction::Close { id } => update_status(id, ReviewStatus::Closed, json),
    }
}

fn get_current_branch() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn request_review(branch: Option<String>, title: String, description: Option<String>, reviewers: Vec<String>, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let now = Local::now().to_rfc3339();

    let branch = branch.unwrap_or_else(get_current_branch);

    let review = Review {
        id: Uuid::new_v4().to_string()[..8].to_string(),
        branch,
        title,
        description,
        author: storage::get_author(),
        reviewers,
        status: ReviewStatus::Pending,
        comments: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };

    let path = dir.join("reviews").join(format!("{}.yaml", review.id));
    storage::save_yaml(&path, &review)?;

    if json {
        println!("{}", serde_json::to_string(&review).unwrap());
    } else {
        println!("Review requested: {} - {}", review.id, review.title);
        println!("Branch: {}", review.branch);
        if !review.reviewers.is_empty() {
            println!("Reviewers: {}", review.reviewers.join(", "));
        }
    }

    Ok(())
}

fn list_reviews(status: Option<ReviewStatus>, author: Option<String>, mine: bool, json: bool) -> Result<()> {
    let me = storage::get_author();
    let mut reviews: Vec<Review> = storage::load_all_from_dir::<Review>("reviews")?
        .into_iter()
        .map(|(_, r)| r)
        .collect();

    if let Some(s) = status {
        reviews.retain(|r| r.status == s);
    }

    if let Some(a) = author {
        reviews.retain(|r| r.author.to_lowercase().contains(&a.to_lowercase()));
    }

    if mine {
        reviews.retain(|r| r.reviewers.iter().any(|rev| rev.to_lowercase() == me.to_lowercase()));
    }

    reviews.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    if json {
        println!("{}", serde_json::to_string(&reviews).unwrap());
    } else if reviews.is_empty() {
        println!("No reviews found.");
    } else {
        for review in reviews {
            let status_icon = match review.status {
                ReviewStatus::Pending => "o",
                ReviewStatus::InReview => "~",
                ReviewStatus::Approved => "+",
                ReviewStatus::ChangesRequested => "!",
                ReviewStatus::Merged => "*",
                ReviewStatus::Closed => "x",
            };
            println!("[{}] {} {} - {} ({})",
                status_icon, review.id, review.branch, review.title, review.author);
        }
    }

    Ok(())
}

fn show_review(id: String, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("reviews").join(format!("{}.yaml", id));

    let review: Review = storage::load_yaml(&path)?;

    if json {
        println!("{}", serde_json::to_string(&review).unwrap());
    } else {
        println!("Review: {} - {}", review.id, review.title);
        println!("Branch: {}", review.branch);
        println!("Author: {}", review.author);
        println!("Status: {:?}", review.status);
        if !review.reviewers.is_empty() {
            println!("Reviewers: {}", review.reviewers.join(", "));
        }
        if let Some(desc) = &review.description {
            println!("\nDescription: {}", desc);
        }
        if !review.comments.is_empty() {
            println!("\nComments:");
            for c in &review.comments {
                println!("  [{}] {}: {}", &c.created_at[..10], c.author, c.content);
            }
        }
        println!("\nCreated: {}", review.created_at);
    }

    Ok(())
}

fn approve_review(id: String, comment: Option<String>, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("reviews").join(format!("{}.yaml", id));

    let mut review: Review = storage::load_yaml(&path)?;
    review.status = ReviewStatus::Approved;
    review.updated_at = Local::now().to_rfc3339();

    if let Some(c) = comment {
        review.comments.push(ReviewComment {
            author: storage::get_author(),
            content: format!("Approved: {}", c),
            created_at: review.updated_at.clone(),
        });
    }

    storage::save_yaml(&path, &review)?;

    if json {
        println!("{}", serde_json::to_string(&review).unwrap());
    } else {
        println!("Review {} approved", id);
    }

    Ok(())
}

fn request_changes(id: String, comment: String, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("reviews").join(format!("{}.yaml", id));

    let mut review: Review = storage::load_yaml(&path)?;
    review.status = ReviewStatus::ChangesRequested;
    review.updated_at = Local::now().to_rfc3339();
    review.comments.push(ReviewComment {
        author: storage::get_author(),
        content: format!("Changes requested: {}", comment),
        created_at: review.updated_at.clone(),
    });

    storage::save_yaml(&path, &review)?;

    if json {
        println!("{}", serde_json::to_string(&review).unwrap());
    } else {
        println!("Changes requested on review {}", id);
    }

    Ok(())
}

fn add_comment(id: String, content: String, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("reviews").join(format!("{}.yaml", id));

    let mut review: Review = storage::load_yaml(&path)?;
    review.updated_at = Local::now().to_rfc3339();
    review.comments.push(ReviewComment {
        author: storage::get_author(),
        content,
        created_at: review.updated_at.clone(),
    });

    storage::save_yaml(&path, &review)?;

    if json {
        println!("{}", serde_json::to_string(&review).unwrap());
    } else {
        println!("Comment added to review {}", id);
    }

    Ok(())
}

fn update_status(id: String, status: ReviewStatus, json: bool) -> Result<()> {
    let dir = storage::ensure_initialized()?;
    let path = dir.join("reviews").join(format!("{}.yaml", id));

    let mut review: Review = storage::load_yaml(&path)?;
    review.status = status;
    review.updated_at = Local::now().to_rfc3339();
    storage::save_yaml(&path, &review)?;

    if json {
        println!("{}", serde_json::to_string(&review).unwrap());
    } else {
        println!("Review {} status: {:?}", id, review.status);
    }

    Ok(())
}

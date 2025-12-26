use clap::{Parser, Subcommand};

mod commands;
mod storage;

#[derive(Parser)]
#[command(name = "agile")]
#[command(about = "CLI agile tools for small teams, designed for AI agent interaction")]
#[command(version)]
pub struct Cli {
    /// Output in JSON format (for agent parsing)
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Daily standup - record what you did, doing, and blockers
    Standup {
        #[command(subcommand)]
        action: commands::standup::StandupAction,
    },
    /// Bug tracking - report and manage bugs
    Bug {
        #[command(subcommand)]
        action: commands::bug::BugAction,
    },
    /// Retrospective - record what went well, what didn't, and action items
    Retro {
        #[command(subcommand)]
        action: commands::retro::RetroAction,
    },
    /// Task management - simple kanban board
    Task {
        #[command(subcommand)]
        action: commands::task::TaskAction,
    },
    /// Decision records - document architectural and team decisions
    Decision {
        #[command(subcommand)]
        action: commands::decision::DecisionAction,
    },
    /// Notes - quick meeting notes and context
    Note {
        #[command(subcommand)]
        action: commands::note::NoteAction,
    },
    /// Code review - track branches ready for review
    Review {
        #[command(subcommand)]
        action: commands::review::ReviewAction,
    },
    /// Kudos - team appreciation and wins
    Kudos {
        #[command(subcommand)]
        action: commands::kudos::KudosAction,
    },
    /// Initialize agile tracking in current directory
    Init,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            storage::init_storage()?;
            if cli.json {
                println!(r#"{{"status": "initialized", "path": ".agile"}}"#);
            } else {
                println!("Initialized agile tracking in .agile/");
            }
        }
        Commands::Standup { action } => commands::standup::run(action, cli.json)?,
        Commands::Bug { action } => commands::bug::run(action, cli.json)?,
        Commands::Retro { action } => commands::retro::run(action, cli.json)?,
        Commands::Task { action } => commands::task::run(action, cli.json)?,
        Commands::Decision { action } => commands::decision::run(action, cli.json)?,
        Commands::Note { action } => commands::note::run(action, cli.json)?,
        Commands::Review { action } => commands::review::run(action, cli.json)?,
        Commands::Kudos { action } => commands::kudos::run(action, cli.json)?,
    }

    Ok(())
}

use clap::{Parser, Subcommand};

mod standup;

#[derive(Parser)]
#[command(name = "standup")]
#[command(about = "Daily standup tracking for teams")]
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
    /// Initialize standup tracking in current directory
    Init,
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
    /// Login to sync service
    Login {
        /// Email address
        #[arg(short, long)]
        email: String,
        /// Password
        #[arg(short, long)]
        password: String,
        /// Server URL
        #[arg(short, long, default_value = "https://api.agile.tools")]
        server: String,
    },
    /// Logout from sync service
    Logout,
    /// Sync local changes with server
    Sync,
    /// Show sync status
    Status,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => standup::init(cli.json)?,
        Commands::Add { yesterday, today, blocker } => {
            standup::add(yesterday, today, blocker, cli.json)?
        }
        Commands::List { date, author, limit } => {
            standup::list(date, author, limit, cli.json)?
        }
        Commands::Today => standup::today(cli.json)?,
        Commands::Login { email, password, server } => {
            standup::login(&email, &password, &server, cli.json).await?
        }
        Commands::Logout => standup::logout(cli.json)?,
        Commands::Sync => standup::sync(cli.json).await?,
        Commands::Status => standup::status(cli.json).await?,
    }

    Ok(())
}

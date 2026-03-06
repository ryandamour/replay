mod analyze;
mod capture;
mod cluster;
mod error;
mod git;
mod init;
mod install;
mod storage;
mod types;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Empty JSON hook output — must not inject context on capture.
const JSON_OUTPUT: &str = r#"{"result":""}"#;

#[derive(Parser)]
#[command(name = "replay", about = "Preserve user prompts across Claude Code context compactions")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Capture a user prompt from a Claude Code hook (reads JSON from stdin)
    Capture,

    /// Analyze captured prompts and output a structured summary
    Analyze {
        /// Show messages from the last duration (e.g. "24h", "3d", "1w")
        #[arg(long)]
        last: Option<String>,

        /// Show messages from the last N sessions
        #[arg(long)]
        sessions: Option<usize>,

        /// Show only today's messages
        #[arg(long)]
        today: bool,

        /// Show all messages (no time filter)
        #[arg(long)]
        all: bool,

        /// Filter messages by topic keyword (case-insensitive substring match)
        #[arg(long)]
        topic: Option<String>,
    },

    /// Initialize a .replay/ directory in the given path
    Init {
        /// Directory to create .replay/ in (defaults to current directory)
        #[arg(long)]
        dir: Option<PathBuf>,
    },

    /// Install hooks into ~/.claude/settings.json and the /replay slash command
    Install,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Capture => capture::run(),
        Commands::Analyze {
            last,
            sessions,
            today,
            all,
            topic,
        } => {
            let filter = if all {
                analyze::Filter::All
            } else if today {
                analyze::Filter::Today
            } else if let Some(n) = sessions {
                analyze::Filter::LastSessions(n)
            } else if let Some(duration_str) = last {
                match parse_duration(&duration_str) {
                    Some(d) => analyze::Filter::LastDuration(d),
                    None => {
                        eprintln!(
                            "replay: invalid duration '{}' (use e.g. 24h, 3d, 1w)",
                            duration_str
                        );
                        std::process::exit(1);
                    }
                }
            } else {
                analyze::Filter::default()
            };
            analyze::run(filter, topic)
        }
        Commands::Init { dir } => {
            let base = dir.unwrap_or_else(|| std::env::current_dir().expect("cannot get cwd"));
            init::init_replay_dir(&base)
        }
        Commands::Install => install::run(),
    };

    if let Err(e) = result {
        eprintln!("replay: {}", e);
    }
}

/// Parse a human-friendly duration string like "24h", "3d", "1w".
fn parse_duration(s: &str) -> Option<chrono::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: i64 = num_str.parse().ok()?;

    match unit {
        "h" => Some(chrono::Duration::hours(num)),
        "d" => Some(chrono::Duration::days(num)),
        "w" => Some(chrono::Duration::weeks(num)),
        _ => None,
    }
}

use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod store;
mod tui;
mod gui;
mod report;
mod cli;
mod pomodoro;
mod charts;
mod idle;
mod autotag;
mod invoice;

#[derive(Parser)]
#[command(name = "tracker")]
#[command(about = "Privacy-first time tracker")]
struct Args {
    #[command(subcommand)]
    cmd: Option<Commands>,

    #[arg(short, long, default_value = "~/.local/share/opentracker/data.db")]
    db: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Start tracking a task
    Start {
        name: String,
        #[arg(short, long)]
        tags: Option<String>,
        /// Auto-detect tags from current window title
        #[arg(short, long)]
        auto_tags: bool,
    },
    /// Stop current tracking
    Stop,
    /// Show current status
    Status,
    /// List today's entries
    Today,
    /// Launch TUI
    Tui,
    /// Launch web dashboard
    Gui {
        #[arg(short, long, default_value = "127.0.0.1:8746")]
        bind: String,
    },
    /// Generate report
    Report {
        #[arg(short, long)]
        days: Option<i64>,
        #[arg(short, long)]
        project: Option<String>,
    },
    /// Export data
    Export {
        format: String,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Start a Pomodoro focus session
    Pomodoro {
        name: Option<String>,
    },
    /// Monitor idle time and auto-pause tracking
    Idle,
    /// Check idle status
    IdleStatus,
    /// Resume last task
    Resume,
    /// Auto-detect tags from current window
    DetectTags,
    /// Generate HTML invoice
    Invoice {
        #[arg(short, long)]
        client: String,
        #[arg(short, long)]
        rate: f64,
        #[arg(short, long, default_value = "30")]
        days: i64,
        #[arg(short, long)]
        tag: Option<String>,
        #[arg(short, long)]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let db_path = shellexpand::tilde(&args.db.to_string_lossy()).to_string();

    match args.cmd {
        Some(Commands::Start { name, tags, auto_tags }) => {
            let final_tags = if auto_tags {
                autotag::ensure_config_exists()?;
                let detected = autotag::auto_detect_tags();
                let inferred = autotag::infer_tags_from_task_name(&name);
                let combined = match (detected, inferred) {
                    (Some(d), Some(i)) => Some(format!("{}, {}", d, i)),
                    (Some(d), None) => Some(d),
                    (None, Some(i)) => Some(i),
                    (None, None) => None,
                };
                match (tags, combined) {
                    (Some(t), Some(c)) => Some(format!("{}, {}", t, c)),
                    (Some(t), None) => Some(t),
                    (None, Some(c)) => Some(c),
                    (None, None) => None,
                }
            } else {
                tags
            };
            cli::start(&db_path, name, final_tags).await
        }
        Some(Commands::Stop) => cli::stop(&db_path).await,
        Some(Commands::Status) => cli::status(&db_path).await,
        Some(Commands::Today) => cli::today(&db_path).await,
        Some(Commands::Tui) => tui::run(&db_path).await,
        Some(Commands::Gui { bind }) => gui::run(&db_path, &bind).await,
        Some(Commands::Report { days, project }) => report::generate(&db_path, days, project).await,
        Some(Commands::Pomodoro { name }) => pomodoro::run(&db_path, name).await,
        Some(Commands::Export { format, output }) => cli::export(&db_path, &format, &output).await,
        Some(Commands::Idle) => idle::run_idle_monitor(&db_path).await,
        Some(Commands::IdleStatus) => {
            println!("{}", idle::check_idle_status());
            Ok(())
        }
        Some(Commands::Resume) => cli::resume(&db_path).await,
        Some(Commands::DetectTags) => {
            autotag::ensure_config_exists()?;
            match autotag::auto_detect_tags() {
                Some(tags) => println!("Detected tags: {}", tags),
                None => println!("No tags detected for current window."),
            }
            Ok(())
        }
        Some(Commands::Invoice { client, rate, days, tag, output }) => {
            invoice::generate_invoice_file(&db_path, &client, rate, days, tag.as_deref(), &output).await
        }
        None => {
            println!("OpenTracker — privacy-first time tracker");
            println!("Run 'tracker --help' for commands.");
            Ok(())
        }
    }
}

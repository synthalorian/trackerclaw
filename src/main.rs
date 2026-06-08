use chrono::Datelike;
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
mod notifications;
mod webhook;
mod budget;
mod calendar;
mod integrations;
mod team;

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
    /// Manage project budgets
    Budget {
        #[command(subcommand)]
        action: BudgetAction,
    },
    /// View calendar
    Calendar {
        #[arg(short, long)]
        month: Option<u32>,
        #[arg(short, long)]
        year: Option<i32>,
    },
    /// Configure webhook
    Webhook {
        #[command(subcommand)]
        action: WebhookAction,
    },
    /// Toggl Track integration
    Toggl {
        #[command(subcommand)]
        action: TogglAction,
    },
    /// Clockify integration
    Clockify {
        #[command(subcommand)]
        action: ClockifyAction,
    },
    /// Team management
    Team {
        #[command(subcommand)]
        action: TeamAction,
    },
}

#[derive(Subcommand)]
enum BudgetAction {
    Set { project: String, hours: f64 },
    List,
    Delete { project: String },
}

#[derive(Subcommand)]
enum WebhookAction {
    Set { url: String, #[arg(short, long)] enabled: bool, #[arg(short, long)] headers: Option<String> },
    Show,
}

#[derive(Subcommand)]
enum TogglAction {
    Import { api_token: String, #[arg(short, long)] start: String, #[arg(short, long)] end: String },
    Export { api_token: String, #[arg(short, long)] workspace_id: i64 },
}

#[derive(Subcommand)]
enum ClockifyAction {
    Import { api_key: String, workspace_id: String },
}

#[derive(Subcommand)]
enum TeamAction {
    Add { name: String, #[arg(short, long, default_value = "member")] role: String },
    List,
    Report { name: String, #[arg(short, long, default_value = "7")] days: i64 },
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
        Some(Commands::Budget { action }) => match action {
            BudgetAction::Set { project, hours } => budget::set_budget(&db_path, &project, hours),
            BudgetAction::List => budget::list_budgets(&db_path),
            BudgetAction::Delete { project } => budget::delete_budget(&db_path, &project),
        },
        Some(Commands::Calendar { month, year }) => {
            let y = year.unwrap_or_else(|| chrono::Local::now().year());
            let m = month.unwrap_or_else(|| chrono::Local::now().month());
            let days = calendar::month_view(&db_path, y, m)?;
            println!("Calendar: {}-{}", y, m);
            for (date, seconds) in days {
                if seconds > 0 {
                    println!("  {}: {}", date, calendar::format_duration_short(seconds));
                }
            }
            Ok(())
        }
        Some(Commands::Webhook { action }) => match action {
            WebhookAction::Set { url, enabled, headers } => webhook::set_webhook(&db_path, &url, enabled, headers.as_deref()),
            WebhookAction::Show => webhook::show_webhook(&db_path),
        },
        Some(Commands::Toggl { action }) => match action {
            TogglAction::Import { api_token, start, end } => integrations::import_toggl(&db_path, &api_token, &start, &end).await,
            TogglAction::Export { api_token, workspace_id } => integrations::export_toggl(&db_path, &api_token, workspace_id).await,
        },
        Some(Commands::Clockify { action }) => match action {
            ClockifyAction::Import { api_key, workspace_id } => integrations::import_clockify(&db_path, &api_key, &workspace_id).await,
        },
        Some(Commands::Team { action }) => match action {
            TeamAction::Add { name, role } => team::add_user(&db_path, &name, &role),
            TeamAction::List => team::list_users(&db_path),
            TeamAction::Report { name, days } => team::user_report(&db_path, &name, days),
        },
        None => {
            println!("OpenTracker — privacy-first time tracker");
            println!("Run 'tracker --help' for commands.");
            Ok(())
        }
    }
}

use chrono::Datelike;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod auth;
mod autotag;
mod budget;
mod calendar;
mod charts;
mod cli;
mod config;
mod gui;
mod idle;
mod install;
mod integrations;
mod invoice;
mod notifications;
mod pomodoro;
mod project;
mod report;
mod store;
mod team;
mod time_parse;
mod tui;
mod webhook;

#[derive(Parser)]
#[command(name = "trackerclaw")]
#[command(about = "TrackerClaw — privacy-first time tracker")]
struct Args {
    #[command(subcommand)]
    cmd: Option<Commands>,

    #[arg(short, long, default_value = "~/.local/share/trackerclaw/data.db")]
    db: PathBuf,

    #[arg(short, long)]
    user: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start tracking a task
    Start {
        name: String,
        #[arg(short, long)]
        tags: Option<String>,
        #[arg(short, long)]
        project: Option<String>,
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
    Pomodoro { name: Option<String> },
    /// Monitor idle time and auto-pause tracking
    Idle,
    /// Check idle status
    IdleStatus,
    /// Resume last task
    Resume,
    /// Install TrackerClaw binary and systemd user service
    Install,
    /// Uninstall TrackerClaw user service and binary
    Uninstall,
    /// Auto-detect tags from current window
    DetectTags,
    /// Manage time entries
    Entry {
        #[command(subcommand)]
        action: EntryAction,
    },
    /// Manage projects
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// Generate HTML invoice
    Invoice {
        #[arg(short, long)]
        client: String,
        #[arg(short, long)]
        rate: Option<f64>,
        #[arg(short, long, default_value = "30")]
        days: i64,
        #[arg(short, long)]
        tag: Option<String>,
        #[arg(short, long)]
        project: Option<String>,
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
        #[arg(long)]
        week: bool,
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
enum EntryAction {
    Show {
        id: i64,
    },
    Edit {
        id: i64,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        tags: Option<String>,
        #[arg(short, long)]
        started_at: Option<String>,
        #[arg(short, long)]
        ended_at: Option<String>,
    },
    Delete {
        id: i64,
    },
}

#[derive(Subcommand)]
enum ProjectAction {
    Add {
        name: String,
        #[arg(short, long)]
        client: Option<String>,
        #[arg(short, long)]
        rate: Option<f64>,
        #[arg(long)]
        color: Option<String>,
    },
    List,
    Edit {
        name: String,
        #[arg(short, long)]
        new_name: Option<String>,
        #[arg(short, long)]
        client: Option<String>,
        #[arg(short, long)]
        rate: Option<f64>,
        #[arg(long)]
        color: Option<String>,
    },
    Delete {
        name: String,
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
    Set {
        url: String,
        #[arg(short, long)]
        enabled: bool,
        #[arg(short, long)]
        headers: Option<String>,
    },
    Show,
}

#[derive(Subcommand)]
enum TogglAction {
    Import {
        api_token: String,
        #[arg(short, long)]
        start: String,
        #[arg(short, long)]
        end: String,
    },
    Export {
        api_token: String,
        #[arg(short, long)]
        workspace_id: i64,
        #[arg(short, long)]
        start: Option<String>,
        #[arg(long)]
        end: Option<String>,
    },
}

#[derive(Subcommand)]
enum ClockifyAction {
    Import {
        api_key: String,
        workspace_id: String,
        #[arg(short, long)]
        start: String,
        #[arg(short, long)]
        end: String,
    },
}

#[derive(Subcommand)]
enum TeamAction {
    Add {
        name: String,
        #[arg(short, long, default_value = "member")]
        role: String,
    },
    List,
    Switch {
        name: String,
    },
    Report {
        name: String,
        #[arg(short, long, default_value = "7")]
        days: i64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let db_path = shellexpand::tilde(&args.db.to_string_lossy()).to_string();

    config::ensure_config_exists()?;
    let _ = autotag::ensure_config_exists();
    auth::ensure_default_user(&db_path)?;
    let (user_id, _user_name, role) = auth::resolve_current_user(&db_path, args.user.as_deref())?;
    let is_admin = role == "admin";

    match args.cmd {
        Some(Commands::Start {
            name,
            tags,
            project,
            auto_tags,
        }) => {
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
            let project_id = match project {
                Some(ref name) => {
                    Some(project::resolve_project_id(&db_path, name)?.ok_or_else(|| {
                        anyhow::anyhow!(
                            "Project '{}' not found. Create it with 'trackerclaw project add {}'",
                            name,
                            name
                        )
                    })?)
                }
                None => None,
            };
            cli::start(&db_path, name, final_tags, project_id, user_id).await
        }
        Some(Commands::Stop) => cli::stop(&db_path, user_id).await,
        Some(Commands::Status) => cli::status(&db_path, user_id).await,
        Some(Commands::Today) => cli::today(&db_path, user_id, is_admin).await,
        Some(Commands::Tui) => tui::run(&db_path).await,
        Some(Commands::Gui { bind }) => gui::run(&db_path, &bind).await,
        Some(Commands::Report { days, project }) => {
            report::generate(&db_path, days, project, user_id, is_admin).await
        }
        Some(Commands::Pomodoro { name }) => pomodoro::run(&db_path, name, user_id).await,
        Some(Commands::Export { format, output }) => {
            cli::export(&db_path, &format, &output, user_id, is_admin).await
        }
        Some(Commands::Idle) => idle::run_idle_monitor(&db_path).await,
        Some(Commands::IdleStatus) => {
            println!("{}", idle::check_idle_status());
            Ok(())
        }
        Some(Commands::Resume) => cli::resume(&db_path, user_id).await,
        Some(Commands::Install) => Ok(install::install()?),
        Some(Commands::Uninstall) => Ok(install::uninstall()?),
        Some(Commands::DetectTags) => {
            autotag::ensure_config_exists()?;
            match autotag::auto_detect_tags() {
                Some(tags) => println!("Detected tags: {}", tags),
                None => println!("No tags detected for current window."),
            }
            Ok(())
        }
        Some(Commands::Entry { action }) => match action {
            EntryAction::Show { id } => cli::show_entry(&db_path, id, user_id, is_admin).await,
            EntryAction::Edit {
                id,
                name,
                tags,
                started_at,
                ended_at,
            } => {
                cli::edit_entry(
                    &db_path,
                    id,
                    name.as_deref(),
                    tags.as_deref(),
                    started_at.as_deref(),
                    ended_at.as_deref(),
                    user_id,
                    is_admin,
                )
                .await
            }
            EntryAction::Delete { id } => cli::delete_entry(&db_path, id, user_id, is_admin).await,
        },
        Some(Commands::Invoice {
            client,
            rate,
            days,
            tag,
            project,
            output,
        }) => {
            let rate = if let Some(ref name) = project {
                let store = crate::store::Store::open(std::path::Path::new(&db_path))?;
                rate.or_else(|| {
                    store
                        .get_project_by_name(name)
                        .ok()
                        .flatten()
                        .and_then(|p| p.hourly_rate)
                })
                .unwrap_or_else(|| config::load_config().default_rate)
            } else {
                rate.unwrap_or_else(|| config::load_config().default_rate)
            };
            invoice::generate_invoice_file(
                &db_path,
                &client,
                rate,
                days,
                tag.as_deref(),
                project.as_deref(),
                user_id,
                is_admin,
                &output,
            )
            .await
        }
        Some(Commands::Project { action }) => match action {
            ProjectAction::Add {
                name,
                client,
                rate,
                color,
            } => project::add_project(&db_path, &name, client.as_deref(), rate, color.as_deref()),
            ProjectAction::List => project::list_projects(&db_path),
            ProjectAction::Edit {
                name,
                new_name,
                client,
                rate,
                color,
            } => project::edit_project(
                &db_path,
                &name,
                new_name.as_deref(),
                client.as_deref(),
                rate,
                color.as_deref(),
            ),
            ProjectAction::Delete { name } => project::delete_project(&db_path, &name),
        },
        Some(Commands::Budget { action }) => match action {
            BudgetAction::Set { project, hours } => budget::set_budget(&db_path, &project, hours),
            BudgetAction::List => budget::list_budgets(&db_path),
            BudgetAction::Delete { project } => budget::delete_budget(&db_path, &project),
        },
        Some(Commands::Calendar { week, month, year }) => {
            if week {
                return cli::calendar_week(&db_path, user_id, is_admin).await;
            }
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
            WebhookAction::Set {
                url,
                enabled,
                headers,
            } => webhook::set_webhook(&db_path, &url, enabled, headers.as_deref()),
            WebhookAction::Show => webhook::show_webhook(&db_path),
        },
        Some(Commands::Toggl { action }) => match action {
            TogglAction::Import {
                api_token,
                start,
                end,
            } => integrations::import_toggl(&db_path, &api_token, &start, &end).await,
            TogglAction::Export {
                api_token,
                workspace_id,
                start,
                end,
            } => {
                integrations::export_toggl(
                    &db_path,
                    &api_token,
                    workspace_id,
                    start.as_deref(),
                    end.as_deref(),
                )
                .await
            }
        },
        Some(Commands::Clockify { action }) => match action {
            ClockifyAction::Import {
                api_key,
                workspace_id,
                start,
                end,
            } => {
                integrations::import_clockify(&db_path, &api_key, &workspace_id, &start, &end).await
            }
        },
        Some(Commands::Team { action }) => match action {
            TeamAction::Add {
                name,
                role: new_role,
            } => {
                auth::require_admin(&role)?;
                team::add_user(&db_path, &name, &new_role)
            }
            TeamAction::List => team::list_users(&db_path),
            TeamAction::Switch { name } => {
                let store = crate::store::Store::open(std::path::Path::new(&db_path))?;
                if store.get_user(&name)?.is_none() {
                    anyhow::bail!(
                        "User '{}' not found. Add them with 'trackerclaw team add {}'",
                        name,
                        name
                    );
                }
                auth::set_current_user(&name)?;
                println!("Switched to user '{}'.", name);
                Ok(())
            }
            TeamAction::Report { name, days } => {
                if !is_admin && name != _user_name {
                    anyhow::bail!("Members can only view their own report.");
                }
                team::user_report(&db_path, &name, days, user_id, is_admin)
            }
        },
        None => {
            println!("TrackerClaw — privacy-first time tracker");
            println!("Run 'trackerclaw --help' for commands.");
            Ok(())
        }
    }
}

# 🦞 TrackerClaw

A privacy-first time tracker. Local SQLite, no accounts, no cloud. TUI for quick entries, web dashboard for reports.

## Features

- ⏱️ **Start/stop tracking** — one command, no friction
- 🏷️ **Tags & projects** — organize by context
- 📊 **Web dashboard** — visual reports at `localhost:8746`
- 📈 **Charts & visualizations** — SVG bar charts for daily hours, pie charts for project breakdown
- 🍅 **Pomodoro timer** — built-in focus sessions
- 😴 **Idle detection** — auto-pause when you step away
- 🏷️ **Auto-tagging** — automatically tag entries by window title
- 🧾 **Invoice generation** — professional HTML invoices from tracked hours
- 📤 **Export** — CSV, JSON, ICAL
- 🔒 **100% local** — your data never leaves your machine

## Install

```bash
cargo build --release
./target/release/trackerclaw install
systemctl --user enable --now trackerclaw-idle
```

The `install` command copies the binary to `~/.local/bin` (or `/usr/local/bin` if it already exists there) and installs a systemd user service for the idle monitor.

To remove:

```bash
trackerclaw uninstall
```

## Configuration

TrackerClaw reads defaults from `~/.config/trackerclaw/config.toml`. The file is created automatically on first run.

```toml
idle_threshold_ms = 300000
pomodoro_work_minutes = 25
pomodoro_break_minutes = 5
default_rate = 150.0
theme = "synthwave"
```

## Usage

```bash
# Projects
trackerclaw project add "VHS-86" --client "Acme" --rate 175 --color "#00f0ff"
trackerclaw project list
trackerclaw project edit "VHS-86" --rate 200
trackerclaw project delete "VHS-86"

trackerclaw start "Coding VHS-86" --tags rust,ui --project "VHS-86"    # Start tracking
trackerclaw start "Coding VHS-86" --project "VHS-86"                    # Project only
trackerclaw start "Meeting" --auto-tags               # Auto-detect tags from window title + task name
trackerclaw stop                                      # Stop current
trackerclaw status                                    # What's active?
trackerclaw resume                                    # Resume last stopped task
trackerclaw today                                     # Today's entries
trackerclaw tui                                       # Interactive TUI with charts
trackerclaw gui                                       # Web dashboard with charts
trackerclaw pomodoro "Deep work"                      # 25min focus + 5min break
trackerclaw report --days 7                           # Weekly report
trackerclaw report --project "VHS-86" --days 30       # Project report
trackerclaw export csv -o hours.csv                   # Export to CSV
trackerclaw export json -o hours.json                 # Export to JSON
trackerclaw export ical -o hours.ics                  # Export to ICAL

# Edit/delete entries
# (IDs come from `trackerclaw today` or `trackerclaw entry show <id>`)
trackerclaw entry show 42                              # Show a single entry
trackerclaw entry edit 42 --tags rust,api              # Edit tags
trackerclaw entry edit 42 --started-at 2026-06-19T09:00:00Z --ended-at 2026-06-19T10:30:00Z
trackerclaw entry delete 42                            # Delete an entry

# Phase 2 Features
trackerclaw idle                                      # Monitor idle time & auto-pause
trackerclaw idle-status                               # Check current idle status
trackerclaw detect-tags                               # Preview tags for current window
trackerclaw invoice --client "Acme Corp" --rate 150 --days 30 -o invoice.html
trackerclaw invoice --client "Acme Corp" --rate 150 --days 30 -o invoice.md
```

## Web Dashboard

Launch the synthwave-styled dashboard:

```bash
trackerclaw gui
```

Features:
- **Start/Stop Timer** — control tracking directly from the browser
- **Live Session Display** — current task + running elapsed time
- **Daily Hours Chart** — server-rendered SVG bar chart for the last 14 days
- **Project Breakdown** — server-rendered SVG pie chart by tag
- **Today's Entries** — live-updating list with durations
- **Glassmorphism synthwave UI** — neon cyan, magenta, and purple accents

All charts are rendered as SVG on the server. The dashboard loads **zero external
resources** (no CDNs, no third-party JS) — everything is served from localhost.

## TUI Charts

The interactive TUI now includes ASCII/Unicode bar charts:

```bash
trackerclaw tui
```

Inside the TUI:
- Press `1` — Today's entries list
- Press `2` — Daily hours chart (last 7 days)
- Press `3` — Project breakdown chart (last 30 days)
- Use `←/→` or `h/l` to switch tabs

Charts render with Unicode block characters for crisp bar visuals in the terminal.

## Idle Detection

Automatically pause tracking when you step away:

```bash
# Start monitoring in background
trackerclaw idle

# Check current status
trackerclaw idle-status
```

Idle detection works by checking input device activity. It supports:
- `xprintidle` command (recommended — install with `pacman -S xprintidle`)
- X11 screensaver extension

Default idle threshold: **5 minutes**

When running the TUI, you'll get a popup prompt when returning from idle asking whether to **resume** the previous task (discard idle gap) or **keep it stopped** (keep idle gap). You can also use `trackerclaw resume` from the command line to quickly restart your last task.

## Auto-Tagging

Automatically assign tags based on the active window title **and task name**:

```bash
# Auto-detect tags when starting tracking
trackerclaw start "Working on API" --auto-tags

# Preview what tags would be assigned
trackerclaw detect-tags
```

Rules are defined in `~/.config/trackerclaw/autotag.toml`. A default config is created automatically with common patterns. Tags are inferred from both the active window title and the task name you provide, then combined:

```toml
[[rules]]
pattern = "(?i)firefox|chrome|chromium|brave|safari"
tags = "web,browsing"

[[rules]]
pattern = "(?i)code\\.exe|visual studio code|vscodium|cursor"
tags = "coding,dev"

[[rules]]
pattern = "(?i)terminal|alacritty|kitty|ghostty|wezterm|tmux"
tags = "terminal,cli"
```

Window title detection supports:
- **Hyprland** — via `hyprctl activewindow`
- **X11** — via X11 property queries

## Invoice Generation

Generate professional HTML or Markdown invoices:

```bash
# Generate HTML invoice with synthwave styling (uses default_rate from config)
trackerclaw invoice --client "Acme Corp" -o invoice.html

# Override rate on the fly
trackerclaw invoice --client "Acme Corp" --rate 150 -o invoice.html

# Invoice by project (uses project's hourly_rate if set)
trackerclaw invoice --client "Acme Corp" --project "VHS-86" -o invoice.html

# Generate Markdown invoice for easy editing
trackerclaw invoice --client "Acme Corp" --rate 150 -o invoice.md

# Generate invoice for specific project/tag
trackerclaw invoice --client "Acme Corp" --rate 150 --tag rust --days 7 -o rust-invoice.html
```

Invoices include:
- Client name and invoice period
- Itemized entries with descriptions, hours, and amounts
- Project totals and grand total
- Print-friendly CSS for HTML output (Ctrl+P to print/PDF)

## Phase 3 Features

### Project Budgets

Set time budgets per project and track progress:

```bash
# Set a 40-hour budget for a project
trackerclaw budget set rust 40

# List all budgets with usage
trackerclaw budget list

# Remove a budget
trackerclaw budget delete rust
```

Budgets appear as visual progress bars in the TUI (Projects tab).

### Calendar View

View your logged time in a calendar grid:

```bash
# View current month
trackerclaw calendar

# View specific month
trackerclaw calendar --month 6 --year 2026

# View current week with entries
trackerclaw calendar --week
```

In the TUI, press `4` to switch to the Calendar tab.

### Desktop Notifications

TrackerClaw now sends desktop notifications via `notify-rust` when:
- A timer hits a milestone (1h, 2h, etc.)
- Idle time is detected and tracking is paused
- A project budget reaches 80% or 100%

### Webhook Exports

Automatically POST time entries to a configured URL when stopping a timer:

```bash
# Configure webhook
trackerclaw webhook set https://example.com/hooks/trackerclaw --enabled

# Show current webhook config
trackerclaw webhook show
```

Payload is JSON with the full entry object (name, tags, started_at, ended_at, duration_seconds).

### Integrations

#### Toggl Track

```bash
# Import time entries from Toggl (date-only or RFC3339)
trackerclaw toggl import <API_TOKEN> --start 2026-06-01 --end 2026-06-08

# Export local entries to a Toggl workspace
# Defaults to the last 7 days; override with --start and --end
trackerclaw toggl export <API_TOKEN> --workspace-id 12345
trackerclaw toggl export <API_TOKEN> --workspace-id 12345 --start 2026-06-01 --end 2026-06-08
```

#### Clockify

```bash
# Import time entries from Clockify (projects are mapped to tags)
trackerclaw clockify import <API_KEY> <WORKSPACE_ID> --start 2026-06-01 --end 2026-06-08
```

### Team Mode

SQLite-backed multi-user support with role-based access:

```bash
# Add a team member (admin only)
trackerclaw team add alice --role member

# Switch active user (persists in ~/.config/trackerclaw/.user)
trackerclaw team switch alice

# Or use --user for one-off commands
trackerclaw --user alice today

# List all users
trackerclaw team list

# Generate report for a user (members can only view themselves)
trackerclaw team report alice --days 7
```

Roles:
- **admin** — full access, can manage users and see all entries.
- **member** — can only start/stop their own entries and view their own reports/calendar/exports.

All entries are tagged with a `user_id`. The default user is `default` with role `admin`.

## TUI Updates

The interactive TUI now includes:
- **Press `4`** — Calendar view (month grid with highlighted days)
- **Projects tab** — Shows budget progress bars alongside project breakdown
- **Idle notifications** — Desktop notifications when idle is detected

## Behavior Notes (the fine print, honestly)

- **Starting a new task auto-stops the current one.** An entry can never be left
  running invisibly; `stop` also recovers any orphaned open entry.
- **Durations are clamped at zero** — clock skew can never produce negative time.
- **"Today" means your local calendar day.** Charts aggregate entries by the day
  they *started* (UTC), so an entry crossing midnight counts toward its start day.
- **Pomodoro logging** — completed focus phases log the full configured length;
  quitting early logs actual focused time (pauses excluded) if at least 1 minute
  accumulated.
- **Budgets** match entries by project *and* by tag containing the project name.
- **Team mode is local and honor-system**: roles gate CLI/web filtering, but
  there is no authentication — anyone with file access to the DB is admin of
  their own machine. It exists for separating contexts (e.g. work/personal),
  not for adversarial multi-user security.
- **Outbound network access** happens only when you explicitly configure it:
  webhook delivery on `stop`, and the Toggl/Clockify integrations. Everything
  else is local-only.

## Roadmap

- [x] Pomodoro timer
- [x] Charts and visualizations
- [x] Idle detection
- [x] Auto-tagging by window title
- [x] Invoice generation
- [x] Project budgets / time limits
- [x] Desktop notifications
- [x] Webhook exports
- [x] Toggl Track integration
- [x] Clockify integration
- [x] Team mode
- [x] Calendar view
- [ ] Sync across devices (encrypted)

## License

MIT — This is the wave. 🎹🦞

---

## ☕ Support the Developer

If this project saved you time, solved a problem, or just made your day a little more neon, you can fuel the next one:

[![Buy Me A Coffee](https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png)](https://buymeacoffee.com/synthalorian)

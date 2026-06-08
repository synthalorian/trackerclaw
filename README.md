# ⏱️ OpenTracker

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
sudo cp target/release/tracker /usr/local/bin/
```

## Usage

```bash
tracker start "Coding VHS-86" --tags rust,ui    # Start tracking
tracker start "Meeting" --auto-tags               # Auto-detect tags from window title + task name
tracker stop                                      # Stop current
tracker status                                    # What's active?
tracker resume                                    # Resume last stopped task
tracker today                                     # Today's entries
tracker tui                                       # Interactive TUI with charts
tracker gui                                       # Web dashboard with charts
tracker pomodoro "Deep work"                      # 25min focus + 5min break
tracker report --days 7                           # Weekly report
tracker report --project rust --days 30           # Project report
tracker export csv -o hours.csv                   # Export to CSV
tracker export json -o hours.json                 # Export to JSON
tracker export ical -o hours.ics                  # Export to ICAL

# Phase 2 Features
tracker idle                                      # Monitor idle time & auto-pause
tracker idle-status                               # Check current idle status
tracker detect-tags                               # Preview tags for current window
tracker invoice --client "Acme Corp" --rate 150 --days 30 -o invoice.html
tracker invoice --client "Acme Corp" --rate 150 --days 30 -o invoice.md
```

## Web Dashboard

Launch the synthwave-styled dashboard:

```bash
tracker gui
```

Features:
- **Daily Hours Chart** — SVG bar chart showing last 14 days
- **Project Breakdown** — SVG pie chart showing time distribution by tag
- **Today's Entries** — Live-updating entry list with durations

## TUI Charts

The interactive TUI now includes ASCII/Unicode bar charts:

```bash
tracker tui
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
tracker idle

# Check current status
tracker idle-status
```

Idle detection works by checking input device activity. It supports:
- `xprintidle` command (recommended — install with `pacman -S xprintidle`)
- X11 screensaver extension
- `/dev/input` event files (fallback)

Default idle threshold: **5 minutes**

When running the TUI, you'll get a popup prompt when returning from idle asking whether to **resume** the previous task (discard idle gap) or **keep it stopped** (keep idle gap). You can also use `tracker resume` from the command line to quickly restart your last task.

## Auto-Tagging

Automatically assign tags based on the active window title **and task name**:

```bash
# Auto-detect tags when starting tracking
tracker start "Working on API" --auto-tags

# Preview what tags would be assigned
tracker detect-tags
```

Rules are defined in `~/.config/opentracker/autotag.toml`. A default config is created automatically with common patterns. Tags are inferred from both the active window title and the task name you provide, then combined:

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
# Generate HTML invoice with synthwave styling
tracker invoice --client "Acme Corp" --rate 150 -o invoice.html

# Generate Markdown invoice for easy editing
tracker invoice --client "Acme Corp" --rate 150 -o invoice.md

# Generate invoice for specific project/tag
tracker invoice --client "Acme Corp" --rate 150 --tag rust --days 7 -o rust-invoice.html
```

Invoices include:
- Client name and invoice period
- Itemized entries with descriptions, hours, and amounts
- Project totals and grand total
- Print-friendly CSS for HTML output (Ctrl+P to print/PDF)

## Roadmap

- [x] Pomodoro timer
- [x] Charts and visualizations
- [x] Idle detection
- [x] Auto-tagging by window title
- [x] Invoice generation
- [ ] Project budgets / time limits
- [ ] Sync across devices (encrypted)

## License

MIT

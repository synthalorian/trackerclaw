use crate::auth;
use crate::store::Store;
use anyhow::Result;
use axum::{
    extract::State,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
struct AppState {
    store: Arc<Mutex<Store>>,
    user_id: i64,
    is_admin: bool,
}

#[derive(Serialize)]
struct EntryResponse {
    id: i64,
    name: String,
    started_at: String,
    duration: i64,
    tags: Option<String>,
}

#[derive(Serialize)]
struct DailyStat {
    day: String,
    hours: f64,
}

#[derive(Serialize)]
struct ProjectStat {
    project: String,
    hours: f64,
}

#[derive(Serialize)]
struct StatusResponse {
    active: bool,
    name: Option<String>,
    tags: Option<String>,
    started_at: Option<String>,
    elapsed_seconds: i64,
}

#[derive(Deserialize)]
struct StartRequest {
    name: String,
    tags: Option<String>,
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn today(State(state): State<AppState>) -> Result<Json<Vec<EntryResponse>>, String> {
    let store = state.store.lock().await;
    let entries = store.list_today(state.user_id, state.is_admin).map_err(|e| e.to_string())?;
    let resp: Vec<_> = entries.into_iter().map(|e| EntryResponse {
        id: e.id,
        name: e.name,
        started_at: e.started_at,
        duration: e.duration_seconds.unwrap_or(0),
        tags: e.tags,
    }).collect();
    Ok(Json(resp))
}

async fn daily_chart(State(state): State<AppState>) -> Result<Html<String>, String> {
    let store = state.store.lock().await;
    let stats = store.daily_stats(14, state.user_id, state.is_admin).map_err(|e| e.to_string())?;
    let data: Vec<(String, f64)> = stats.into_iter()
        .map(|(day, seconds)| {
            let short_day = day.split('-').skip(1).collect::<Vec<_>>().join("-");
            (short_day, seconds as f64 / 3600.0)
        })
        .collect();
    let svg = crate::charts::bar_chart(&data, "Daily Hours (Last 14 Days)", 700, 300);
    Ok(Html(svg))
}

async fn project_chart(State(state): State<AppState>) -> Result<Html<String>, String> {
    let store = state.store.lock().await;
    let stats = store.project_stats(30, state.user_id, state.is_admin).map_err(|e| e.to_string())?;
    let data: Vec<(String, i64)> = stats.into_iter()
        .filter(|(_, seconds)| *seconds > 0)
        .collect();
    let svg = crate::charts::pie_chart(&data, "Project Breakdown (Last 30 Days)", 700, 350);
    Ok(Html(svg))
}

async fn stats_api(State(state): State<AppState>) -> Result<Json<serde_json::Value>, String> {
    let store = state.store.lock().await;
    let daily = store.daily_stats(14, state.user_id, state.is_admin).map_err(|e| e.to_string())?;
    let projects = store.project_stats(30, state.user_id, state.is_admin).map_err(|e| e.to_string())?;

    let daily_resp: Vec<DailyStat> = daily.into_iter()
        .map(|(day, seconds)| DailyStat { day, hours: seconds as f64 / 3600.0 })
        .collect();

    let project_resp: Vec<ProjectStat> = projects.into_iter()
        .map(|(project, seconds)| ProjectStat { project, hours: seconds as f64 / 3600.0 })
        .collect();

    Ok(Json(serde_json::json!({
        "daily": daily_resp,
        "projects": project_resp,
    })))
}

async fn status(State(state): State<AppState>) -> Result<Json<StatusResponse>, String> {
    let store = state.store.lock().await;
    match store.get_current(state.user_id).map_err(|e| e.to_string())? {
        Some(entry) => {
            let started = chrono::DateTime::parse_from_rfc3339(&entry.started_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let elapsed = (Utc::now() - started).num_seconds();
            Ok(Json(StatusResponse {
                active: true,
                name: Some(entry.name),
                tags: entry.tags,
                started_at: Some(entry.started_at),
                elapsed_seconds: elapsed,
            }))
        }
        None => Ok(Json(StatusResponse {
            active: false,
            name: None,
            tags: None,
            started_at: None,
            elapsed_seconds: 0,
        })),
    }
}

async fn start_timer(
    State(state): State<AppState>,
    Json(req): Json<StartRequest>,
) -> Result<Json<serde_json::Value>, String> {
    let store = state.store.lock().await;
    if store.get_current(state.user_id).map_err(|e| e.to_string())?.is_some() {
        return Ok(Json(serde_json::json!({"error": "Already tracking"})));
    }
    let id = store.start_entry(&req.name, req.tags.as_deref(), None, state.user_id).map_err(|e| e.to_string())?;
    Ok(Json(serde_json::json!({"id": id, "name": req.name, "tags": req.tags})))
}

async fn stop_timer(State(state): State<AppState>) -> Result<Json<EntryResponse>, String> {
    let store = state.store.lock().await;
    match store.stop_current(state.user_id).map_err(|e| e.to_string())? {
        Some(entry) => Ok(Json(EntryResponse {
            id: entry.id,
            name: entry.name,
            started_at: entry.started_at,
            duration: entry.duration_seconds.unwrap_or(0),
            tags: entry.tags,
        })),
        None => Err("Nothing is being tracked.".to_string()),
    }
}

pub async fn run(db: &str, bind: &str) -> Result<()> {
    let (user_id, _user_name, role) = auth::resolve_current_user(db, None)?;
    let is_admin = role == "admin";
    let store = Store::open(std::path::Path::new(db))?;
    let state = AppState {
        store: Arc::new(Mutex::new(store)),
        user_id,
        is_admin,
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/api/today", get(today))
        .route("/api/stats", get(stats_api))
        .route("/api/status", get(status))
        .route("/api/start", post(start_timer))
        .route("/api/stop", post(stop_timer))
        .route("/chart/daily", get(daily_chart))
        .route("/chart/projects", get(project_chart))
        .with_state(state);

    println!("TrackerClaw dashboard at http://{}", bind);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

const INDEX_HTML: &str = r#"
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>TrackerClaw Dashboard</title>
<script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.1/dist/chart.umd.min.js"></script>
<style>
:root {
  --bg: #0a0514;
  --bg-card: rgba(20, 12, 40, 0.75);
  --cyan: #00f0ff;
  --magenta: #ff00ff;
  --yellow: #f3e70f;
  --purple: #8f00ff;
  --text: #e8e6f0;
  --muted: #9b94b0;
  --border: rgba(0, 240, 255, 0.18);
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body {
  background: radial-gradient(ellipse at top, #1a0b3d 0%, var(--bg) 60%), var(--bg);
  color: var(--text);
  font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
  min-height: 100vh;
  padding: 2rem;
}
header {
  display: flex;
  justify-content: space-between;
  align-items: flex-end;
  margin-bottom: 2rem;
  border-bottom: 1px solid var(--border);
  padding-bottom: 1rem;
}
.brand { display: flex; align-items: center; gap: 0.75rem; }
.logo {
  font-size: 2.4rem;
  font-weight: 900;
  letter-spacing: 2px;
  background: linear-gradient(90deg, var(--cyan), var(--magenta));
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  text-shadow: 0 0 30px rgba(0, 240, 255, 0.25);
}
.brand span { font-size: 1.5rem; }
.subtitle { color: var(--muted); font-size: 0.95rem; margin-top: 0.25rem; }
.clock { font-variant-numeric: tabular-nums; color: var(--cyan); font-size: 1.3rem; }

.card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 16px;
  padding: 1.5rem;
  backdrop-filter: blur(12px);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.35);
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 1.5rem;
  margin-bottom: 1.5rem;
}

.timer-card { text-align: center; }
.timer-card h2 { color: var(--muted); font-size: 0.9rem; text-transform: uppercase; letter-spacing: 2px; margin-bottom: 1rem; }
#current-task { font-size: 1.6rem; font-weight: 700; margin-bottom: 0.5rem; min-height: 2rem; }
#elapsed { font-size: 3.2rem; font-weight: 200; font-variant-numeric: tabular-nums; color: var(--cyan); margin-bottom: 1.5rem; text-shadow: 0 0 20px rgba(0, 240, 255, 0.3); }
#elapsed.stopped { color: var(--muted); text-shadow: none; }
.controls { display: flex; gap: 0.75rem; justify-content: center; flex-wrap: wrap; }
input[type="text"] {
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 0.85rem 1rem;
  color: var(--text);
  font-size: 1rem;
  min-width: 240px;
  outline: none;
}
input[type="text"]:focus { border-color: var(--cyan); box-shadow: 0 0 12px rgba(0, 240, 255, 0.2); }
button {
  background: linear-gradient(135deg, var(--cyan), #0088ff);
  border: none;
  border-radius: 10px;
  padding: 0.85rem 1.6rem;
  color: #000;
  font-weight: 700;
  font-size: 1rem;
  cursor: pointer;
  transition: transform 0.15s, box-shadow 0.15s;
}
button:hover { transform: translateY(-2px); box-shadow: 0 6px 20px rgba(0, 240, 255, 0.35); }
button.stop {
  background: linear-gradient(135deg, var(--magenta), #ff0055);
  color: #fff;
}
button.stop:hover { box-shadow: 0 6px 20px rgba(255, 0, 255, 0.35); }
button:disabled { opacity: 0.5; cursor: not-allowed; transform: none; }
.chips { display: flex; gap: 0.5rem; justify-content: center; flex-wrap: wrap; margin-top: 1rem; }
.chip {
  background: rgba(143, 0, 255, 0.18);
  border: 1px solid rgba(143, 0, 255, 0.4);
  color: var(--text);
  padding: 0.35rem 0.75rem;
  border-radius: 999px;
  font-size: 0.85rem;
  cursor: pointer;
  transition: all 0.15s;
}
.chip:hover { background: rgba(143, 0, 255, 0.35); border-color: var(--magenta); }

.chart-box h3 { color: var(--muted); font-size: 0.85rem; text-transform: uppercase; letter-spacing: 2px; margin-bottom: 1rem; }
.entries h3 { color: var(--muted); font-size: 0.85rem; text-transform: uppercase; letter-spacing: 2px; margin-bottom: 1rem; }
.entry {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.85rem 0;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}
.entry:last-child { border-bottom: none; }
.entry .name { font-weight: 600; }
.entry .meta { display: flex; gap: 1rem; align-items: center; }
.entry .dur { color: var(--cyan); font-variant-numeric: tabular-nums; }
.entry .tags { color: var(--magenta); font-size: 0.8rem; background: rgba(255, 0, 255, 0.1); padding: 0.2rem 0.5rem; border-radius: 6px; }
#total { margin-top: 1rem; padding-top: 1rem; border-top: 1px solid var(--border); color: var(--yellow); font-size: 1.1rem; font-weight: 700; }
.empty { color: var(--muted); text-align: center; padding: 2rem 0; }

@media (max-width: 700px) {
  body { padding: 1rem; }
  #elapsed { font-size: 2.4rem; }
  .brand { flex-direction: column; align-items: flex-start; }
}
</style>
</head>
<body>
<header>
  <div class="brand">
    <div>🦞</div>
    <div>
      <div class="logo">TRACKERCLAW</div>
      <div class="subtitle">Privacy-first time tracker — local dashboard</div>
    </div>
  </div>
  <div class="clock" id="clock">--:--:--</div>
</header>

<div class="grid">
  <div class="card timer-card">
    <h2>Current Session</h2>
    <div id="current-task">Not tracking</div>
    <div id="elapsed" class="stopped">00:00:00</div>
    <div class="controls">
      <input type="text" id="task-input" placeholder="What are you working on?">
      <button id="action-btn" onclick="toggleTimer()">Start</button>
    </div>
    <div class="chips">
      <span class="chip" onclick="setTask('Deep work')">Deep work</span>
      <span class="chip" onclick="setTask('Coding')">Coding</span>
      <span class="chip" onclick="setTask('Meeting')">Meeting</span>
      <span class="chip" onclick="setTask('Design')">Design</span>
    </div>
  </div>

  <div class="card entries">
    <h3>Today's Entries</h3>
    <div id="entries"><div class="empty">Loading...</div></div>
    <div id="total"></div>
  </div>
</div>

<div class="grid">
  <div class="card chart-box">
    <h3>Daily Hours (Last 14 Days)</h3>
    <canvas id="dailyChart"></canvas>
  </div>
  <div class="card chart-box">
    <h3>Project Breakdown (Last 30 Days)</h3>
    <canvas id="projectChart"></canvas>
  </div>
</div>

<script>
let active = false;
let elapsedInterval = null;
let dailyChart = null;
let projectChart = null;

function formatTime(totalSeconds) {
  const h = Math.floor(totalSeconds / 3600).toString().padStart(2, '0');
  const m = Math.floor((totalSeconds % 3600) / 60).toString().padStart(2, '0');
  const s = (totalSeconds % 60).toString().padStart(2, '0');
  return `${h}:${m}:${s}`;
}

function setTask(name) {
  document.getElementById('task-input').value = name;
}

async function toggleTimer() {
  const input = document.getElementById('task-input');
  const btn = document.getElementById('action-btn');
  if (active) {
    btn.disabled = true;
    await fetch('/api/stop', { method: 'POST' });
  } else {
    const name = input.value.trim() || 'Untitled';
    btn.disabled = true;
    await fetch('/api/start', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, tags: '' })
    });
    input.value = '';
  }
  await refreshAll();
}

async function refreshStatus() {
  try {
    const r = await fetch('/api/status');
    const s = await r.json();
    const taskEl = document.getElementById('current-task');
    const elapsedEl = document.getElementById('elapsed');
    const btn = document.getElementById('action-btn');
    active = s.active;

    if (s.active) {
      taskEl.textContent = s.name;
      elapsedEl.classList.remove('stopped');
      btn.textContent = 'Stop';
      btn.classList.add('stop');
      clearInterval(elapsedInterval);
      let seconds = s.elapsed_seconds;
      elapsedEl.textContent = formatTime(seconds);
      elapsedInterval = setInterval(() => {
        seconds++;
        elapsedEl.textContent = formatTime(seconds);
      }, 1000);
    } else {
      taskEl.textContent = 'Not tracking';
      elapsedEl.classList.add('stopped');
      elapsedEl.textContent = '00:00:00';
      btn.textContent = 'Start';
      btn.classList.remove('stop');
      clearInterval(elapsedInterval);
    }
    btn.disabled = false;
  } catch (e) {
    console.error('status failed', e);
  }
}

async function refreshEntries() {
  try {
    const r = await fetch('/api/today');
    const data = await r.json();
    const total = data.reduce((s, e) => s + e.duration, 0);
    const container = document.getElementById('entries');
    if (data.length === 0) {
      container.innerHTML = '<div class="empty">No entries today.</div>';
    } else {
      container.innerHTML = data.map(e => {
        const h = (e.duration / 3600).toFixed(2);
        return `<div class="entry">
          <span class="name">${e.name}</span>
          <span class="meta">
            <span class="dur">${h}h</span>
            ${e.tags ? `<span class="tags">${e.tags}</span>` : ''}
          </span>
        </div>`;
      }).join('');
    }
    document.getElementById('total').innerHTML = `Total today: <strong>${(total / 3600).toFixed(2)}h</strong>`;
  } catch (e) {
    console.error('entries failed', e);
  }
}

async function refreshCharts() {
  try {
    const r = await fetch('/api/stats');
    const s = await r.json();
    const labels = s.daily.map(d => d.day.split('-').slice(1).join('-'));
    const dailyData = s.daily.map(d => d.hours);
    const projectLabels = s.projects.map(p => p.project);
    const projectData = s.projects.map(p => p.hours);

    const colors = ['#00f0ff', '#ff00ff', '#f3e70f', '#8f00ff', '#00ff88', '#ff4444', '#ff8800'];

    if (dailyChart) dailyChart.destroy();
    dailyChart = new Chart(document.getElementById('dailyChart'), {
      type: 'bar',
      data: {
        labels,
        datasets: [{
          label: 'Hours',
          data: dailyData,
          backgroundColor: 'rgba(0, 240, 255, 0.7)',
          borderColor: '#00f0ff',
          borderWidth: 1,
          borderRadius: 6
        }]
      },
      options: {
        responsive: true,
        plugins: { legend: { display: false } },
        scales: {
          y: { grid: { color: 'rgba(255,255,255,0.08)' }, ticks: { color: '#9b94b0' } },
          x: { grid: { display: false }, ticks: { color: '#9b94b0' } }
        }
      }
    });

    if (projectChart) projectChart.destroy();
    projectChart = new Chart(document.getElementById('projectChart'), {
      type: 'doughnut',
      data: {
        labels: projectLabels,
        datasets: [{
          data: projectData,
          backgroundColor: colors,
          borderColor: '#0a0514',
          borderWidth: 2
        }]
      },
      options: {
        responsive: true,
        plugins: {
          legend: { position: 'right', labels: { color: '#e8e6f0' } }
        }
      }
    });
  } catch (e) {
    console.error('charts failed', e);
  }
}

async function refreshAll() {
  await refreshStatus();
  await refreshEntries();
  await refreshCharts();
}

function updateClock() {
  document.getElementById('clock').textContent = new Date().toLocaleTimeString();
}

document.getElementById('task-input').addEventListener('keypress', (e) => {
  if (e.key === 'Enter' && !active) toggleTimer();
});

setInterval(updateClock, 1000);
setInterval(refreshAll, 5000);
updateClock();
refreshAll();
</script>
</body>
</html>
"#;

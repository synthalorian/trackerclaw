use crate::store::Store;
use crate::charts;
use anyhow::Result;
use axum::{
    extract::State,
    response::Html,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
struct AppState {
    store: Arc<Mutex<Store>>,
}

#[derive(Serialize)]
struct EntryResponse {
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

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn today(State(state): State<AppState>) -> Result<Json<Vec<EntryResponse>>, String> {
    let store = state.store.lock().await;
    let entries = store.list_today().map_err(|e| e.to_string())?;
    let resp: Vec<_> = entries.into_iter().map(|e| EntryResponse {
        name: e.name,
        started_at: e.started_at,
        duration: e.duration_seconds.unwrap_or(0),
        tags: e.tags,
    }).collect();
    Ok(Json(resp))
}

async fn daily_chart(State(state): State<AppState>) -> Result<Html<String>, String> {
    let store = state.store.lock().await;
    let stats = store.daily_stats(14).map_err(|e| e.to_string())?;
    let data: Vec<(String, f64)> = stats.into_iter()
        .map(|(day, seconds)| {
            let short_day = day.split('-').skip(1).collect::<Vec<_>>().join("-");
            (short_day, charts::format_hours(seconds))
        })
        .collect();
    let svg = charts::bar_chart(&data, "Daily Hours (Last 14 Days)", 700, 300);
    Ok(Html(svg))
}

async fn project_chart(State(state): State<AppState>) -> Result<Html<String>, String> {
    let store = state.store.lock().await;
    let stats = store.project_stats(30).map_err(|e| e.to_string())?;
    let data: Vec<(String, i64)> = stats.into_iter()
        .filter(|(_, seconds)| *seconds > 0)
        .collect();
    let svg = charts::pie_chart(&data, "Project Breakdown (Last 30 Days)", 700, 350);
    Ok(Html(svg))
}

async fn stats_api(State(state): State<AppState>) -> Result<Json<serde_json::Value>, String> {
    let store = state.store.lock().await;
    let daily = store.daily_stats(14).map_err(|e| e.to_string())?;
    let projects = store.project_stats(30).map_err(|e| e.to_string())?;

    let daily_resp: Vec<DailyStat> = daily.into_iter()
        .map(|(day, seconds)| DailyStat { day, hours: charts::format_hours(seconds) })
        .collect();

    let project_resp: Vec<ProjectStat> = projects.into_iter()
        .map(|(project, seconds)| ProjectStat { project, hours: charts::format_hours(seconds) })
        .collect();

    Ok(Json(serde_json::json!({
        "daily": daily_resp,
        "projects": project_resp,
    })))
}

pub async fn run(db: &str, bind: &str) -> Result<()> {
    let store = Store::open(std::path::Path::new(db))?;
    let state = AppState {
        store: Arc::new(Mutex::new(store)),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/api/today", get(today))
        .route("/api/stats", get(stats_api))
        .route("/chart/daily", get(daily_chart))
        .route("/chart/projects", get(project_chart))
        .with_state(state);

    println!("OpenTracker dashboard at http://{}", bind);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

const INDEX_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>OpenTracker Dashboard</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{background:#050508;color:#e0e0e0;font-family:'Segoe UI',monospace;padding:2rem;min-height:100vh}
h1{color:#00ffff;text-shadow:0 0 20px rgba(0,255,255,0.4);margin-bottom:0.5rem;font-size:2.5rem}
.subtitle{color:#888;margin-bottom:2rem;font-size:0.95rem}
.grid{display:grid;grid-template-columns:1fr 1fr;gap:1.5rem;margin-bottom:2rem}
.chart-box{background:#0f0f1a;border:1px solid #222;border-radius:8px;padding:1rem;overflow:hidden}
.chart-box h3{color:#ff00ff;margin-bottom:0.5rem;font-size:1rem;text-transform:uppercase;letter-spacing:1px}
.chart-box svg{width:100%;height:auto;display:block}
.entry{background:#0f0f1a;border:1px solid #222;padding:1rem;margin:0.5rem 0;border-radius:4px;display:flex;justify-content:space-between;align-items:center;transition:all 0.2s}
.entry:hover{border-color:#00ffff;transform:translateX(4px)}
.name{color:#e0e0e0;font-weight:600}
.dur{color:#00ffff;font-family:monospace}
.tags{color:#ff00ff;font-size:0.8rem;margin-left:1rem}
#total{margin-top:1rem;color:#ffff00;font-size:1.3rem;padding:1rem;background:#0f0f1a;border-radius:8px;border:1px solid #222}
.section-title{color:#00ffff;margin:2rem 0 1rem;font-size:1.2rem;text-transform:uppercase;letter-spacing:2px;border-bottom:2px solid #00ffff;padding-bottom:0.5rem}
@media(max-width:900px){.grid{grid-template-columns:1fr}}
</style>
</head>
<body>
<h1>⏱️ OPENTRACKER</h1>
<div class="subtitle">Privacy-first time tracker — Local dashboard</div>

<div class="section-title">Visualizations</div>
<div class="grid">
  <div class="chart-box">
    <h3>Daily Hours</h3>
    <div id="daily-chart">Loading...</div>
  </div>
  <div class="chart-box">
    <h3>Project Breakdown</h3>
    <div id="project-chart">Loading...</div>
  </div>
</div>

<div class="section-title">Today's Entries</div>
<div id="entries"></div>
<div id="total"></div>

<script>
async function loadDailyChart(){
  try {
    const r=await fetch('/chart/daily');
    const svg=await r.text();
    document.getElementById('daily-chart').innerHTML=svg;
  } catch(e) {
    document.getElementById('daily-chart').textContent='Failed to load chart';
  }
}

async function loadProjectChart(){
  try {
    const r=await fetch('/chart/projects');
    const svg=await r.text();
    document.getElementById('project-chart').innerHTML=svg;
  } catch(e) {
    document.getElementById('project-chart').textContent='Failed to load chart';
  }
}

async function loadEntries(){
  try {
    const r=await fetch('/api/today');
    const data=await r.json();
    const total=data.reduce((s,e)=>s+e.duration,0);
    const totalHours=(total/3600).toFixed(2);
    document.getElementById('entries').innerHTML=data.map(e=>{
      const h=(e.duration/3600).toFixed(2);
      return `<div class="entry">
        <span class="name">${e.name}</span>
        <span>
          <span class="dur">${h}h</span>
          ${e.tags?`<span class="tags">${e.tags}</span>`:''}
        </span>
      </div>`;
    }).join('');
    document.getElementById('total').innerHTML=`Total today: <strong>${totalHours}h</strong> (${total}s)`;
  } catch(e) {
    document.getElementById('entries').textContent='Failed to load entries';
  }
}

loadDailyChart();
loadProjectChart();
loadEntries();
</script>
</body>
</html>
"#;

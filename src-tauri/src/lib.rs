//! Nereus desktop app: Tauri commands over nereus-core.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nereus_core::browse::{self, DeploymentDetail, DeploymentSummary, TimelineData, TimelineMeta, Track};
use nereus_core::export::{self, ExportPreview, ExportRequest, ExportResult, Progress, Selection};
use nereus_core::species::{self, SpeciesName};
use nereus_core::{ConnectionSettings, Db, DbInfo};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::RwLock;

#[derive(Default)]
struct AppState {
    db: RwLock<Option<Db>>,
    cancel: Arc<AtomicBool>,
}

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

async fn db(state: &State<'_, AppState>) -> Res<Db> {
    state.db.read().await.clone().ok_or_else(|| err(nereus_core::Error::NotConnected))
}

#[tauri::command]
async fn connect(state: State<'_, AppState>, settings: ConnectionSettings) -> Res<DbInfo> {
    let (db, info) = Db::connect(settings).await.map_err(err)?;
    if let Some(old) = state.db.write().await.replace(db) {
        old.pool.close();
    }
    Ok(info)
}

#[tauri::command]
async fn disconnect(state: State<'_, AppState>) -> Res<()> {
    if let Some(db) = state.db.write().await.take() {
        db.pool.close();
    }
    Ok(())
}

#[tauri::command]
fn parse_connection_string(url: String) -> Res<ConnectionSettings> {
    ConnectionSettings::parse(&url).map_err(err)
}

#[tauri::command]
async fn deployments(state: State<'_, AppState>) -> Res<Vec<DeploymentSummary>> {
    browse::deployments(&db(&state).await?).await.map_err(err)
}

#[tauri::command]
async fn tracks(state: State<'_, AppState>) -> Res<Vec<Track>> {
    browse::tracks(&db(&state).await?).await.map_err(err)
}

#[tauri::command]
async fn deployment_detail(state: State<'_, AppState>, id: i64) -> Res<DeploymentDetail> {
    browse::deployment_detail(&db(&state).await?, id).await.map_err(err)
}

#[tauri::command]
async fn timeline_meta(state: State<'_, AppState>, ids: Vec<i64>) -> Res<TimelineMeta> {
    browse::timeline_meta(&db(&state).await?, &ids).await.map_err(err)
}

#[tauri::command]
async fn timeline_data(state: State<'_, AppState>, ids: Vec<i64>, t0: f64, t1: f64, bins: u32) -> Res<TimelineData> {
    browse::timeline_data(&db(&state).await?, &ids, t0, t1, bins).await.map_err(err)
}

#[tauri::command]
async fn species_names(app: AppHandle, tsns: Vec<i64>) -> Res<HashMap<i64, SpeciesName>> {
    let cache = app.path().app_cache_dir().map_err(err)?.join("itis_species_v2.json");
    Ok(species::names(&tsns, &cache).await)
}

#[tauri::command]
async fn export_preview(state: State<'_, AppState>, selection: Selection) -> Res<ExportPreview> {
    export::preview(&db(&state).await?, &selection).await.map_err(err)
}

#[tauri::command]
async fn start_export(app: AppHandle, state: State<'_, AppState>, request: ExportRequest) -> Res<ExportResult> {
    let db = db(&state).await?;
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    // At most ~10 progress events a second; stage changes always go through.
    let last = Mutex::new((Instant::now() - Duration::from_secs(1), String::new()));
    let progress = move |p: Progress| {
        let mut l = last.lock().unwrap();
        if l.1 != p.stage || l.0.elapsed() >= Duration::from_millis(100) {
            *l = (Instant::now(), p.stage.clone());
            let _ = app.emit("export-progress", p);
        }
    };
    export::run(&db, &request, &progress, &cancel).await.map_err(err)
}

#[tauri::command]
fn cancel_export(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::Relaxed);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    nereus_core::db::install_crypto();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            connect,
            disconnect,
            parse_connection_string,
            deployments,
            tracks,
            deployment_detail,
            timeline_meta,
            timeline_data,
            species_names,
            export_preview,
            start_export,
            cancel_export,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Nereus");
}

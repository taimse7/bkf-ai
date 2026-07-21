mod scanner;
mod conversion;

use bkf_converter_core::{convert_bkc, ConversionReport};
use bkf_container_probe::{probe_path, ProbeReport};
use bkf_scanner_core::database::{conversion_sources, init_database, last_scan, list_items, set_selected};
use conversion::{ConversionJob, ConversionState};
use bkf_scanner_core::models::{LibraryPage, ScanRun};
use scanner::{spawn_scan, ScanState};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use std::fs;

fn database_path(app: &AppHandle) -> Result<PathBuf, tauri::Error> {
    Ok(app.path().app_data_dir()?.join("library.sqlite3"))
}

#[tauri::command]
fn build_proof() -> &'static str {
    "החיבור ל־Rust פעיל"
}

#[tauri::command]
fn start_scan(
    source_path: String,
    app: AppHandle,
    state: State<'_, ScanState>,
) -> Result<ScanRun, String> {
    let source = PathBuf::from(source_path);
    let canonical = source
        .canonicalize()
        .map_err(|error| format!("לא ניתן לפתוח את המקור: {error}"))?;
    if !canonical.is_dir() {
        return Err("המקור שנבחר אינו תיקייה או כונן".into());
    }
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    if app_data.starts_with(&canonical) {
        return Err("לא ניתן לבחור מקור שמכיל את תיקיית Application Support של האפליקציה".into());
    }
    spawn_scan(&app, state.inner(), canonical).map_err(|error| error.to_string())
}

#[tauri::command]
fn cancel_scan(scan_id: String, state: State<'_, ScanState>) -> bool {
    state.cancel(&scan_id)
}

#[tauri::command]
fn get_last_scan(app: AppHandle) -> Result<Option<ScanRun>, String> {
    let path = database_path(&app).map_err(|error| error.to_string())?;
    let connection = init_database(&path).map_err(|error| error.to_string())?;
    last_scan(&connection).map_err(|error| error.to_string())
}

#[tauri::command]
fn resume_last_scan(
    app: AppHandle,
    state: State<'_, ScanState>,
) -> Result<Option<ScanRun>, String> {
    let path = database_path(&app).map_err(|error| error.to_string())?;
    let connection = init_database(&path).map_err(|error| error.to_string())?;
    let Some(run) = last_scan(&connection).map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    if run.status == "completed" || run.status == "completed_with_errors" {
        return Ok(Some(run));
    }
    let source = PathBuf::from(&run.root_path);
    if !source.is_dir() {
        bkf_scanner_core::database::mark_scan_disconnected(&connection, &run.id)
            .map_err(|error| error.to_string())?;
        return last_scan(&connection).map_err(|error| error.to_string());
    }
    drop(connection);
    spawn_scan(&app, state.inner(), source)
        .map(Some)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_library_page(
    scan_id: String,
    offset: u64,
    limit: u64,
    name_query: String,
    file_type: String,
    app: AppHandle,
) -> Result<LibraryPage, String> {
    let path = database_path(&app).map_err(|error| error.to_string())?;
    let connection = init_database(&path).map_err(|error| error.to_string())?;
    list_items(&connection, &scan_id, offset, limit.min(500), &name_query, &file_type)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn convert_verified_bkc(
    input_path: String,
    output_path: String,
) -> Result<ConversionReport, String> {
    convert_bkc(
        PathBuf::from(input_path).as_path(),
        PathBuf::from(output_path).as_path(),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn probe_book_structure(input_path: String) -> Result<ProbeReport, String> {
    probe_path(Path::new(&input_path)).map_err(|error| error.to_string())
}

#[tauri::command]
fn export_probe_report(input_path: String, output_path: String) -> Result<(), String> {
    let input = PathBuf::from(&input_path)
        .canonicalize()
        .map_err(|error| format!("קובץ המקור אינו זמין: {error}"))?;
    if !input.is_file() {
        return Err("המקור שנבחר אינו קובץ".into());
    }
    let report = probe_path(&input).map_err(|error| error.to_string())?;
    let document = serde_json::json!({
        "schemaVersion": 1,
        "application": "BKF AI",
        "generatedAtUnixMs": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis()),
        "source": {
            "name": input.file_name().and_then(|name| name.to_str()).unwrap_or(""),
            "size": report.file_size,
        },
        "probe": report,
        "scope": "structural-analysis-only",
    });
    fs::write(
        &output_path,
        serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("לא ניתן לשמור את דוח המבנה: {error}"))
}

#[tauri::command]
fn enqueue_conversions(
    scan_id: String,
    relative_paths: Vec<String>,
    all_supported: bool,
    destination_path: String,
    collision_policy: String,
    app: AppHandle,
    state: State<'_, Arc<ConversionState>>,
) -> Result<Vec<ConversionJob>, String> {
    let destination = conversion::verify_destination(Path::new(&destination_path))?;
    let db_path = database_path(&app).map_err(|error| error.to_string())?;
    let connection = init_database(&db_path).map_err(|error| error.to_string())?;
    let scan = last_scan(&connection).map_err(|error| error.to_string())?
        .filter(|scan| scan.id == scan_id).ok_or("הסריקה אינה זמינה")?;
    let root = PathBuf::from(&scan.root_path).canonicalize()
        .map_err(|error| format!("כונן המקור אינו זמין: {error}"))?;
    let sources = conversion_sources(&connection, &scan_id, &relative_paths, all_supported)
        .map_err(|error| error.to_string())?;
    let mut jobs = Vec::new();
    for source in sources {
        if all_supported && source.file_type != "BKC" { continue; }
        let input = root.join(&source.relative_path);
        let canonical = input.canonicalize().map_err(|error| format!("קובץ המקור אינו זמין: {error}"))?;
        if !canonical.starts_with(&root) { return Err("נתיב מקור לא בטוח".into()); }
        jobs.push(conversion::make_job(canonical, &destination, source.name, source.file_type, source.size, collision_policy == "rename"));
    }
    if jobs.is_empty() { return Err("לא נבחרו קובצי BKC נתמכים להמרה".into()); }
    state.add(jobs);
    conversion::start_worker(app, state.inner().clone());
    Ok(state.snapshot())
}

#[tauri::command]
fn get_conversion_queue(state: State<'_, Arc<ConversionState>>) -> Vec<ConversionJob> { state.snapshot() }

#[tauri::command]
fn resume_conversion_queue(app: AppHandle, state: State<'_, Arc<ConversionState>>) -> Vec<ConversionJob> {
    conversion::start_worker(app, state.inner().clone()); state.snapshot()
}

#[tauri::command]
fn cancel_conversions(state: State<'_, Arc<ConversionState>>) { state.cancel_all(); }

#[tauri::command]
fn retry_conversion(id: String, app: AppHandle, state: State<'_, Arc<ConversionState>>) -> Result<Vec<ConversionJob>, String> {
    if !state.retry(&id) { return Err("לא ניתן לנסות שוב משימה זו".into()); }
    conversion::start_worker(app, state.inner().clone()); Ok(state.snapshot())
}

#[tauri::command]
fn open_local_path(path: String) -> Result<(), String> {
    let value = PathBuf::from(path).canonicalize().map_err(|error| format!("הנתיב אינו זמין: {error}"))?;
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(&value).status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("explorer").arg(&value).status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let status = std::process::Command::new("xdg-open").arg(&value).status();
    status.map_err(|error| error.to_string()).and_then(|result| if result.success() { Ok(()) } else { Err("מערכת ההפעלה לא הצליחה לפתוח את הנתיב".into()) })
}

#[tauri::command]
fn export_diagnostics(
    output_path: String,
    app: AppHandle,
    state: State<'_, Arc<ConversionState>>,
) -> Result<(), String> {
    let db_path = database_path(&app).map_err(|error| error.to_string())?;
    let scan = init_database(&db_path).ok().and_then(|connection| last_scan(&connection).ok().flatten());
    let application_log = app.path().app_data_dir().ok()
        .and_then(|directory| fs::read_to_string(directory.join("bkf-ai.log")).ok())
        .unwrap_or_default();
    let report = serde_json::json!({
        "application": "BKF AI",
        "generatedAtUnixMs": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis()),
        "scan": scan,
        "conversionQueue": state.snapshot(),
        "applicationLog": application_log,
    });
    fs::write(&output_path, serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?)
        .map_err(|error| format!("לא ניתן לשמור את קובץ האבחון: {error}"))
}

#[tauri::command]
fn update_selected(
    scan_id: String,
    relative_path: String,
    selected: bool,
    app: AppHandle,
) -> Result<(), String> {
    let path = database_path(&app).map_err(|error| error.to_string())?;
    let connection = init_database(&path).map_err(|error| error.to_string())?;
    set_selected(&connection, &scan_id, &relative_path, selected).map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ScanState::default())
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            let path = app_data.join("library.sqlite3");
            init_database(&path)?;
            app.manage(Arc::new(ConversionState::load(app_data.join("conversion-queue.json"))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            build_proof,
            start_scan,
            cancel_scan,
            get_last_scan,
            resume_last_scan,
            get_library_page,
            update_selected,
            probe_book_structure,
            export_probe_report,
            convert_verified_bkc,
            enqueue_conversions,
            get_conversion_queue,
            resume_conversion_queue,
            cancel_conversions,
            retry_conversion,
            open_local_path,
            export_diagnostics
        ])
        .run(tauri::generate_context!())
        .expect("error while running BKF AI");
}

#[cfg(test)]
mod tests {
    use super::build_proof;

    #[test]
    fn build_proof_reports_rust_connection() {
        assert_eq!(build_proof(), "החיבור ל־Rust פעיל");
    }
}

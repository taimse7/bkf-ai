mod scanner;

use bkf_scanner_core::database::{init_database, last_scan, list_items, set_selected};
use bkf_scanner_core::models::{LibraryPage, ScanRun};
use scanner::{spawn_scan, ScanState};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

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
    app: AppHandle,
) -> Result<LibraryPage, String> {
    let path = database_path(&app).map_err(|error| error.to_string())?;
    let connection = init_database(&path).map_err(|error| error.to_string())?;
    list_items(&connection, &scan_id, offset, limit.min(500)).map_err(|error| error.to_string())
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
            let path = app.path().app_data_dir()?.join("library.sqlite3");
            init_database(&path)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            build_proof,
            start_scan,
            cancel_scan,
            get_last_scan,
            resume_last_scan,
            get_library_page,
            update_selected
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

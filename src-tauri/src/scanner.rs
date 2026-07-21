use bkf_scanner_core::classification::classify_file;
use bkf_scanner_core::database::{
    begin_scan, init_database, insert_batch, remove_stale, resumable_scan, unchanged_item,
    update_scan,
};
use bkf_scanner_core::models::{PendingItem, ScanProgress, ScanRun};
use std::collections::HashMap;
use std::io::{self};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Clone, Default)]
pub struct ScanState {
    cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl ScanState {
    pub fn cancel(&self, scan_id: &str) -> bool {
        self.cancellations
            .lock()
            .expect("scan cancellation lock poisoned")
            .get(scan_id)
            .map(|flag| {
                flag.store(true, Ordering::Release);
                true
            })
            .unwrap_or(false)
    }

    fn insert(&self, scan_id: String, flag: Arc<AtomicBool>) -> Result<(), ScanError> {
        let mut scans = self
            .cancellations
            .lock()
            .map_err(|_| ScanError::State("scan state lock poisoned".into()))?;
        if scans.contains_key(&scan_id) {
            return Err(ScanError::State("הסריקה כבר פעילה".into()));
        }
        scans.insert(scan_id, flag);
        Ok(())
    }

    fn remove(&self, scan_id: &str) {
        if let Ok(mut scans) = self.cancellations.lock() {
            scans.remove(scan_id);
        }
    }
}

#[derive(Debug)]
pub enum ScanError {
    Io(io::Error),
    Database(rusqlite::Error),
    Tauri(tauri::Error),
    State(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Database(error) => write!(formatter, "{error}"),
            Self::Tauri(error) => write!(formatter, "{error}"),
            Self::State(error) => formatter.write_str(error),
        }
    }
}

impl From<io::Error> for ScanError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<rusqlite::Error> for ScanError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value)
    }
}
impl From<tauri::Error> for ScanError {
    fn from(value: tauri::Error) -> Self {
        Self::Tauri(value)
    }
}

pub fn spawn_scan(
    app: &AppHandle,
    state: &ScanState,
    source: PathBuf,
) -> Result<ScanRun, ScanError> {
    let db_path = app.path().app_data_dir()?.join("library.sqlite3");
    let connection = init_database(&db_path)?;
    let root_path = source.to_string_lossy().into_owned();
    let previous = resumable_scan(&connection, &root_path)?;
    let id = previous
        .as_ref()
        .map(|run| run.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let generation = now_ms();
    begin_scan(&connection, &id, &root_path, generation, generation)?;
    let run = ScanRun {
        id: id.clone(),
        root_path,
        status: "running".into(),
        scanned: previous.as_ref().map_or(0, |run| run.scanned),
        errors: previous.as_ref().map_or(0, |run| run.errors),
        generation,
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    state.insert(id.clone(), cancelled.clone())?;
    let state = state.clone();
    let app = app.clone();
    let thread_run = run.clone();
    std::thread::spawn(move || {
        let result = scan_directory(&app, &thread_run, &source, &db_path, &cancelled);
        state.remove(&thread_run.id);
        if let Err(error) = result {
            if let Ok(connection) = init_database(&db_path) {
                let _ = update_scan(&connection, &thread_run.id, "error", 0, 1, now_ms());
            }
            let _ = app.emit(
                "scan-progress",
                ScanProgress {
                    scan_id: thread_run.id,
                    status: format!("error:{error}"),
                    scanned: 0,
                    errors: 1,
                    current_path: None,
                },
            );
        }
    });
    Ok(run)
}

fn scan_directory(
    app: &AppHandle,
    run: &ScanRun,
    source: &Path,
    db_path: &Path,
    cancelled: &AtomicBool,
) -> Result<(), ScanError> {
    let mut connection = init_database(db_path)?;
    let mut pending = Vec::with_capacity(500);
    let mut scanned = 0_u64;
    let mut errors = 0_u64;
    let mut disconnected = false;

    for entry in WalkDir::new(source).follow_links(false).into_iter() {
        if cancelled.load(Ordering::Acquire) {
            flush(&mut connection, &run.id, &mut pending)?;
            update_scan(&connection, &run.id, "cancelled", scanned, errors, now_ms())?;
            emit_progress(app, run, "cancelled", scanned, errors, None);
            return Ok(());
        }
        if !source.exists() {
            disconnected = true;
            break;
        }

        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors += 1;
                let path = error.path().unwrap_or(source);
                append_diagnostic(app, &format!("scan path={} error={error}", path.display()));
                let relative = relative_string(source, path);
                let status = if error
                    .io_error()
                    .is_some_and(|error| error.kind() == io::ErrorKind::PermissionDenied)
                {
                    "permission_denied"
                } else {
                    "read_error"
                };
                pending.push(PendingItem {
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    relative_path: relative,
                    size: 0,
                    file_type: "Unknown".into(),
                    modified_ms: None,
                    status: status.into(),
                    seen_generation: run.generation,
                });
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        scanned += 1;
        let path = entry.path();
        let relative_path = relative_string(source, path);
        let name = entry.file_name().to_string_lossy().into_owned();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                errors += 1;
                append_diagnostic(app, &format!("metadata path={} error={error}", path.display()));
                pending.push(PendingItem {
                    name,
                    relative_path,
                    size: 0,
                    file_type: "Unknown".into(),
                    modified_ms: None,
                    status: io_status(error.io_error().map_or(io::ErrorKind::Other, |e| e.kind()))
                        .into(),
                    seen_generation: run.generation,
                });
                continue;
            }
        };
        let modified_ms = metadata.modified().ok().and_then(system_time_ms);
        let size = metadata.len();
        let (file_type, status) =
            match unchanged_item(&connection, &run.id, &relative_path, size, modified_ms)? {
                Some(previous) => previous,
                None => match classify_file(path) {
                    Ok(file_type) => (file_type.into(), "ready".into()),
                    Err(error) => {
                        errors += 1;
                        append_diagnostic(app, &format!("classify path={} error={error}", path.display()));
                        ("Unknown".into(), io_status(error.kind()).into())
                    }
                },
            };
        pending.push(PendingItem {
            name,
            relative_path: relative_path.clone(),
            size,
            file_type,
            modified_ms,
            status,
            seen_generation: run.generation,
        });
        if pending.len() >= 500 {
            flush(&mut connection, &run.id, &mut pending)?;
        }
        if scanned % 250 == 0 {
            update_scan(&connection, &run.id, "running", scanned, errors, now_ms())?;
            emit_progress(app, run, "running", scanned, errors, Some(relative_path));
        }
    }

    flush(&mut connection, &run.id, &mut pending)?;
    let status = if disconnected {
        "disconnected"
    } else if errors > 0 {
        "completed_with_errors"
    } else {
        "completed"
    };
    if !disconnected {
        remove_stale(&connection, &run.id, run.generation)?;
    }
    update_scan(&connection, &run.id, status, scanned, errors, now_ms())?;
    emit_progress(app, run, status, scanned, errors, None);
    Ok(())
}

fn append_diagnostic(app: &AppHandle, message: &str) {
    let Ok(directory) = app.path().app_data_dir() else { return; };
    let _ = std::fs::create_dir_all(&directory);
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(directory.join("bkf-ai.log")) {
        let timestamp = now_ms();
        let _ = writeln!(file, "[{timestamp}] {message}");
    }
}

fn flush(
    connection: &mut rusqlite::Connection,
    scan_id: &str,
    pending: &mut Vec<PendingItem>,
) -> Result<(), ScanError> {
    if !pending.is_empty() {
        insert_batch(connection, scan_id, pending)?;
        pending.clear();
    }
    Ok(())
}

fn emit_progress(
    app: &AppHandle,
    run: &ScanRun,
    status: &str,
    scanned: u64,
    errors: u64,
    current_path: Option<String>,
) {
    let _ = app.emit(
        "scan-progress",
        ScanProgress {
            scan_id: run.id.clone(),
            status: status.into(),
            scanned,
            errors,
            current_path,
        },
    );
}

fn relative_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn io_status(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::NotFound => "disconnected",
        _ => "read_error",
    }
}

fn now_ms() -> i64 {
    system_time_ms(SystemTime::now()).unwrap_or(0)
}

fn system_time_ms(time: SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

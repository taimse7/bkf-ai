use bkf_bkf_core::{discover_sidecar, read_sidecar, validate_sidecar_for_book};
use bkf_catalog::{Catalog, Document, DocumentPage, Repository};
use bkf_converter_core::{analyze_bkc, convert_bkc, BkcSupport};
use bkf_local_api::ServerHandle;
use bkf_scanner_core::{scan_repository, ScanOptions, ScanProgress};
use bkf_search_core::{PageText, SearchEngine, SearchHit};
use bkf_text_extract::{extract_bkf_text_sidecar, extract_pdf_pages};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

const LOCAL_API_PORT: u16 = 47_831;

struct AppState {
    catalog: Catalog,
    search: SearchEngine,
    cache_dir: PathBuf,
    server: ServerHandle,
    scans: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapInfo {
    app_data_dir: String,
    local_api_port: u16,
    local_api_token: String,
    local_api_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewDescriptor {
    kind: String,
    document_id: String,
    title: String,
    local_path: Option<String>,
    page_count: Option<u64>,
    message: Option<String>,
    support_status: String,
}

#[tauri::command]
fn get_bootstrap(app: AppHandle, state: State<'_, AppState>) -> Result<BootstrapInfo, String> {
    let app_data = app.path().app_data_dir().map_err(|error| error.to_string())?;
    Ok(BootstrapInfo {
        app_data_dir: app_data.to_string_lossy().into_owned(),
        local_api_port: state.server.port,
        local_api_token: state.server.token.clone(),
        local_api_url: state.server.url.clone(),
    })
}

#[tauri::command]
fn list_repositories(state: State<'_, AppState>) -> Result<Vec<Repository>, String> {
    state
        .catalog
        .list_repositories()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn add_repository(
    root_path: String,
    display_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<Repository, String> {
    let canonical = PathBuf::from(&root_path)
        .canonicalize()
        .map_err(|error| format!("המאגר אינו זמין: {error}"))?;
    if !canonical.is_dir() {
        return Err("המקור שנבחר אינו תיקייה או כונן".into());
    }
    state
        .catalog
        .add_repository(
            &canonical.to_string_lossy(),
            display_name.as_deref(),
            now_ms(),
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn start_repository_scan(
    repository_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let repository = state
        .catalog
        .repository(&repository_id)
        .map_err(|error| error.to_string())?
        .ok_or("המאגר אינו קיים")?;

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut scans = state
            .scans
            .lock()
            .map_err(|_| "scan state lock poisoned")?;
        if scans.contains_key(&repository_id) {
            return Err("הסריקה כבר פעילה".into());
        }
        scans.insert(repository_id.clone(), cancel.clone());
    }

    let catalog = state.catalog.clone();
    let app_for_thread = app.clone();
    let id_for_thread = repository_id.clone();

    std::thread::spawn(move || {
        let result = scan_repository(
            &catalog,
            &repository,
            ScanOptions::default(),
            &cancel,
            |progress: ScanProgress| {
                let _ = app_for_thread.emit("repository-scan-progress", progress);
            },
        );
        if let Err(error) = result {
            let _ = app_for_thread.emit(
                "repository-scan-progress",
                ScanProgress {
                    repository_id: id_for_thread.clone(),
                    scanned: 0,
                    changed: 0,
                    errors: 1,
                    status: format!("error:{error}"),
                    current_path: None,
                },
            );
        }
        if let Ok(mut scans) = app_for_thread.state::<AppState>().scans.lock() {
            scans.remove(&id_for_thread);
        }
    });
    Ok(())
}

#[tauri::command]
fn cancel_repository_scan(
    repository_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let scans = state.scans.lock().map_err(|_| "scan state lock poisoned")?;
    let flag = scans.get(&repository_id).ok_or("אין סריקה פעילה למאגר")?;
    flag.store(true, Ordering::Release);
    Ok(())
}

#[tauri::command]
fn list_documents(
    repository_ids: Vec<String>,
    query: String,
    format: String,
    offset: u64,
    limit: u64,
    state: State<'_, AppState>,
) -> Result<DocumentPage, String> {
    state
        .catalog
        .list_documents(&repository_ids, &query, &format, offset, limit)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_document_preview(
    document_id: String,
    state: State<'_, AppState>,
) -> Result<PreviewDescriptor, String> {
    let document = get_document(&state.catalog, &document_id)?;
    let source = resolve_document_path(&state.catalog, &document)?;

    match document.format.as_str() {
        "PDF" => {
            let cache = cache_pdf_path(&state.cache_dir, &document);
            if !cache.is_file() {
                if let Some(parent) = cache.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                std::fs::copy(&source, &cache).map_err(|error| error.to_string())?;
            }
            state
                .catalog
                .set_preview(
                    &document.id,
                    "exact",
                    Some(&cache.to_string_lossy()),
                    document.page_count,
                )
                .map_err(|error| error.to_string())?;
            Ok(PreviewDescriptor {
                kind: "pdf".into(),
                document_id: document.id,
                title: document.name,
                local_path: Some(cache.to_string_lossy().into_owned()),
                page_count: document.page_count,
                message: None,
                support_status: "exact".into(),
            })
        }
        "BKC" => prepare_bkc_preview(&state, document, &source),
        "BKF" => prepare_bkf_preview(&state, document, &source),
        _ => Ok(PreviewDescriptor {
            kind: "unsupported".into(),
            document_id: document.id,
            title: document.name,
            local_path: None,
            page_count: None,
            message: Some("מבנה הקובץ אינו נתמך".into()),
            support_status: "unsupported".into(),
        }),
    }
}

fn prepare_bkc_preview(
    state: &AppState,
    document: Document,
    source: &Path,
) -> Result<PreviewDescriptor, String> {
    let analysis = analyze_bkc(source).map_err(|error| error.to_string())?;
    if matches!(analysis.support, BkcSupport::RepairUnavailable) {
        state
            .catalog
            .set_preview(&document.id, "unsupported", None, None)
            .map_err(|error| error.to_string())?;
        return Ok(PreviewDescriptor {
            kind: "unsupported".into(),
            document_id: document.id,
            title: document.name,
            local_path: None,
            page_count: document.page_count,
            message: Some("נדרש Ghostscript עבור וריאנט BKC זה".into()),
            support_status: "unsupported".into(),
        });
    }
    let cache = cache_pdf_path(&state.cache_dir, &document);
    if !cache.is_file() {
        if let Some(parent) = cache.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        convert_bkc(source, &cache).map_err(|error| error.to_string())?;
    }

    let support_status = match analysis.support {
        BkcSupport::Exact => "exact",
        BkcSupport::RepairAvailable => "repair",
        BkcSupport::RepairUnavailable => "unsupported",
    };
    let message = match analysis.support {
        BkcSupport::Exact => Some("BKC שוחזר באמצעות פרופיל מאומת".into()),
        BkcSupport::RepairAvailable => Some("BKC נבנה מחדש באמצעות מנוע Repair".into()),
        BkcSupport::RepairUnavailable => Some("נדרש Ghostscript עבור וריאנט BKC זה".into()),
    };

    state
        .catalog
        .set_preview(
            &document.id,
            support_status,
            Some(&cache.to_string_lossy()),
            document.page_count,
        )
        .map_err(|error| error.to_string())?;

    Ok(PreviewDescriptor {
        kind: "pdf".into(),
        document_id: document.id,
        title: document.name,
        local_path: Some(cache.to_string_lossy().into_owned()),
        page_count: document.page_count,
        message,
        support_status: support_status.into(),
    })
}

fn prepare_bkf_preview(
    state: &AppState,
    document: Document,
    source: &Path,
) -> Result<PreviewDescriptor, String> {
    let Some(sidecar_path) = discover_sidecar(source) else {
        state
            .catalog
            .set_preview(&document.id, "unsupported", None, None)
            .map_err(|error| error.to_string())?;
        return Ok(PreviewDescriptor {
            kind: "bkf".into(),
            document_id: document.id,
            title: document.name,
            local_path: None,
            page_count: None,
            message: Some("לא נמצא Sidecar תואם ל־200 הבתים הראשונים בכל עמוד".into()),
            support_status: "unsupported".into(),
        });
    };

    let sidecar = read_sidecar(&sidecar_path).map_err(|error| error.to_string())?;
    validate_sidecar_for_book(&sidecar, source, false).map_err(|error| error.to_string())?;
    state
        .catalog
        .set_preview(
            &document.id,
            "renderer_required",
            None,
            Some(sidecar.header.page_count as u64),
        )
        .map_err(|error| error.to_string())?;

    Ok(PreviewDescriptor {
        kind: "bkf".into(),
        document_id: document.id,
        title: document.name,
        local_path: None,
        page_count: Some(sidecar.header.page_count as u64),
        message: Some(
            "Sidecar תקין נמצא. נדרש לחבר DjVu Renderer כדי להציג את העמודים.".into(),
        ),
        support_status: "renderer_required".into(),
    })
}

#[tauri::command]
fn export_document_pdf(
    document_id: String,
    output_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let document = get_document(&state.catalog, &document_id)?;
    let source = resolve_document_path(&state.catalog, &document)?;
    let output = PathBuf::from(output_path);
    if output.exists() {
        return Err("קובץ היעד כבר קיים".into());
    }

    match document.format.as_str() {
        "PDF" => {
            std::fs::copy(source, output).map_err(|error| error.to_string())?;
            Ok(())
        }
        "BKC" => {
            convert_bkc(&source, &output).map_err(|error| error.to_string())?;
            Ok(())
        }
        "BKF" => Err(
            "יצוא BKF דורש Sidecar, DjVu Renderer ומחבר PDF; המימוש אינו שלם עדיין".into(),
        ),
        _ => Err("הפורמט אינו נתמך ליצוא PDF".into()),
    }
}

#[tauri::command]
fn index_document_text(
    document_id: String,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let document = get_document(&state.catalog, &document_id)?;
    let source = resolve_document_path(&state.catalog, &document)?;

    let extracted = match document.format.as_str() {
        "PDF" => extract_pdf_pages(&source).map_err(|error| error.to_string())?,
        "BKC" => {
            let preview = prepare_bkc_preview(&state, document.clone(), &source)?;
            let path = preview.local_path.ok_or("לא נוצר PDF זמני")?;
            extract_pdf_pages(Path::new(&path)).map_err(|error| error.to_string())?
        }
        "BKF" => extract_bkf_text_sidecar(&source).map_err(|error| error.to_string())?,
        _ => return Err("אין Text Extractor לפורמט זה".into()),
    };

    let pages = extracted
        .into_iter()
        .map(|page| PageText {
            repository_id: document.repository_id.clone(),
            document_id: document.id.clone(),
            document_name: document.name.clone(),
            repository_name: document.repository_name.clone(),
            page_index: page.page_index,
            text: page.text,
            text_source: page.source,
        })
        .collect::<Vec<_>>();

    let count = state
        .search
        .replace_document_pages(&document.id, &pages)
        .map_err(|error| error.to_string())?;
    state
        .catalog
        .set_text_indexed(&document.id, true)
        .map_err(|error| error.to_string())?;
    Ok(count)
}

#[tauri::command]
fn search_library(
    query: String,
    repository_ids: Vec<String>,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<SearchHit>, String> {
    let hits = state
        .search
        .search(&query, &repository_ids, limit.min(500))
        .map_err(|error| error.to_string())?;
    Ok(hits
        .into_iter()
        .filter(|hit| {
            state
                .catalog
                .document(&hit.document_id)
                .ok()
                .flatten()
                .is_some_and(|document| document.text_indexed)
        })
        .collect())
}

#[tauri::command]
fn open_local_path(path: String) -> Result<(), String> {
    let value = PathBuf::from(path)
        .canonicalize()
        .map_err(|error| format!("הנתיב אינו זמין: {error}"))?;
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(&value).status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("explorer").arg(&value).status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let status = std::process::Command::new("xdg-open").arg(&value).status();

    status
        .map_err(|error| error.to_string())
        .and_then(|result| {
            if result.success() {
                Ok(())
            } else {
                Err("מערכת ההפעלה לא הצליחה לפתוח את הנתיב".into())
            }
        })
}

fn cache_pdf_path(cache_dir: &Path, document: &Document) -> PathBuf {
    cache_dir.join("pdf").join(format!(
        "{}-{}-{}.pdf",
        document.id,
        document.size,
        document.modified_ms.unwrap_or(0)
    ))
}

fn get_document(catalog: &Catalog, id: &str) -> Result<Document, String> {
    catalog
        .document(id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "המסמך אינו קיים".into())
}

fn resolve_document_path(catalog: &Catalog, document: &Document) -> Result<PathBuf, String> {
    let repository = catalog
        .repository(&document.repository_id)
        .map_err(|error| error.to_string())?
        .ok_or("המאגר אינו קיים")?;
    let root = PathBuf::from(repository.root_path)
        .canonicalize()
        .map_err(|error| format!("המאגר אינו מחובר: {error}"))?;
    let path = root
        .join(&document.relative_path)
        .canonicalize()
        .map_err(|error| format!("המסמך אינו זמין: {error}"))?;
    if !path.starts_with(&root) {
        return Err("נתיב מסמך אינו בטוח".into());
    }
    Ok(path)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data)?;
            let catalog = Catalog::open(app_data.join("library.sqlite3"))?;
            let search = SearchEngine::open(app_data.join("search-index"))
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let server = bkf_local_api::spawn(catalog.clone(), search.clone(), app_data.join("cache"), LOCAL_API_PORT)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            app.manage(AppState {
                catalog,
                search,
                cache_dir: app_data.join("cache"),
                server,
                scans: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bootstrap,
            list_repositories,
            add_repository,
            start_repository_scan,
            cancel_repository_scan,
            list_documents,
            prepare_document_preview,
            export_document_pdf,
            index_document_text,
            search_library,
            open_local_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running BKF AI");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_api_port_is_loopback_service_port() {
        assert_eq!(LOCAL_API_PORT, 47_831);
    }

    #[test]
    fn uuid_is_available_for_future_jobs() {
        assert!(!uuid::Uuid::new_v4().to_string().is_empty());
    }
}

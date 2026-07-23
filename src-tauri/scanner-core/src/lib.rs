use bkf_catalog::{Catalog, PendingDocument, Repository};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use walkdir::WalkDir;

pub const PREFIX_LIMIT: u64 = 16;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub repository_id: String,
    pub scanned: u64,
    pub changed: u64,
    pub errors: u64,
    pub status: String,
    pub current_path: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    pub include_pdf: bool,
    pub include_unknown_book_files: bool,
    pub batch_size: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            include_pdf: true,
            include_unknown_book_files: true,
            batch_size: 500,
        }
    }
}

#[derive(Debug)]
pub enum ScanError {
    Io(io::Error),
    Database(rusqlite::Error),
    InvalidRepository(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Database(error) => write!(formatter, "{error}"),
            Self::InvalidRepository(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for ScanError {}

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

pub fn scan_repository<F>(
    catalog: &Catalog,
    repository: &Repository,
    options: ScanOptions,
    cancelled: &AtomicBool,
    mut progress: F,
) -> Result<ScanProgress, ScanError>
where
    F: FnMut(ScanProgress),
{
    let root = PathBuf::from(&repository.root_path)
        .canonicalize()
        .map_err(|error| ScanError::InvalidRepository(format!("המאגר אינו זמין: {error}")))?;
    if !root.is_dir() {
        return Err(ScanError::InvalidRepository("המאגר אינו תיקייה".into()));
    }

    let generation = now_ms();
    let scan_id = Uuid::new_v4().to_string();
    catalog.begin_scan(&scan_id, &repository.id, generation, generation)?;
    let existing = catalog.existing_fingerprints(&repository.id)?;

    let mut state = ScanProgress {
        repository_id: repository.id.clone(),
        scanned: 0,
        changed: 0,
        errors: 0,
        status: "running".into(),
        current_path: None,
    };
    progress(state.clone());

    let mut batch = Vec::with_capacity(options.batch_size);
    let mut disconnected = false;

    for entry in WalkDir::new(&root).follow_links(false).into_iter() {
        if cancelled.load(Ordering::Acquire) {
            flush(catalog, &mut batch)?;
            state.status = "cancelled".into();
            catalog.update_scan(
                &scan_id,
                &state.status,
                state.scanned,
                state.changed,
                state.errors,
                now_ms(),
            )?;
            catalog.set_repository_scan_status(&repository.id, &state.status, None)?;
            progress(state.clone());
            return Ok(state);
        }

        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                state.errors += 1;
                if error
                    .io_error()
                    .is_some_and(|value| value.kind() == io::ErrorKind::NotFound)
                {
                    disconnected = true;
                    break;
                }
                continue;
            }
        };

        if !entry.file_type().is_file() || !should_consider(entry.path(), options) {
            continue;
        }

        state.scanned += 1;
        let path = entry.path();
        let relative_path = relative_string(&root, path);
        state.current_path = Some(relative_path.clone());

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => {
                state.errors += 1;
                continue;
            }
        };
        let size = metadata.len();
        let modified_ms = metadata.modified().ok().and_then(system_time_ms);

        let existing_item = existing.get(&relative_path);
        let unchanged =
            existing_item.is_some_and(|item| item.size == size && item.modified_ms == modified_ms);

        let (id, format, status, support_status) = if unchanged {
            let item = existing_item.expect("checked above");
            (
                item.id.clone(),
                item.format.clone(),
                item.status.clone(),
                item.support_status.clone(),
            )
        } else {
            state.changed += 1;
            let format = classify_file(path).unwrap_or("Unknown").to_string();
            let support_status = match format.as_str() {
                "BKC" => "unknown",
                "BKF" => "unknown",
                "PDF" => "exact",
                _ => "unsupported",
            }
            .to_string();
            (
                existing_item
                    .map(|item| item.id.clone())
                    .unwrap_or_else(|| Uuid::new_v4().to_string()),
                format,
                "ready".into(),
                support_status,
            )
        };

        batch.push(PendingDocument {
            id,
            repository_id: repository.id.clone(),
            name: entry.file_name().to_string_lossy().into_owned(),
            relative_path,
            size,
            modified_ms,
            format,
            status,
            support_status,
            seen_generation: generation,
        });

        if batch.len() >= options.batch_size {
            flush(catalog, &mut batch)?;
        }

        if state.scanned % 500 == 0 {
            catalog.update_scan(
                &scan_id,
                "running",
                state.scanned,
                state.changed,
                state.errors,
                now_ms(),
            )?;
            progress(state.clone());
        }
    }

    flush(catalog, &mut batch)?;
    state.status = if disconnected {
        "disconnected"
    } else if state.errors > 0 {
        "completed_with_errors"
    } else {
        "completed"
    }
    .into();
    state.current_path = None;

    catalog.update_scan(
        &scan_id,
        &state.status,
        state.scanned,
        state.changed,
        state.errors,
        now_ms(),
    )?;

    if !disconnected {
        catalog.finish_scan(&repository.id, generation, &state.status, now_ms())?;
    } else {
        catalog.set_repository_scan_status(&repository.id, &state.status, None)?;
    }

    progress(state.clone());
    Ok(state)
}

fn should_consider(path: &Path, options: ScanOptions) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match extension.as_str() {
        "book" | "bkc" | "bkf" => true,
        "pdf" => options.include_pdf,
        _ => false,
    }
}

fn flush(catalog: &Catalog, batch: &mut Vec<PendingDocument>) -> Result<(), ScanError> {
    if !batch.is_empty() {
        catalog.upsert_documents(batch)?;
        batch.clear();
    }
    Ok(())
}

pub fn classify_file(path: &Path) -> io::Result<&'static str> {
    let mut file = OpenOptions::new().read(true).write(false).open(path)?;
    classify_reader(&mut file)
}

pub fn classify_reader(reader: &mut impl Read) -> io::Result<&'static str> {
    let mut prefix = Vec::with_capacity(PREFIX_LIMIT as usize);
    reader.take(PREFIX_LIMIT).read_to_end(&mut prefix)?;

    Ok(if prefix.starts_with(b"BKC") {
        "BKC"
    } else if prefix.starts_with(b"BKF") {
        "BKF"
    } else if prefix.starts_with(b"%PDF-") {
        "PDF"
    } else {
        "Unknown"
    })
}

fn relative_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn now_ms() -> i64 {
    system_time_ms(SystemTime::now()).unwrap_or(0)
}

fn system_time_ms(time: SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn detects_magic_without_extension() {
        assert_eq!(
            classify_reader(&mut Cursor::new(b"BKC payload")).unwrap(),
            "BKC"
        );
        assert_eq!(
            classify_reader(&mut Cursor::new(b"BKF payload")).unwrap(),
            "BKF"
        );
        assert_eq!(
            classify_reader(&mut Cursor::new(b"%PDF-1.7")).unwrap(),
            "PDF"
        );
    }

    #[test]
    fn reads_only_prefix_limit() {
        let mut input = Cursor::new(vec![b'x'; 4096]);
        assert_eq!(classify_reader(&mut input).unwrap(), "Unknown");
        assert_eq!(input.position(), PREFIX_LIMIT);
    }
}

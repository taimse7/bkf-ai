use bkf_converter_core::convert_bkc_with_control;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionJob {
    pub id: String,
    pub input_path: String,
    pub output_path: String,
    pub name: String,
    pub file_type: String,
    pub total_bytes: u64,
    pub processed_bytes: u64,
    pub status: String,
    pub error: Option<String>,
    pub technical_report: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct StoredQueue { jobs: Vec<ConversionJob> }

pub struct ConversionState {
    jobs: Mutex<Vec<ConversionJob>>,
    cancellations: Mutex<HashMap<String, Arc<AtomicBool>>>,
    worker_running: AtomicBool,
    path: PathBuf,
}

impl ConversionState {
    pub fn load(path: PathBuf) -> Self {
        let mut jobs = fs::read(&path).ok()
            .and_then(|bytes| serde_json::from_slice::<StoredQueue>(&bytes).ok())
            .map_or_else(Vec::new, |stored| stored.jobs);
        for job in &mut jobs {
            if job.status == "running" { job.status = "queued".into(); }
        }
        Self { jobs: Mutex::new(jobs), cancellations: Mutex::new(HashMap::new()), worker_running: AtomicBool::new(false), path }
    }

    fn save(&self) {
        let jobs = self.jobs.lock().expect("conversion queue poisoned").clone();
        if let Some(parent) = self.path.parent() { let _ = fs::create_dir_all(parent); }
        let temp = self.path.with_extension("json.tmp");
        if let Ok(bytes) = serde_json::to_vec_pretty(&StoredQueue { jobs }) {
            if fs::write(&temp, bytes).is_ok() { let _ = fs::rename(temp, &self.path); }
        }
    }

    pub fn snapshot(&self) -> Vec<ConversionJob> {
        self.jobs.lock().expect("conversion queue poisoned").clone()
    }

    pub fn add(&self, jobs: Vec<ConversionJob>) {
        self.jobs.lock().expect("conversion queue poisoned").extend(jobs);
        self.save();
    }

    pub fn cancel_all(&self) {
        for flag in self.cancellations.lock().expect("cancellations poisoned").values() {
            flag.store(true, Ordering::Relaxed);
        }
        let mut jobs = self.jobs.lock().expect("conversion queue poisoned");
        for job in jobs.iter_mut().filter(|job| job.status == "queued") { job.status = "cancelled".into(); }
        drop(jobs);
        self.save();
    }

    pub fn retry(&self, id: &str) -> bool {
        let mut jobs = self.jobs.lock().expect("conversion queue poisoned");
        let Some(job) = jobs.iter_mut().find(|job| job.id == id && matches!(job.status.as_str(), "failed" | "cancelled" | "disconnected")) else { return false; };
        job.status = "queued".into(); job.processed_bytes = 0; job.error = None; job.technical_report = None;
        drop(jobs); self.save(); true
    }
}

pub fn make_job(input: PathBuf, destination: &Path, name: String, file_type: String, size: u64, rename: bool) -> ConversionJob {
    let stem = Path::new(&name).file_stem().unwrap_or_default().to_string_lossy();
    let mut output = destination.join(format!("{stem}.pdf"));
    if rename {
        let mut suffix = 2;
        while output.exists() {
            output = destination.join(format!("{stem} ({suffix}).pdf")); suffix += 1;
        }
    }
    let status = if file_type == "BKF" { "unsupported" } else if output.exists() { "skipped" } else { "queued" };
    let processed_bytes = if matches!(status, "unsupported" | "skipped") { size } else { 0 };
    ConversionJob { id: Uuid::new_v4().to_string(), input_path: input.to_string_lossy().into(), output_path: output.to_string_lossy().into(), name, file_type, total_bytes: size, processed_bytes, status: status.into(), error: None, technical_report: None }
}

pub fn verify_destination(path: &Path) -> Result<PathBuf, String> {
    let canonical = path.canonicalize().map_err(|error| format!("תיקיית היעד אינה זמינה: {error}"))?;
    if !canonical.is_dir() { return Err("היעד אינו תיקייה".into()); }
    let probe = canonical.join(format!(".bkf-ai-write-test-{}", Uuid::new_v4()));
    OpenOptions::new().write(true).create_new(true).open(&probe)
        .map_err(|error| format!("אין הרשאת כתיבה בתיקיית היעד: {error}"))?;
    fs::remove_file(probe).map_err(|error| format!("בדיקת הרשאות נכשלה: {error}"))?;
    Ok(canonical)
}

pub fn start_worker(app: AppHandle, state: Arc<ConversionState>) {
    if state.worker_running.swap(true, Ordering::SeqCst) { return; }
    std::thread::spawn(move || {
        loop {
            let next = {
                let jobs = state.jobs.lock().expect("conversion queue poisoned");
                jobs.iter().position(|job| job.status == "queued")
            };
            let Some(index) = next else { break; };
            let (job_id, input, output) = {
                let mut jobs = state.jobs.lock().expect("conversion queue poisoned");
                let job = &mut jobs[index]; job.status = "running".into();
                (job.id.clone(), PathBuf::from(&job.input_path), PathBuf::from(&job.output_path))
            };
            state.save(); let _ = app.emit("conversion-progress", state.snapshot());
            let cancel = Arc::new(AtomicBool::new(false));
            state.cancellations.lock().expect("cancellations poisoned").insert(job_id.clone(), cancel.clone());
            let result = convert_bkc_with_control(&input, &output, |processed| {
                let mut jobs = state.jobs.lock().expect("conversion queue poisoned");
                if let Some(job) = jobs.iter_mut().find(|job| job.id == job_id) { job.processed_bytes = processed.min(job.total_bytes); }
                drop(jobs); let _ = app.emit("conversion-progress", state.snapshot());
            }, || cancel.load(Ordering::Relaxed));
            state.cancellations.lock().expect("cancellations poisoned").remove(&job_id);
            let mut jobs = state.jobs.lock().expect("conversion queue poisoned");
            if let Some(job) = jobs.iter_mut().find(|job| job.id == job_id) {
                match result {
                    Ok(report) => { job.status = "completed".into(); job.processed_bytes = job.total_bytes; job.technical_report = serde_json::to_string_pretty(&report).ok(); }
                    Err(error) => {
                        let message = error.to_string();
                        job.status = if message.contains("בוטלה") { "cancelled" } else if !input.exists() { "disconnected" } else { "failed" }.into();
                        job.error = Some(message.clone());
                        job.technical_report = Some(format!("input: {}\noutput: {}\nerror: {}", input.display(), output.display(), message));
                    }
                }
            }
            drop(jobs); state.save(); let _ = app.emit("conversion-progress", state.snapshot());
        }
        state.worker_running.store(false, Ordering::SeqCst);
        if state.jobs.lock().expect("conversion queue poisoned").iter().any(|job| job.status == "queued") { start_worker(app, state); }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!("bkf-ai-queue-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap(); path
    }

    #[test]
    fn existing_output_is_skipped_or_renamed() {
        let destination = temporary_directory();
        fs::write(destination.join("ספר.pdf"), b"existing").unwrap();
        let skipped = make_job(PathBuf::from("/source/book"), &destination, "ספר.book".into(), "BKC".into(), 10, false);
        let renamed = make_job(PathBuf::from("/source/book"), &destination, "ספר.book".into(), "BKC".into(), 10, true);
        assert_eq!(skipped.status, "skipped");
        assert!(renamed.output_path.ends_with("ספר (2).pdf"));
        assert_eq!(renamed.status, "queued");
        fs::remove_dir_all(destination).unwrap();
    }

    #[test]
    fn bkf_never_enters_conversion_queue() {
        let destination = temporary_directory();
        let job = make_job(PathBuf::from("/source/book"), &destination, "ספר.book".into(), "BKF".into(), 10, true);
        assert_eq!(job.status, "unsupported");
        fs::remove_dir_all(destination).unwrap();
    }

    #[test]
    fn interrupted_running_job_is_queued_after_reopen() {
        let directory = temporary_directory();
        let path = directory.join("queue.json");
        let state = ConversionState::load(path.clone());
        let mut job = make_job(PathBuf::from("/source/book"), &directory, "book.book".into(), "BKC".into(), 10, true);
        job.status = "running".into(); state.add(vec![job]);
        let reopened = ConversionState::load(path);
        assert_eq!(reopened.snapshot()[0].status, "queued");
        fs::remove_dir_all(directory).unwrap();
    }
}

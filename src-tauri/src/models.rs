use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub file_type: String,
    pub modified_ms: Option<i64>,
    pub status: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryPage {
    pub items: Vec<LibraryItem>,
    pub total: u64,
    pub offset: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRun {
    pub id: String,
    pub root_path: String,
    pub status: String,
    pub scanned: u64,
    pub errors: u64,
    pub generation: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub scan_id: String,
    pub status: String,
    pub scanned: u64,
    pub errors: u64,
    pub current_path: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PendingItem {
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub file_type: String,
    pub modified_ms: Option<i64>,
    pub status: String,
    pub seen_generation: i64,
}

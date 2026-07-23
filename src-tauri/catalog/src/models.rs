use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: String,
    pub display_name: String,
    pub root_path: String,
    pub connected: bool,
    pub scan_status: String,
    pub document_count: u64,
    pub indexed_count: u64,
    pub last_scan_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: String,
    pub repository_id: String,
    pub repository_name: String,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub format: String,
    pub status: String,
    pub support_status: String,
    pub page_count: Option<u64>,
    pub text_indexed: bool,
    pub cache_pdf_path: Option<String>,
    pub seen_generation: i64,
}

#[derive(Debug, Clone)]
pub struct PendingDocument {
    pub id: String,
    pub repository_id: String,
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub format: String,
    pub status: String,
    pub support_status: String,
    pub seen_generation: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPage {
    pub items: Vec<Document>,
    pub total: u64,
    pub offset: u64,
}

#[derive(Debug, Clone)]
pub struct Fingerprint {
    pub id: String,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub format: String,
    pub status: String,
    pub support_status: String,
}

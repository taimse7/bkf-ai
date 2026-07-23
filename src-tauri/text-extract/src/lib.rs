use serde::Deserialize;
use std::path::Path;

#[derive(Debug)]
pub enum TextExtractError {
    Io(std::io::Error),
    Pdf(String),
    Json(serde_json::Error),
    Unsupported(String),
}

impl std::fmt::Display for TextExtractError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Pdf(error) => formatter.write_str(error),
            Self::Json(error) => write!(formatter, "{error}"),
            Self::Unsupported(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for TextExtractError {}

impl From<std::io::Error> for TextExtractError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for TextExtractError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug, Clone)]
pub struct ExtractedPage {
    pub page_index: u32,
    pub text: String,
    pub source: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextSidecar {
    pages: Vec<TextSidecarPage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextSidecarPage {
    page_index: u32,
    text: String,
}

pub fn extract_pdf_pages(path: &Path) -> Result<Vec<ExtractedPage>, TextExtractError> {
    let pages = pdf_extract::extract_text_by_pages(path)
        .map_err(|error| TextExtractError::Pdf(error.to_string()))?;
    Ok(pages
        .into_iter()
        .enumerate()
        .filter_map(|(index, text)| {
            let text = text.trim().to_string();
            (!text.is_empty()).then_some(ExtractedPage {
                page_index: index as u32,
                text,
                source: "pdf-text-layer".into(),
            })
        })
        .collect())
}

pub fn extract_bkf_text_sidecar(book_path: &Path) -> Result<Vec<ExtractedPage>, TextExtractError> {
    let candidates = [
        book_path.with_extension("bkf-text.json"),
        std::path::PathBuf::from(format!("{}.bkf-text.json", book_path.display())),
    ];
    let path = candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            TextExtractError::Unsupported("לא נמצא Text Sidecar מתאים לקובץ BKF".into())
        })?;
    let sidecar: TextSidecar = serde_json::from_slice(&std::fs::read(path)?)?;
    Ok(sidecar
        .pages
        .into_iter()
        .map(|page| ExtractedPage {
            page_index: page.page_index,
            text: page.text,
            source: "bkf-text-sidecar".into(),
        })
        .collect())
}

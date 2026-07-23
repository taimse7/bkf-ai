use axum::body::Body;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bkf_bkf_core::{discover_sidecar, read_sidecar, validate_sidecar_for_book};
use bkf_catalog::{Catalog, Document};
use bkf_converter_core::{analyze_bkc, convert_bkc, BkcSupport};
use bkf_search_core::SearchEngine;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Debug)]
pub enum ApiError {
    Io(std::io::Error),
    Startup(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Startup(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<std::io::Error> for ApiError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerHandle {
    pub port: u16,
    pub token: String,
    pub url: String,
}

#[derive(Clone)]
struct ApiState {
    catalog: Catalog,
    search: SearchEngine,
    cache_dir: PathBuf,
    token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Health {
    ok: bool,
    service: &'static str,
    version: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareResponse {
    kind: String,
    document_id: String,
    title: String,
    page_count: Option<u64>,
    support_status: String,
    message: Option<String>,
    pdf_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentsQuery {
    repository_ids: Option<String>,
    query: Option<String>,
    format: Option<String>,
    offset: Option<u64>,
    limit: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchRequest {
    query: String,
    repository_ids: Vec<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

pub fn spawn(
    catalog: Catalog,
    search: SearchEngine,
    cache_dir: PathBuf,
    port: u16,
) -> Result<ServerHandle, ApiError> {
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let listener = TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;

    let token = Uuid::new_v4().simple().to_string();
    let state = Arc::new(ApiState {
        catalog,
        search,
        cache_dir,
        token: token.clone(),
    });

    std::thread::Builder::new()
        .name("bkf-local-api".into())
        .spawn(move || {
            let runtime = tokio::runtime::Runtime::new()
                .expect("failed to create local API runtime");
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)
                    .expect("failed to adopt local API listener");
                let app = Router::new()
                    .route("/api/v1/health", get(health))
                    .route("/api/v1/repositories", get(repositories))
                    .route("/api/v1/documents", get(documents))
                    .route("/api/v1/search", post(search))
                    .route("/api/v1/documents/:id/prepare", post(prepare))
                    .route("/api/v1/documents/:id/pdf", get(pdf))
                    .layer(CorsLayer::permissive())
                    .with_state(state);
                if let Err(error) = axum::serve(listener, app).await {
                    eprintln!("local API stopped: {error}");
                }
            });
        })
        .map_err(|error| ApiError::Startup(error.to_string()))?;

    Ok(ServerHandle {
        port,
        token,
        url: format!("http://127.0.0.1:{port}"),
    })
}

async fn health() -> Json<Health> {
    Json(Health {
        ok: true,
        service: "bkf-ai-local-engine",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn repositories(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&state, &headers, None) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.catalog.list_repositories() {
        Ok(rows) => Json(rows).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn documents(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Query(query): Query<DocumentsQuery>,
) -> Response {
    if !authorized(&state, &headers, None) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let repository_ids = query
        .repository_ids
        .unwrap_or_default()
        .split(',')
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    match state.catalog.list_documents(
        &repository_ids,
        query.query.as_deref().unwrap_or(""),
        query.format.as_deref().unwrap_or(""),
        query.offset.unwrap_or(0),
        query.limit.unwrap_or(100),
    ) {
        Ok(page) => Json(page).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn search(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(request): Json<SearchRequest>,
) -> Response {
    if !authorized(&state, &headers, None) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.search.search(
        &request.query,
        &request.repository_ids,
        request.limit.unwrap_or(100),
    ) {
        Ok(hits) => {
            let filtered = hits
                .into_iter()
                .filter(|hit| {
                    state
                        .catalog
                        .document(&hit.document_id)
                        .ok()
                        .flatten()
                        .is_some_and(|document| document.text_indexed)
                })
                .collect::<Vec<_>>();
            Json(filtered).into_response()
        }
        Err(error) => internal_error(error),
    }
}

async fn prepare(
    State(state): State<Arc<ApiState>>,
    AxumPath(document_id): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&state, &headers, None) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    match prepare_document(&state, &document_id) {
        Ok(response) => Json(response).into_response(),
        Err((status, message)) => (status, Json(serde_json::json!({ "error": message }))).into_response(),
    }
}

fn prepare_document(
    state: &ApiState,
    document_id: &str,
) -> Result<PrepareResponse, (StatusCode, String)> {
    let document = state
        .catalog
        .document(document_id)
        .map_err(internal_tuple)?
        .ok_or((StatusCode::NOT_FOUND, "document not found".into()))?;
    let source = resolve_document_path(&state.catalog, &document)
        .map_err(|message| (StatusCode::CONFLICT, message))?;

    match document.format.as_str() {
        "PDF" => {
            let cache = cache_pdf_path(&state.cache_dir, &document);
            if !cache.is_file() {
                if let Some(parent) = cache.parent() {
                    std::fs::create_dir_all(parent).map_err(internal_tuple)?;
                }
                std::fs::copy(&source, &cache).map_err(internal_tuple)?;
            }
            state
                .catalog
                .set_preview(
                    &document.id,
                    "exact",
                    Some(&cache.to_string_lossy()),
                    document.page_count,
                )
                .map_err(internal_tuple)?;
            Ok(PrepareResponse {
                kind: "pdf".into(),
                document_id: document.id.clone(),
                title: document.name,
                page_count: document.page_count,
                support_status: "exact".into(),
                message: None,
                pdf_url: Some(format!("/api/v1/documents/{}/pdf", document.id)),
            })
        }
        "BKC" => {
            let analysis = analyze_bkc(&source)
                .map_err(|error| (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
            if matches!(analysis.support, BkcSupport::RepairUnavailable) {
                return Ok(PrepareResponse {
                    kind: "unsupported".into(),
                    document_id: document.id,
                    title: document.name,
                    page_count: document.page_count,
                    support_status: "unsupported".into(),
                    message: Some("נדרש Ghostscript עבור וריאנט BKC זה".into()),
                    pdf_url: None,
                });
            }

            let cache = cache_pdf_path(&state.cache_dir, &document);
            if !cache.is_file() {
                if let Some(parent) = cache.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(internal_tuple)?;
                }
                convert_bkc(&source, &cache)
                    .map_err(|error| (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
            }

            let support_status = if matches!(analysis.support, BkcSupport::Exact) {
                "exact"
            } else {
                "repair"
            };
            state
                .catalog
                .set_preview(
                    &document.id,
                    support_status,
                    Some(&cache.to_string_lossy()),
                    document.page_count,
                )
                .map_err(internal_tuple)?;

            Ok(PrepareResponse {
                kind: "pdf".into(),
                document_id: document.id.clone(),
                title: document.name,
                page_count: document.page_count,
                support_status: support_status.into(),
                message: Some(if support_status == "exact" {
                    "BKC שוחזר באמצעות פרופיל מאומת".into()
                } else {
                    "BKC נבנה מחדש באמצעות Repair".into()
                }),
                pdf_url: Some(format!("/api/v1/documents/{}/pdf", document.id)),
            })
        }
        "BKF" => {
            let Some(sidecar_path) = discover_sidecar(&source) else {
                return Ok(PrepareResponse {
                    kind: "bkf".into(),
                    document_id: document.id,
                    title: document.name,
                    page_count: None,
                    support_status: "unsupported".into(),
                    message: Some("לא נמצא Sidecar תואם".into()),
                    pdf_url: None,
                });
            };
            let sidecar = read_sidecar(&sidecar_path)
                .map_err(|error| (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
            validate_sidecar_for_book(&sidecar, &source, false)
                .map_err(|error| (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
            state
                .catalog
                .set_preview(
                    &document.id,
                    "renderer_required",
                    None,
                    Some(sidecar.header.page_count as u64),
                )
                .map_err(internal_tuple)?;
            Ok(PrepareResponse {
                kind: "bkf".into(),
                document_id: document.id,
                title: document.name,
                page_count: Some(sidecar.header.page_count as u64),
                support_status: "renderer_required".into(),
                message: Some("Sidecar תקין נמצא, אך DjVu Renderer עדיין אינו מחובר".into()),
                pdf_url: None,
            })
        }
        _ => Ok(PrepareResponse {
            kind: "unsupported".into(),
            document_id: document.id,
            title: document.name,
            page_count: None,
            support_status: "unsupported".into(),
            message: Some("הפורמט אינו נתמך".into()),
            pdf_url: None,
        }),
    }
}

async fn pdf(
    State(state): State<Arc<ApiState>>,
    AxumPath(document_id): AxumPath<String>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Response {
    if !authorized(&state, &headers, query.token.as_deref()) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let document = match state.catalog.document(&document_id) {
        Ok(Some(document)) => document,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error(error),
    };
    let Some(path) = document.cache_pdf_path else {
        return (
            StatusCode::CONFLICT,
            "PDF cache is not ready. Call the prepare endpoint first.",
        )
            .into_response();
    };

    let mut file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let size = match file.metadata().await {
        Ok(metadata) => metadata.len(),
        Err(error) => return internal_error(error),
    };

    let requested_range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_byte_range(value, size));

    let (status, start, end) = requested_range
        .map(|(start, end)| (StatusCode::PARTIAL_CONTENT, start, end))
        .unwrap_or((StatusCode::OK, 0, size.saturating_sub(1)));

    if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
    }

    let length = if size == 0 { 0 } else { end.saturating_sub(start) + 1 };
    let body = Body::from_stream(ReaderStream::new(file.take(length)));
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/pdf"),
    );
    response.headers_mut().insert(
        header::ACCEPT_RANGES,
        header::HeaderValue::from_static("bytes"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, max-age=60"),
    );
    if let Ok(value) = header::HeaderValue::from_str(&length.to_string()) {
        response.headers_mut().insert(header::CONTENT_LENGTH, value);
    }
    if status == StatusCode::PARTIAL_CONTENT {
        if let Ok(value) = header::HeaderValue::from_str(&format!(
            "bytes {start}-{end}/{size}"
        )) {
            response.headers_mut().insert(header::CONTENT_RANGE, value);
        }
    }
    response
}


fn parse_byte_range(value: &str, size: u64) -> Option<(u64, u64)> {
    if size == 0 {
        return None;
    }
    let range = value.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (start, end) = range.split_once('-')?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().ok()?.min(size);
        return Some((size - suffix, size - 1));
    }
    let start = start.parse::<u64>().ok()?;
    if start >= size {
        return None;
    }
    let end = if end.is_empty() {
        size - 1
    } else {
        end.parse::<u64>().ok()?.min(size - 1)
    };
    (start <= end).then_some((start, end))
}


fn cache_pdf_path(cache_dir: &Path, document: &Document) -> PathBuf {
    cache_dir.join("pdf").join(format!(
        "{}-{}-{}.pdf",
        document.id,
        document.size,
        document.modified_ms.unwrap_or(0)
    ))
}

fn resolve_document_path(catalog: &Catalog, document: &Document) -> Result<PathBuf, String> {
    let repository = catalog
        .repository(&document.repository_id)
        .map_err(|error| error.to_string())?
        .ok_or("repository not found")?;
    let root = PathBuf::from(repository.root_path)
        .canonicalize()
        .map_err(|error| format!("repository unavailable: {error}"))?;
    let source = root
        .join(&document.relative_path)
        .canonicalize()
        .map_err(|error| format!("document unavailable: {error}"))?;
    if !source.starts_with(&root) {
        return Err("unsafe document path".into());
    }
    Ok(source)
}

fn authorized(state: &ApiState, headers: &HeaderMap, query_token: Option<&str>) -> bool {
    if query_token.is_some_and(|value| value == state.token) {
        return true;
    }
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|value| value == state.token)
}

fn internal_tuple(error: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn internal_error(error: impl std::fmt::Display) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": error.to_string() })),
    )
        .into_response()
}


#[cfg(test)]
mod tests {
    use super::parse_byte_range;

    #[test]
    fn parses_http_ranges() {
        assert_eq!(parse_byte_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_byte_range("bytes=900-", 1000), Some((900, 999)));
        assert_eq!(parse_byte_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_byte_range("bytes=1000-", 1000), None);
    }
}

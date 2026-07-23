use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Document, Field, Schema, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

#[derive(Debug)]
pub enum SearchError {
    Tantivy(tantivy::TantivyError),
    Query(tantivy::query::QueryParserError),
    State(String),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tantivy(error) => write!(formatter, "{error}"),
            Self::Query(error) => write!(formatter, "{error}"),
            Self::State(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for SearchError {}

impl From<tantivy::TantivyError> for SearchError {
    fn from(value: tantivy::TantivyError) -> Self {
        Self::Tantivy(value)
    }
}

impl From<tantivy::query::QueryParserError> for SearchError {
    fn from(value: tantivy::query::QueryParserError) -> Self {
        Self::Query(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageText {
    pub repository_id: String,
    pub document_id: String,
    pub document_name: String,
    pub repository_name: String,
    pub page_index: u32,
    pub text: String,
    pub text_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub repository_id: String,
    pub document_id: String,
    pub document_name: String,
    pub repository_name: String,
    pub page_index: u32,
    pub snippet: String,
    pub score: f32,
    pub text_source: String,
}

#[derive(Clone)]
pub struct SearchEngine {
    base_path: Arc<PathBuf>,
    shards: Arc<Mutex<HashMap<String, Arc<SearchShard>>>>,
}

struct SearchShard {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    fields: SearchFields,
}

#[derive(Clone, Copy)]
struct SearchFields {
    repository_id: Field,
    document_id: Field,
    document_name: Field,
    repository_name: Field,
    page_index: Field,
    text: Field,
    normalized_text: Field,
    text_source: Field,
}

impl SearchEngine {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SearchError> {
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)
            .map_err(|error| SearchError::State(error.to_string()))?;
        Ok(Self {
            base_path: Arc::new(path),
            shards: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn path(&self) -> &Path {
        self.base_path.as_ref()
    }

    pub fn replace_document_pages(
        &self,
        document_id: &str,
        pages: &[PageText],
    ) -> Result<usize, SearchError> {
        let repository_id = pages
            .first()
            .map(|page| page.repository_id.as_str())
            .ok_or_else(|| SearchError::State("cannot index an empty page list".into()))?;
        if pages
            .iter()
            .any(|page| page.repository_id != repository_id)
        {
            return Err(SearchError::State(
                "all pages must belong to the same repository".into(),
            ));
        }

        let shard = self.shard(repository_id)?;
        let fields = shard.fields;
        let mut writer = shard
            .writer
            .lock()
            .map_err(|_| SearchError::State("search writer lock poisoned".into()))?;
        writer.delete_term(Term::from_field_text(fields.document_id, document_id));

        for page in pages {
            writer.add_document(doc!(
                fields.repository_id => page.repository_id.clone(),
                fields.document_id => page.document_id.clone(),
                fields.document_name => page.document_name.clone(),
                fields.repository_name => page.repository_name.clone(),
                fields.page_index => page.page_index as u64,
                fields.text => page.text.clone(),
                fields.normalized_text => normalize_hebrew(&page.text),
                fields.text_source => page.text_source.clone(),
            ))?;
        }

        writer.commit()?;
        shard.reader.reload()?;
        Ok(pages.len())
    }

    pub fn search(
        &self,
        query: &str,
        repository_ids: &[String],
        limit: usize,
    ) -> Result<Vec<SearchHit>, SearchError> {
        let normalized = normalize_hebrew(query);
        if normalized.trim().is_empty() || repository_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_hits = Vec::new();
        let per_shard_limit = limit.max(1);

        for repository_id in repository_ids {
            let shard = self.shard(repository_id)?;
            all_hits.extend(search_shard(&shard, &normalized, per_shard_limit)?);
        }

        all_hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all_hits.truncate(limit);
        Ok(all_hits)
    }

    fn shard(&self, repository_id: &str) -> Result<Arc<SearchShard>, SearchError> {
        validate_repository_id(repository_id)?;
        if let Some(shard) = self
            .shards
            .lock()
            .map_err(|_| SearchError::State("search shard lock poisoned".into()))?
            .get(repository_id)
            .cloned()
        {
            return Ok(shard);
        }

        let shard = Arc::new(SearchShard::open(
            self.base_path.join(repository_id),
        )?);
        let mut shards = self
            .shards
            .lock()
            .map_err(|_| SearchError::State("search shard lock poisoned".into()))?;
        Ok(shards
            .entry(repository_id.to_string())
            .or_insert_with(|| shard.clone())
            .clone())
    }
}

impl SearchShard {
    fn open(path: PathBuf) -> Result<Self, SearchError> {
        std::fs::create_dir_all(&path)
            .map_err(|error| SearchError::State(error.to_string()))?;
        let schema = schema();
        let index = match Index::open_in_dir(&path) {
            Ok(index) => index,
            Err(_) => Index::create_in_dir(&path, schema)?,
        };
        let fields = fields(&index.schema())?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let writer = index.writer(64 * 1024 * 1024)?;
        Ok(Self {
            index,
            reader,
            writer: Mutex::new(writer),
            fields,
        })
    }
}

fn search_shard(
    shard: &SearchShard,
    normalized_query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, SearchError> {
    let fields = shard.fields;
    let parser = QueryParser::for_index(
        &shard.index,
        vec![fields.normalized_text, fields.document_name],
    );
    let query = parser.parse_query(normalized_query)?;
    let searcher = shard.reader.searcher();
    let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

    top_docs
        .into_iter()
        .map(|(score, address)| {
            let document: TantivyDocument = searcher.doc(address)?;
            let text = string_value(&document, fields.text);
            Ok(SearchHit {
                repository_id: string_value(&document, fields.repository_id),
                document_id: string_value(&document, fields.document_id),
                document_name: string_value(&document, fields.document_name),
                repository_name: string_value(&document, fields.repository_name),
                page_index: u64_value(&document, fields.page_index) as u32,
                snippet: make_snippet(&text, query_terms(normalized_query)),
                score,
                text_source: string_value(&document, fields.text_source),
            })
        })
        .collect()
}

fn schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("repository_id", STRING | STORED);
    builder.add_text_field("document_id", STRING | STORED);
    builder.add_text_field("document_name", TEXT | STORED);
    builder.add_text_field("repository_name", TEXT | STORED);
    builder.add_u64_field("page_index", STORED);
    builder.add_text_field("text", STORED);
    builder.add_text_field("normalized_text", TEXT);
    builder.add_text_field("text_source", STRING | STORED);
    builder.build()
}

fn fields(schema: &Schema) -> Result<SearchFields, SearchError> {
    let get = |name: &str| {
        schema
            .get_field(name)
            .map_err(|_| SearchError::State(format!("missing search field: {name}")))
    };
    Ok(SearchFields {
        repository_id: get("repository_id")?,
        document_id: get("document_id")?,
        document_name: get("document_name")?,
        repository_name: get("repository_name")?,
        page_index: get("page_index")?,
        text: get("text")?,
        normalized_text: get("normalized_text")?,
        text_source: get("text_source")?,
    })
}

fn string_value(document: &TantivyDocument, field: Field) -> String {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string()
}

fn u64_value(document: &TantivyDocument, field: Field) -> u64 {
    document
        .get_first(field)
        .and_then(|value| value.as_u64())
        .unwrap_or(0)
}

pub fn normalize_hebrew(input: &str) -> String {
    input
        .chars()
        .filter_map(|character| match character {
            '\u{0591}'..='\u{05C7}' => None,
            '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' => None,
            '״' | '“' | '”' => Some('"'),
            '׳' | '‘' | '’' => Some('\''),
            '־' | '–' | '—' => Some('-'),
            character if character.is_whitespace() => Some(' '),
            character => Some(character),
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn query_terms(query: &str) -> Vec<&str> {
    query
        .split_whitespace()
        .filter(|value| !value.is_empty())
        .collect()
}

fn make_snippet(text: &str, terms: Vec<&str>) -> String {
    if text.is_empty() {
        return String::new();
    }
    let normalized = normalize_hebrew(text);
    let first_byte = terms
        .iter()
        .filter_map(|term| normalized.find(term))
        .min()
        .unwrap_or(0);
    let first_char = normalized[..first_byte].chars().count();
    let characters = normalized.chars().collect::<Vec<_>>();
    let start = first_char.saturating_sub(60);
    let end = (first_char + 150).min(characters.len());
    let mut snippet = characters[start..end].iter().collect::<String>();
    if start > 0 {
        snippet.insert(0, '…');
    }
    if end < characters.len() {
        snippet.push('…');
    }
    snippet
}

fn validate_repository_id(repository_id: &str) -> Result<(), SearchError> {
    if repository_id.is_empty()
        || repository_id
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || character == '-' || character == '_'))
    {
        return Err(SearchError::State(
            "invalid repository id for search shard".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn normalizes_hebrew_marks_and_quotes() {
        assert_eq!(normalize_hebrew("שָׁלוֹם  ״בדיקה״"), "שלום \"בדיקה\"");
    }

    #[test]
    fn indexes_and_merges_repository_shards() {
        let temp = tempdir().unwrap();
        let engine = SearchEngine::open(temp.path()).unwrap();

        for (repository_id, page_index) in [("repo-1", 4), ("repo-2", 8)] {
            engine
                .replace_document_pages(
                    &format!("doc-{repository_id}"),
                    &[PageText {
                        repository_id: repository_id.into(),
                        document_id: format!("doc-{repository_id}"),
                        document_name: "מסמך בדיקה".into(),
                        repository_name: repository_id.into(),
                        page_index,
                        text: "מנוע חיפוש מקומי ומהיר".into(),
                        text_source: "test".into(),
                    }],
                )
                .unwrap();
        }

        let hits = engine
            .search("חיפוש", &["repo-1".into(), "repo-2".into()], 10)
            .unwrap();
        assert_eq!(hits.len(), 2);
    }
}

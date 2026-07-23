mod models;

pub use models::*;

use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Catalog {
    path: PathBuf,
}

impl Catalog {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        }
        let catalog = Self { path };
        catalog.connect()?;
        Ok(catalog)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connect(&self) -> rusqlite::Result<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=5000;

             CREATE TABLE IF NOT EXISTS repositories (
               id TEXT PRIMARY KEY,
               display_name TEXT NOT NULL,
               root_path TEXT NOT NULL UNIQUE,
               scan_status TEXT NOT NULL DEFAULT 'idle',
               last_scan_ms INTEGER,
               created_ms INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS documents (
               id TEXT PRIMARY KEY,
               repository_id TEXT NOT NULL,
               name TEXT NOT NULL,
               relative_path TEXT NOT NULL,
               size INTEGER NOT NULL,
               modified_ms INTEGER,
               format TEXT NOT NULL,
               status TEXT NOT NULL,
               support_status TEXT NOT NULL DEFAULT 'unknown',
               page_count INTEGER,
               text_indexed INTEGER NOT NULL DEFAULT 0,
               cache_pdf_path TEXT,
               seen_generation INTEGER NOT NULL,
               UNIQUE(repository_id, relative_path),
               FOREIGN KEY(repository_id) REFERENCES repositories(id) ON DELETE CASCADE
             );

             CREATE INDEX IF NOT EXISTS idx_documents_repository_path
               ON documents(repository_id, relative_path);
             CREATE INDEX IF NOT EXISTS idx_documents_repository_name
               ON documents(repository_id, name COLLATE NOCASE);
             CREATE INDEX IF NOT EXISTS idx_documents_format
               ON documents(format);

             CREATE TABLE IF NOT EXISTS scan_runs (
               id TEXT PRIMARY KEY,
               repository_id TEXT NOT NULL,
               generation INTEGER NOT NULL,
               status TEXT NOT NULL,
               scanned INTEGER NOT NULL DEFAULT 0,
               changed INTEGER NOT NULL DEFAULT 0,
               errors INTEGER NOT NULL DEFAULT 0,
               started_ms INTEGER NOT NULL,
               updated_ms INTEGER NOT NULL,
               FOREIGN KEY(repository_id) REFERENCES repositories(id) ON DELETE CASCADE
             );",
        )?;
        Ok(connection)
    }

    pub fn add_repository(
        &self,
        root_path: &str,
        display_name: Option<&str>,
        now_ms: i64,
    ) -> rusqlite::Result<Repository> {
        let connection = self.connect()?;
        let id = connection
            .query_row(
                "SELECT id FROM repositories WHERE root_path=?1",
                [root_path],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let default_name = Path::new(root_path)
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("מאגר");
        let name = display_name.filter(|value| !value.trim().is_empty()).unwrap_or(default_name);

        connection.execute(
            "INSERT INTO repositories(id, display_name, root_path, scan_status, created_ms)
             VALUES(?1, ?2, ?3, 'idle', ?4)
             ON CONFLICT(root_path) DO UPDATE SET display_name=excluded.display_name",
            params![id, name, root_path, now_ms],
        )?;
        self.repository(&id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn repository(&self, id: &str) -> rusqlite::Result<Option<Repository>> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT r.id, r.display_name, r.root_path, r.scan_status, r.last_scan_ms,
                        COUNT(d.id),
                        SUM(CASE WHEN d.text_indexed=1 THEN 1 ELSE 0 END)
                 FROM repositories r
                 LEFT JOIN documents d ON d.repository_id=r.id
                 WHERE r.id=?1
                 GROUP BY r.id",
                [id],
                |row| {
                    let root_path: String = row.get(2)?;
                    Ok(Repository {
                        id: row.get(0)?,
                        display_name: row.get(1)?,
                        connected: Path::new(&root_path).is_dir(),
                        root_path,
                        scan_status: row.get(3)?,
                        last_scan_ms: row.get(4)?,
                        document_count: row.get::<_, i64>(5)? as u64,
                        indexed_count: row.get::<_, Option<i64>>(6)?.unwrap_or(0) as u64,
                    })
                },
            )
            .optional()
    }

    pub fn list_repositories(&self) -> rusqlite::Result<Vec<Repository>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT r.id, r.display_name, r.root_path, r.scan_status, r.last_scan_ms,
                    COUNT(d.id),
                    SUM(CASE WHEN d.text_indexed=1 THEN 1 ELSE 0 END)
             FROM repositories r
             LEFT JOIN documents d ON d.repository_id=r.id
             GROUP BY r.id
             ORDER BY r.display_name COLLATE NOCASE",
        )?;
        let rows = statement.query_map([], |row| {
            let root_path: String = row.get(2)?;
            Ok(Repository {
                id: row.get(0)?,
                display_name: row.get(1)?,
                connected: Path::new(&root_path).is_dir(),
                root_path,
                scan_status: row.get(3)?,
                last_scan_ms: row.get(4)?,
                document_count: row.get::<_, i64>(5)? as u64,
                indexed_count: row.get::<_, Option<i64>>(6)?.unwrap_or(0) as u64,
            })
        })?;
        rows.collect()
    }

    pub fn set_repository_scan_status(
        &self,
        repository_id: &str,
        status: &str,
        last_scan_ms: Option<i64>,
    ) -> rusqlite::Result<()> {
        let connection = self.connect()?;
        connection.execute(
            "UPDATE repositories
             SET scan_status=?2, last_scan_ms=COALESCE(?3,last_scan_ms)
             WHERE id=?1",
            params![repository_id, status, last_scan_ms],
        )?;
        Ok(())
    }

    pub fn begin_scan(
        &self,
        scan_id: &str,
        repository_id: &str,
        generation: i64,
        now_ms: i64,
    ) -> rusqlite::Result<()> {
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO scan_runs(id, repository_id, generation, status, started_ms, updated_ms)
             VALUES(?1, ?2, ?3, 'running', ?4, ?4)",
            params![scan_id, repository_id, generation, now_ms],
        )?;
        self.set_repository_scan_status(repository_id, "running", None)
    }

    pub fn update_scan(
        &self,
        scan_id: &str,
        status: &str,
        scanned: u64,
        changed: u64,
        errors: u64,
        now_ms: i64,
    ) -> rusqlite::Result<()> {
        let connection = self.connect()?;
        connection.execute(
            "UPDATE scan_runs
             SET status=?2, scanned=?3, changed=?4, errors=?5, updated_ms=?6
             WHERE id=?1",
            params![
                scan_id,
                status,
                scanned as i64,
                changed as i64,
                errors as i64,
                now_ms
            ],
        )?;
        Ok(())
    }

    pub fn existing_fingerprints(
        &self,
        repository_id: &str,
    ) -> rusqlite::Result<HashMap<String, Fingerprint>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, relative_path, size, modified_ms, format, status, support_status
             FROM documents WHERE repository_id=?1",
        )?;
        let rows = statement.query_map([repository_id], |row| {
            Ok((
                row.get::<_, String>(1)?,
                Fingerprint {
                    id: row.get(0)?,
                    size: row.get::<_, i64>(2)? as u64,
                    modified_ms: row.get(3)?,
                    format: row.get(4)?,
                    status: row.get(5)?,
                    support_status: row.get(6)?,
                },
            ))
        })?;
        rows.collect()
    }

    pub fn upsert_documents(&self, documents: &[PendingDocument]) -> rusqlite::Result<()> {
        if documents.is_empty() {
            return Ok(());
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO documents(
                   id, repository_id, name, relative_path, size, modified_ms,
                   format, status, support_status, seen_generation
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
                 ON CONFLICT(repository_id, relative_path) DO UPDATE SET
                   name=excluded.name,
                   cache_pdf_path=CASE
                     WHEN documents.size != excluded.size
                       OR documents.modified_ms IS NOT excluded.modified_ms
                     THEN NULL ELSE documents.cache_pdf_path END,
                   page_count=CASE
                     WHEN documents.size != excluded.size
                       OR documents.modified_ms IS NOT excluded.modified_ms
                     THEN NULL ELSE documents.page_count END,
                   text_indexed=CASE
                     WHEN documents.size != excluded.size
                       OR documents.modified_ms IS NOT excluded.modified_ms
                     THEN 0 ELSE documents.text_indexed END,
                   size=excluded.size,
                   modified_ms=excluded.modified_ms,
                   format=excluded.format,
                   status=excluded.status,
                   support_status=excluded.support_status,
                   seen_generation=excluded.seen_generation",
            )?;
            for document in documents {
                statement.execute(params![
                    document.id,
                    document.repository_id,
                    document.name,
                    document.relative_path,
                    document.size as i64,
                    document.modified_ms,
                    document.format,
                    document.status,
                    document.support_status,
                    document.seen_generation,
                ])?;
            }
        }
        transaction.commit()
    }

    pub fn finish_scan(
        &self,
        repository_id: &str,
        generation: i64,
        status: &str,
        now_ms: i64,
    ) -> rusqlite::Result<()> {
        let connection = self.connect()?;
        connection.execute(
            "DELETE FROM documents
             WHERE repository_id=?1 AND seen_generation != ?2",
            params![repository_id, generation],
        )?;
        self.set_repository_scan_status(repository_id, status, Some(now_ms))
    }

    pub fn list_documents(
        &self,
        repository_ids: &[String],
        query: &str,
        format: &str,
        offset: u64,
        limit: u64,
    ) -> rusqlite::Result<DocumentPage> {
        let connection = self.connect()?;
        let ids = if repository_ids.is_empty() {
            self.list_repositories()?.into_iter().map(|item| item.id).collect::<Vec<_>>()
        } else {
            repository_ids.to_vec()
        };
        if ids.is_empty() {
            return Ok(DocumentPage { items: Vec::new(), total: 0, offset });
        }

        let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let pattern = format!("%{}%", escape_like(query));
        let format_filter = if matches!(format, "BKC" | "BKF" | "PDF" | "Unknown") {
            format
        } else {
            ""
        };

        let count_sql = format!(
            "SELECT COUNT(*) FROM documents d
             WHERE d.repository_id IN ({placeholders})
               AND d.name LIKE ? ESCAPE '\\' COLLATE NOCASE
               AND (? = '' OR d.format = ?)"
        );
        let mut count_values: Vec<rusqlite::types::Value> =
            ids.iter().cloned().map(Into::into).collect();
        count_values.push(pattern.clone().into());
        count_values.push(format_filter.to_string().into());
        count_values.push(format_filter.to_string().into());
        let total = connection.query_row(
            &count_sql,
            rusqlite::params_from_iter(count_values),
            |row| row.get::<_, i64>(0),
        )? as u64;

        let list_sql = format!(
            "SELECT d.id, d.repository_id, r.display_name, d.name, d.relative_path,
                    d.size, d.modified_ms, d.format, d.status, d.support_status,
                    d.page_count, d.text_indexed, d.cache_pdf_path, d.seen_generation
             FROM documents d
             JOIN repositories r ON r.id=d.repository_id
             WHERE d.repository_id IN ({placeholders})
               AND d.name LIKE ? ESCAPE '\\' COLLATE NOCASE
               AND (? = '' OR d.format = ?)
             ORDER BY d.name COLLATE NOCASE, d.relative_path COLLATE NOCASE
             LIMIT ? OFFSET ?"
        );
        let mut values: Vec<rusqlite::types::Value> =
            ids.into_iter().map(Into::into).collect();
        values.push(pattern.into());
        values.push(format_filter.to_string().into());
        values.push(format_filter.to_string().into());
        values.push((limit.min(500) as i64).into());
        values.push((offset as i64).into());

        let mut statement = connection.prepare(&list_sql)?;
        let items = statement
            .query_map(rusqlite::params_from_iter(values), map_document)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(DocumentPage { items, total, offset })
    }

    pub fn document(&self, id: &str) -> rusqlite::Result<Option<Document>> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT d.id, d.repository_id, r.display_name, d.name, d.relative_path,
                        d.size, d.modified_ms, d.format, d.status, d.support_status,
                        d.page_count, d.text_indexed, d.cache_pdf_path, d.seen_generation
                 FROM documents d
                 JOIN repositories r ON r.id=d.repository_id
                 WHERE d.id=?1",
                [id],
                map_document,
            )
            .optional()
    }

    pub fn set_preview(
        &self,
        document_id: &str,
        support_status: &str,
        cache_pdf_path: Option<&str>,
        page_count: Option<u64>,
    ) -> rusqlite::Result<()> {
        let connection = self.connect()?;
        connection.execute(
            "UPDATE documents
             SET support_status=?2,
                 cache_pdf_path=COALESCE(?3, cache_pdf_path),
                 page_count=COALESCE(?4, page_count)
             WHERE id=?1",
            params![
                document_id,
                support_status,
                cache_pdf_path,
                page_count.map(|value| value as i64)
            ],
        )?;
        Ok(())
    }

    pub fn set_text_indexed(&self, document_id: &str, indexed: bool) -> rusqlite::Result<()> {
        let connection = self.connect()?;
        connection.execute(
            "UPDATE documents SET text_indexed=?2 WHERE id=?1",
            params![document_id, indexed as i64],
        )?;
        Ok(())
    }
}

fn map_document(row: &rusqlite::Row<'_>) -> rusqlite::Result<Document> {
    Ok(Document {
        id: row.get(0)?,
        repository_id: row.get(1)?,
        repository_name: row.get(2)?,
        name: row.get(3)?,
        relative_path: row.get(4)?,
        size: row.get::<_, i64>(5)? as u64,
        modified_ms: row.get(6)?,
        format: row.get(7)?,
        status: row.get(8)?,
        support_status: row.get(9)?,
        page_count: row.get::<_, Option<i64>>(10)?.map(|value| value as u64),
        text_indexed: row.get::<_, i64>(11)? != 0,
        cache_pdf_path: row.get(12)?,
        seen_generation: row.get(13)?,
    })
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn stores_multiple_repositories_and_pages_documents() {
        let temp = tempdir().unwrap();
        let catalog = Catalog::open(temp.path().join("library.sqlite3")).unwrap();
        let first = catalog.add_repository("/tmp/one", Some("אחד"), 1).unwrap();
        let second = catalog.add_repository("/tmp/two", Some("שתיים"), 1).unwrap();
        assert_ne!(first.id, second.id);

        let mut rows = Vec::new();
        for index in 0..10_000 {
            rows.push(PendingDocument {
                id: Uuid::new_v4().to_string(),
                repository_id: first.id.clone(),
                name: format!("קובץ {index}.book"),
                relative_path: format!("{index:05}.book"),
                size: index as u64,
                modified_ms: Some(index as i64),
                format: if index % 2 == 0 { "BKC" } else { "BKF" }.into(),
                status: "ready".into(),
                support_status: "unknown".into(),
                seen_generation: 1,
            });
        }
        for chunk in rows.chunks(500) {
            catalog.upsert_documents(chunk).unwrap();
        }
        let page = catalog
            .list_documents(&[first.id], "", "BKC", 4_950, 100)
            .unwrap();
        assert_eq!(page.total, 5_000);
        assert_eq!(page.items.len(), 50);
    }
}

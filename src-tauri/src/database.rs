use crate::models::{LibraryItem, LibraryPage, PendingItem, ScanRun};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub fn init_database(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    }
    let connection = Connection::open(path)?;
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS scan_runs (
           id TEXT PRIMARY KEY,
           root_path TEXT NOT NULL,
           status TEXT NOT NULL,
           scanned INTEGER NOT NULL DEFAULT 0,
           errors INTEGER NOT NULL DEFAULT 0,
           generation INTEGER NOT NULL,
           started_at INTEGER NOT NULL,
           updated_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS library_items (
           scan_id TEXT NOT NULL,
           relative_path TEXT NOT NULL,
           name TEXT NOT NULL,
           size INTEGER NOT NULL,
           file_type TEXT NOT NULL,
           modified_ms INTEGER,
           status TEXT NOT NULL,
           selected INTEGER NOT NULL DEFAULT 0,
           seen_generation INTEGER NOT NULL,
           PRIMARY KEY (scan_id, relative_path),
           FOREIGN KEY (scan_id) REFERENCES scan_runs(id) ON DELETE CASCADE
         );
         CREATE INDEX IF NOT EXISTS idx_library_items_scan_path
           ON library_items(scan_id, relative_path);",
    )?;
    Ok(connection)
}

pub fn last_scan(connection: &Connection) -> rusqlite::Result<Option<ScanRun>> {
    connection
        .query_row(
            "SELECT id, root_path, status, scanned, errors, generation
             FROM scan_runs ORDER BY updated_at DESC LIMIT 1",
            [],
            |row| {
                Ok(ScanRun {
                    id: row.get(0)?,
                    root_path: row.get(1)?,
                    status: row.get(2)?,
                    scanned: row.get::<_, i64>(3)? as u64,
                    errors: row.get::<_, i64>(4)? as u64,
                    generation: row.get(5)?,
                })
            },
        )
        .optional()
}

pub fn resumable_scan(connection: &Connection, root: &str) -> rusqlite::Result<Option<ScanRun>> {
    connection
        .query_row(
            "SELECT id, root_path, status, scanned, errors, generation
             FROM scan_runs
             WHERE root_path = ?1 AND status != 'completed' AND status != 'completed_with_errors'
             ORDER BY updated_at DESC LIMIT 1",
            [root],
            |row| {
                Ok(ScanRun {
                    id: row.get(0)?,
                    root_path: row.get(1)?,
                    status: row.get(2)?,
                    scanned: row.get::<_, i64>(3)? as u64,
                    errors: row.get::<_, i64>(4)? as u64,
                    generation: row.get(5)?,
                })
            },
        )
        .optional()
}

pub fn begin_scan(
    connection: &Connection,
    id: &str,
    root: &str,
    generation: i64,
    now: i64,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO scan_runs(id, root_path, status, scanned, errors, generation, started_at, updated_at)
         VALUES(?1, ?2, 'running', 0, 0, ?3, ?4, ?4)
         ON CONFLICT(id) DO UPDATE SET status='running', generation=?3, updated_at=?4",
        params![id, root, generation, now],
    )?;
    Ok(())
}

pub fn update_scan(
    connection: &Connection,
    id: &str,
    status: &str,
    scanned: u64,
    errors: u64,
    now: i64,
) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE scan_runs SET status=?2, scanned=?3, errors=?4, updated_at=?5 WHERE id=?1",
        params![id, status, scanned as i64, errors as i64, now],
    )?;
    Ok(())
}

pub fn mark_scan_disconnected(connection: &Connection, id: &str) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE scan_runs SET status='disconnected', updated_at=strftime('%s','now')*1000 WHERE id=?1",
        [id],
    )?;
    Ok(())
}

pub fn unchanged_item(
    connection: &Connection,
    scan_id: &str,
    relative_path: &str,
    size: u64,
    modified_ms: Option<i64>,
) -> rusqlite::Result<Option<(String, String)>> {
    connection
        .query_row(
            "SELECT file_type, status FROM library_items
             WHERE scan_id=?1 AND relative_path=?2 AND size=?3
               AND modified_ms IS ?4",
            params![scan_id, relative_path, size as i64, modified_ms],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
}

pub fn insert_batch(
    connection: &mut Connection,
    scan_id: &str,
    items: &[PendingItem],
) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    {
        let mut statement = transaction.prepare(
            "INSERT INTO library_items(
               scan_id, relative_path, name, size, file_type, modified_ms, status, seen_generation
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(scan_id, relative_path) DO UPDATE SET
               name=excluded.name, size=excluded.size, file_type=excluded.file_type,
               modified_ms=excluded.modified_ms, status=excluded.status,
               seen_generation=excluded.seen_generation",
        )?;
        for item in items {
            statement.execute(params![
                scan_id,
                item.relative_path,
                item.name,
                item.size as i64,
                item.file_type,
                item.modified_ms,
                item.status,
                item.seen_generation,
            ])?;
        }
    }
    transaction.commit()
}

pub fn remove_stale(
    connection: &Connection,
    scan_id: &str,
    generation: i64,
) -> rusqlite::Result<()> {
    connection.execute(
        "DELETE FROM library_items WHERE scan_id=?1 AND seen_generation != ?2",
        params![scan_id, generation],
    )?;
    Ok(())
}

pub fn list_items(
    connection: &Connection,
    scan_id: &str,
    offset: u64,
    limit: u64,
    name_query: &str,
) -> rusqlite::Result<LibraryPage> {
    let pattern = format!("%{}%", escape_like(name_query));
    let total = connection.query_row(
        "SELECT COUNT(*) FROM library_items
         WHERE scan_id=?1 AND name LIKE ?2 ESCAPE '\\' COLLATE NOCASE",
        params![scan_id, pattern],
        |row| row.get::<_, i64>(0),
    )? as u64;
    let mut statement = connection.prepare(
        "SELECT name, relative_path, size, file_type, modified_ms, status, selected
         FROM library_items
         WHERE scan_id=?1 AND name LIKE ?2 ESCAPE '\\' COLLATE NOCASE
         ORDER BY relative_path COLLATE NOCASE LIMIT ?3 OFFSET ?4",
    )?;
    let items = statement
        .query_map(params![scan_id, pattern, limit as i64, offset as i64], |row| {
            Ok(LibraryItem {
                name: row.get(0)?,
                relative_path: row.get(1)?,
                size: row.get::<_, i64>(2)? as u64,
                file_type: row.get(3)?,
                modified_ms: row.get(4)?,
                status: row.get(5)?,
                selected: row.get::<_, i64>(6)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(LibraryPage {
        items,
        total,
        offset,
    })
}

fn escape_like(value: &str) -> String {
    value.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

pub fn set_selected(
    connection: &Connection,
    scan_id: &str,
    relative_path: &str,
    selected: bool,
) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE library_items SET selected=?3 WHERE scan_id=?1 AND relative_path=?2",
        params![scan_id, relative_path, selected as i64],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn pages_through_ten_thousand_real_sqlite_rows() {
        let temp = tempdir().unwrap();
        let mut connection = init_database(&temp.path().join("library.sqlite3")).unwrap();
        begin_scan(&connection, "scan-1", "/מקור", 1, 1).unwrap();
        for chunk_start in (0..10_000).step_by(500) {
            let items: Vec<_> = (chunk_start..chunk_start + 500)
                .map(|index| PendingItem {
                    name: format!("ספר {index}.book"),
                    relative_path: format!("תיקייה/{index:05}.book"),
                    size: index as u64,
                    file_type: if index % 2 == 0 { "BKC" } else { "Unknown" }.into(),
                    modified_ms: Some(index as i64),
                    status: "ready".into(),
                    seen_generation: 1,
                })
                .collect();
            insert_batch(&mut connection, "scan-1", &items).unwrap();
        }
        let page = list_items(&connection, "scan-1", 9_950, 100, "").unwrap();
        assert_eq!(page.total, 10_000);
        assert_eq!(page.items.len(), 50);
        assert_eq!(page.items[0].relative_path, "תיקייה/09950.book");
        let filtered = list_items(&connection, "scan-1", 0, 100, "ספר 9999").unwrap();
        assert_eq!(filtered.total, 1);
        assert_eq!(filtered.items[0].name, "ספר 9999.book");
    }
}

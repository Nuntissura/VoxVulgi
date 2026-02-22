use crate::paths::AppPaths;
use crate::Result;
use rusqlite::{Connection, OpenFlags};
use std::time::Duration;

pub fn open(paths: &AppPaths) -> Result<Connection> {
    paths.ensure_dirs()?;

    let db_path = paths.db_dir().join("app.sqlite");
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )?;

    conn.busy_timeout(Duration::from_secs(10))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    Ok(conn)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS library_item (
  id TEXT PRIMARY KEY,
  created_at_ms INTEGER NOT NULL,
  source_type TEXT NOT NULL,
  source_uri TEXT NOT NULL,
  title TEXT NOT NULL,
  media_path TEXT NOT NULL,
  duration_ms INTEGER,
  width INTEGER,
  height INTEGER,
  container TEXT,
  video_codec TEXT,
  audio_codec TEXT,
  thumbnail_path TEXT
);

CREATE TABLE IF NOT EXISTS ingest_provenance (
  item_id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  source_url TEXT NOT NULL,
  rights_note TEXT NOT NULL,
  attested_at_ms INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  FOREIGN KEY (item_id) REFERENCES library_item(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS tag (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS library_item_tag (
  item_id TEXT NOT NULL,
  tag_id TEXT NOT NULL,
  PRIMARY KEY (item_id, tag_id),
  FOREIGN KEY (item_id) REFERENCES library_item(id) ON DELETE CASCADE,
  FOREIGN KEY (tag_id) REFERENCES tag(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS smart_tag (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS library_item_smart_tag (
  item_id TEXT NOT NULL,
  smart_tag_id TEXT NOT NULL,
  confidence REAL,
  PRIMARY KEY (item_id, smart_tag_id),
  FOREIGN KEY (item_id) REFERENCES library_item(id) ON DELETE CASCADE,
  FOREIGN KEY (smart_tag_id) REFERENCES smart_tag(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS subtitle_track (
  id TEXT PRIMARY KEY,
  item_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  lang TEXT NOT NULL,
  format TEXT NOT NULL,
  path TEXT NOT NULL,
  created_by TEXT NOT NULL,
  version INTEGER NOT NULL,
  FOREIGN KEY (item_id) REFERENCES library_item(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS job (
  id TEXT PRIMARY KEY,
  item_id TEXT,
  batch_id TEXT,
  type TEXT NOT NULL,
  status TEXT NOT NULL,
  progress REAL NOT NULL,
  error TEXT,
  params_json TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  started_at_ms INTEGER,
  finished_at_ms INTEGER,
  logs_path TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_job_status_created ON job(status, created_at_ms);
CREATE INDEX IF NOT EXISTS idx_library_item_created ON library_item(created_at_ms);
CREATE INDEX IF NOT EXISTS idx_ingest_provenance_created ON ingest_provenance(created_at_ms);
"#,
    )?;

    // Backfill older installs that created `job` without `batch_id`.
    let has_batch_id = {
        let mut stmt = conn.prepare("PRAGMA table_info(job)")?;
        let mut rows = stmt.query([])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "batch_id" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_batch_id {
        conn.execute("ALTER TABLE job ADD COLUMN batch_id TEXT", [])?;
    }
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_job_batch_created ON job(batch_id, created_at_ms)",
        [],
    )?;

    let current_schema_version = 3;
    let existing: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    match existing {
        Some(v) if v == current_schema_version.to_string() => {}
        _ => {
            conn.execute(
                "INSERT INTO meta(key, value) VALUES('schema_version', ?)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [current_schema_version.to_string()],
            )?;
        }
    }

    Ok(())
}

pub fn ensure_schema(paths: &AppPaths) -> Result<()> {
    let conn = open(paths)?;
    migrate(&conn)?;
    Ok(())
}

trait OptionalRowExt<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalRowExt<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;

    #[test]
    fn migrate_adds_batch_id_for_legacy_job_table() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        paths.ensure_dirs().expect("ensure dirs");
        let db_path = paths.db_dir().join("app.sqlite");

        {
            let conn = Connection::open(&db_path).expect("open");
            conn.execute_batch(
                r#"
CREATE TABLE IF NOT EXISTS job (
  id TEXT PRIMARY KEY,
  item_id TEXT,
  type TEXT NOT NULL,
  status TEXT NOT NULL,
  progress REAL NOT NULL,
  error TEXT,
  params_json TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  started_at_ms INTEGER,
  finished_at_ms INTEGER,
  logs_path TEXT NOT NULL
);
"#,
            )
            .expect("create legacy job table");
        }

        let conn = open(&paths).expect("open migrated");
        migrate(&conn).expect("migrate");

        let mut stmt = conn
            .prepare("PRAGMA table_info(job)")
            .expect("table_info");
        let mut rows = stmt.query([]).expect("query table_info");
        let mut has_batch_id = false;
        while let Some(row) = rows.next().expect("next row") {
            let name: String = row.get(1).expect("name");
            if name == "batch_id" {
                has_batch_id = true;
                break;
            }
        }
        assert!(has_batch_id, "batch_id column should exist after migrate");
    }
}

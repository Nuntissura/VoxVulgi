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

CREATE TABLE IF NOT EXISTS item_speaker (
  item_id TEXT NOT NULL,
  speaker_key TEXT NOT NULL,
  display_name TEXT,
  voice_profile_id TEXT,
  tts_voice_id TEXT,
  tts_voice_profile_path TEXT,
  tts_voice_profile_paths_json TEXT,
  style_preset TEXT,
  prosody_preset TEXT,
  pronunciation_overrides TEXT,
  render_mode TEXT,
  subtitle_prosody_mode TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (item_id, speaker_key),
  FOREIGN KEY (item_id) REFERENCES library_item(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_item_speaker_item ON item_speaker(item_id);

CREATE TABLE IF NOT EXISTS item_voice_plan (
  item_id TEXT PRIMARY KEY,
  goal TEXT NOT NULL,
  preferred_backend_id TEXT,
  fallback_backend_id TEXT,
  selected_candidate_id TEXT,
  selected_variant_label TEXT,
  notes TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  FOREIGN KEY (item_id) REFERENCES library_item(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_item_voice_plan_updated
  ON item_voice_plan(updated_at_ms);

CREATE TABLE IF NOT EXISTS voice_template (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  goal TEXT,
  preferred_backend_id TEXT,
  fallback_backend_id TEXT,
  selected_variant_label TEXT,
  notes TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS voice_template_speaker (
  template_id TEXT NOT NULL,
  speaker_key TEXT NOT NULL,
  display_name TEXT,
  tts_voice_id TEXT,
  tts_voice_profile_path TEXT,
  tts_voice_profile_paths_json TEXT,
  style_preset TEXT,
  prosody_preset TEXT,
  pronunciation_overrides TEXT,
  render_mode TEXT,
  subtitle_prosody_mode TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (template_id, speaker_key),
  FOREIGN KEY (template_id) REFERENCES voice_template(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS voice_template_reference (
  template_id TEXT NOT NULL,
  speaker_key TEXT NOT NULL,
  reference_id TEXT NOT NULL,
  label TEXT,
  path TEXT NOT NULL,
  sort_order INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (template_id, speaker_key, reference_id),
  FOREIGN KEY (template_id, speaker_key)
    REFERENCES voice_template_speaker(template_id, speaker_key)
    ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_voice_template_updated ON voice_template(updated_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_voice_template_speaker_template
  ON voice_template_speaker(template_id, speaker_key);
CREATE INDEX IF NOT EXISTS idx_voice_template_reference_template
  ON voice_template_reference(template_id, speaker_key, sort_order, created_at_ms);

CREATE TABLE IF NOT EXISTS voice_cast_pack (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  goal TEXT,
  preferred_backend_id TEXT,
  fallback_backend_id TEXT,
  selected_variant_label TEXT,
  notes TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS voice_cast_pack_role (
  pack_id TEXT NOT NULL,
  role_key TEXT NOT NULL,
  display_name TEXT,
  template_id TEXT NOT NULL,
  template_speaker_key TEXT NOT NULL,
  style_preset TEXT,
  prosody_preset TEXT,
  pronunciation_overrides TEXT,
  render_mode TEXT,
  subtitle_prosody_mode TEXT,
  sort_order INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (pack_id, role_key),
  FOREIGN KEY (pack_id) REFERENCES voice_cast_pack(id) ON DELETE CASCADE,
  FOREIGN KEY (template_id, template_speaker_key)
    REFERENCES voice_template_speaker(template_id, speaker_key)
);

CREATE INDEX IF NOT EXISTS idx_voice_cast_pack_updated ON voice_cast_pack(updated_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_voice_cast_pack_role_pack
  ON voice_cast_pack_role(pack_id, sort_order, role_key);

CREATE TABLE IF NOT EXISTS voice_library_profile (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  display_name TEXT,
  tts_voice_id TEXT,
  tts_voice_profile_path TEXT,
  tts_voice_profile_paths_json TEXT,
  style_preset TEXT,
  prosody_preset TEXT,
  pronunciation_overrides TEXT,
  render_mode TEXT,
  subtitle_prosody_mode TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS voice_library_reference (
  profile_id TEXT NOT NULL,
  reference_id TEXT NOT NULL,
  label TEXT,
  path TEXT NOT NULL,
  sort_order INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (profile_id, reference_id),
  FOREIGN KEY (profile_id) REFERENCES voice_library_profile(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_voice_library_profile_kind_updated
  ON voice_library_profile(kind, updated_at_ms DESC, name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_voice_library_reference_profile
  ON voice_library_reference(profile_id, sort_order, created_at_ms);

CREATE TABLE IF NOT EXISTS youtube_subscription (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  source_url TEXT NOT NULL UNIQUE,
  folder_map TEXT NOT NULL,
  output_dir_override TEXT,
  use_browser_cookies INTEGER NOT NULL DEFAULT 0,
  active INTEGER NOT NULL DEFAULT 1,
  preset_id TEXT,
  refresh_interval_minutes INTEGER NOT NULL DEFAULT 60,
  last_queued_at_ms INTEGER,
  last_error_at_ms INTEGER,
  consecutive_failures INTEGER NOT NULL DEFAULT 0,
  next_allowed_refresh_at_ms INTEGER,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_youtube_subscription_active_updated
  ON youtube_subscription(active, updated_at_ms);

CREATE TABLE IF NOT EXISTS youtube_subscription_group (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS youtube_subscription_group_member (
  subscription_id TEXT NOT NULL,
  group_id TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  PRIMARY KEY (subscription_id, group_id),
  FOREIGN KEY (subscription_id) REFERENCES youtube_subscription(id) ON DELETE CASCADE,
  FOREIGN KEY (group_id) REFERENCES youtube_subscription_group(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_youtube_subscription_group_member_group
  ON youtube_subscription_group_member(group_id, subscription_id);

CREATE TABLE IF NOT EXISTS instagram_subscription (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  source_url TEXT NOT NULL UNIQUE,
  folder_map TEXT NOT NULL,
  output_dir_override TEXT,
  use_browser_cookies INTEGER NOT NULL DEFAULT 0,
  active INTEGER NOT NULL DEFAULT 1,
  refresh_interval_minutes INTEGER NOT NULL DEFAULT 60,
  last_queued_at_ms INTEGER,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_instagram_subscription_active_updated
  ON instagram_subscription(active, updated_at_ms);

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

    let has_tts_voice_profile_path = {
        let mut stmt = conn.prepare("PRAGMA table_info(item_speaker)")?;
        let mut rows = stmt.query([])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "tts_voice_profile_path" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_tts_voice_profile_path {
        conn.execute(
            "ALTER TABLE item_speaker ADD COLUMN tts_voice_profile_path TEXT",
            [],
        )?;
    }
    ensure_column(conn, "item_speaker", "tts_voice_profile_paths_json", "TEXT")?;
    ensure_column(conn, "item_speaker", "style_preset", "TEXT")?;
    ensure_column(conn, "item_speaker", "prosody_preset", "TEXT")?;
    ensure_column(conn, "item_speaker", "pronunciation_overrides", "TEXT")?;
    ensure_column(conn, "item_speaker", "render_mode", "TEXT")?;
    ensure_column(conn, "item_speaker", "voice_profile_id", "TEXT")?;
    ensure_column(conn, "item_speaker", "subtitle_prosody_mode", "TEXT")?;
    ensure_column(
        conn,
        "voice_template_speaker",
        "tts_voice_profile_paths_json",
        "TEXT",
    )?;
    ensure_column(conn, "voice_template_speaker", "style_preset", "TEXT")?;
    ensure_column(conn, "voice_template_speaker", "prosody_preset", "TEXT")?;
    ensure_column(
        conn,
        "voice_template_speaker",
        "pronunciation_overrides",
        "TEXT",
    )?;
    ensure_column(conn, "voice_template_speaker", "render_mode", "TEXT")?;
    ensure_column(
        conn,
        "voice_template_speaker",
        "subtitle_prosody_mode",
        "TEXT",
    )?;
    ensure_column(conn, "voice_template", "goal", "TEXT")?;
    ensure_column(conn, "voice_template", "preferred_backend_id", "TEXT")?;
    ensure_column(conn, "voice_template", "fallback_backend_id", "TEXT")?;
    ensure_column(conn, "voice_template", "selected_variant_label", "TEXT")?;
    ensure_column(conn, "voice_template", "notes", "TEXT")?;
    ensure_column(conn, "voice_cast_pack", "goal", "TEXT")?;
    ensure_column(conn, "voice_cast_pack", "preferred_backend_id", "TEXT")?;
    ensure_column(conn, "voice_cast_pack", "fallback_backend_id", "TEXT")?;
    ensure_column(conn, "voice_cast_pack", "selected_variant_label", "TEXT")?;
    ensure_column(conn, "voice_cast_pack", "notes", "TEXT")?;
    ensure_column(
        conn,
        "voice_cast_pack_role",
        "subtitle_prosody_mode",
        "TEXT",
    )?;

    let has_subscription_refresh_interval_minutes = {
        let mut stmt = conn.prepare("PRAGMA table_info(youtube_subscription)")?;
        let mut rows = stmt.query([])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "refresh_interval_minutes" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_subscription_refresh_interval_minutes {
        conn.execute(
            "ALTER TABLE youtube_subscription ADD COLUMN refresh_interval_minutes INTEGER NOT NULL DEFAULT 60",
            [],
        )?;
    }

    let has_subscription_preset_id = {
        let mut stmt = conn.prepare("PRAGMA table_info(youtube_subscription)")?;
        let mut rows = stmt.query([])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "preset_id" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_subscription_preset_id {
        conn.execute(
            "ALTER TABLE youtube_subscription ADD COLUMN preset_id TEXT",
            [],
        )?;
    }

    let has_subscription_last_error_at_ms = {
        let mut stmt = conn.prepare("PRAGMA table_info(youtube_subscription)")?;
        let mut rows = stmt.query([])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "last_error_at_ms" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_subscription_last_error_at_ms {
        conn.execute(
            "ALTER TABLE youtube_subscription ADD COLUMN last_error_at_ms INTEGER",
            [],
        )?;
    }

    let has_subscription_consecutive_failures = {
        let mut stmt = conn.prepare("PRAGMA table_info(youtube_subscription)")?;
        let mut rows = stmt.query([])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "consecutive_failures" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_subscription_consecutive_failures {
        conn.execute(
            "ALTER TABLE youtube_subscription ADD COLUMN consecutive_failures INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }

    let has_subscription_next_allowed_refresh_at_ms = {
        let mut stmt = conn.prepare("PRAGMA table_info(youtube_subscription)")?;
        let mut rows = stmt.query([])?;
        let mut found = false;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "next_allowed_refresh_at_ms" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_subscription_next_allowed_refresh_at_ms {
        conn.execute(
            "ALTER TABLE youtube_subscription ADD COLUMN next_allowed_refresh_at_ms INTEGER",
            [],
        )?;
    }

    conn.execute(
        "CREATE TABLE IF NOT EXISTS youtube_subscription_group (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL UNIQUE,
          created_at_ms INTEGER NOT NULL,
          updated_at_ms INTEGER NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS youtube_subscription_group_member (
          subscription_id TEXT NOT NULL,
          group_id TEXT NOT NULL,
          created_at_ms INTEGER NOT NULL,
          PRIMARY KEY (subscription_id, group_id),
          FOREIGN KEY (subscription_id) REFERENCES youtube_subscription(id) ON DELETE CASCADE,
          FOREIGN KEY (group_id) REFERENCES youtube_subscription_group(id) ON DELETE CASCADE
        )",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_youtube_subscription_group_member_group ON youtube_subscription_group_member(group_id, subscription_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_youtube_subscription_next_allowed ON youtube_subscription(active, next_allowed_refresh_at_ms)",
        [],
    )?;
    ensure_column(
        conn,
        "instagram_subscription",
        "refresh_interval_minutes",
        "INTEGER NOT NULL DEFAULT 60",
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_instagram_subscription_active_updated ON instagram_subscription(active, updated_at_ms)",
        [],
    )?;

    let current_schema_version = 9;
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

fn ensure_column(conn: &Connection, table: &str, column: &str, column_def: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(());
        }
    }

    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {column_def}"),
        [],
    )?;
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

        let mut stmt = conn.prepare("PRAGMA table_info(job)").expect("table_info");
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

    #[test]
    fn migrate_creates_youtube_subscription_table() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");

        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='youtube_subscription'",
            )
            .expect("prepare");
        let found: Option<String> = stmt
            .query_row([], |row| row.get(0))
            .optional()
            .expect("query");
        assert_eq!(found.as_deref(), Some("youtube_subscription"));

        let mut col_stmt = conn
            .prepare("PRAGMA table_info(youtube_subscription)")
            .expect("table_info");
        let mut rows = col_stmt.query([]).expect("table_info query");
        let mut has_refresh_interval = false;
        while let Some(row) = rows.next().expect("next col") {
            let name: String = row.get(1).expect("col name");
            if name == "refresh_interval_minutes" {
                has_refresh_interval = true;
                break;
            }
        }
        assert!(
            has_refresh_interval,
            "refresh_interval_minutes column should exist after migrate"
        );
    }

    #[test]
    fn migrate_creates_voice_template_tables() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");

        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('voice_template', 'voice_template_speaker') ORDER BY name",
            )
            .expect("prepare");
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query");
        let names = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect rows");

        assert_eq!(
            names,
            vec![
                "voice_template".to_string(),
                "voice_template_speaker".to_string()
            ]
        );
    }

    #[test]
    fn migrate_creates_extended_voice_feature_tables() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");

        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('voice_template_reference', 'voice_cast_pack', 'voice_cast_pack_role', 'voice_library_profile', 'voice_library_reference') ORDER BY name",
            )
            .expect("prepare");
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query");
        let names = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect rows");

        assert_eq!(
            names,
            vec![
                "voice_cast_pack".to_string(),
                "voice_cast_pack_role".to_string(),
                "voice_library_profile".to_string(),
                "voice_library_reference".to_string(),
                "voice_template_reference".to_string()
            ]
        );
    }
}

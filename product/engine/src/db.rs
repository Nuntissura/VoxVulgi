use crate::paths::AppPaths;
use crate::Result;
use rusqlite::{Connection, OpenFlags};
use std::time::Duration;

const CURRENT_SCHEMA_VERSION: u32 = 50;
// WP-0258: raised from 750ms to 4000ms so read-only UI queries wait out a WAL
// checkpoint instead of erroring "database is locked". Evidence: 47 subscription
// refreshes failed with "database is locked" under DB contention.
const READ_ONLY_BUSY_TIMEOUT_MS: u64 = 4000;

struct MigrationStep {
    version: u32,
    apply: fn(&Connection) -> Result<()>,
}

const MIGRATION_STEPS: &[MigrationStep] = &[
    MigrationStep {
        version: 1,
        apply: apply_base_schema_v1,
    },
    MigrationStep {
        version: 10,
        apply: apply_schema_v10,
    },
    MigrationStep {
        version: 11,
        apply: apply_schema_v11,
    },
    MigrationStep {
        version: 12,
        apply: apply_schema_v12,
    },
    MigrationStep {
        version: 13,
        apply: apply_schema_v13,
    },
    MigrationStep {
        version: 14,
        apply: apply_schema_v14,
    },
    MigrationStep {
        version: 15,
        apply: apply_schema_v15,
    },
    MigrationStep {
        version: 16,
        apply: apply_schema_v16,
    },
    MigrationStep {
        version: 17,
        apply: apply_schema_v17,
    },
    MigrationStep {
        version: 18,
        apply: apply_schema_v18,
    },
    MigrationStep {
        version: 19,
        apply: apply_schema_v19,
    },
    MigrationStep {
        version: 20,
        apply: apply_schema_v20,
    },
    MigrationStep {
        version: 21,
        apply: apply_schema_v21,
    },
    MigrationStep {
        version: 22,
        apply: apply_schema_v22,
    },
    MigrationStep {
        version: 23,
        apply: apply_schema_v23,
    },
    MigrationStep {
        version: 24,
        apply: apply_schema_v24,
    },
    MigrationStep {
        version: 25,
        apply: apply_schema_v25,
    },
    MigrationStep {
        version: 26,
        apply: apply_schema_v26,
    },
    MigrationStep {
        version: 27,
        apply: apply_schema_v27,
    },
    MigrationStep {
        version: 28,
        apply: apply_schema_v28,
    },
    MigrationStep {
        version: 29,
        apply: apply_schema_v29,
    },
    MigrationStep {
        version: 30,
        apply: apply_schema_v30,
    },
    MigrationStep {
        version: 31,
        apply: apply_schema_v31,
    },
    MigrationStep {
        version: 32,
        apply: apply_schema_v32,
    },
    MigrationStep {
        version: 33,
        apply: apply_schema_v33,
    },
    MigrationStep {
        version: 34,
        apply: apply_schema_v34,
    },
    MigrationStep {
        version: 35,
        apply: apply_schema_v35,
    },
    MigrationStep {
        version: 36,
        apply: apply_schema_v36,
    },
    MigrationStep {
        version: 37,
        apply: apply_schema_v37,
    },
    MigrationStep {
        version: 38,
        apply: apply_schema_v38,
    },
    MigrationStep {
        version: 39,
        apply: apply_schema_v39,
    },
    MigrationStep {
        version: 40,
        apply: apply_schema_v40,
    },
    MigrationStep {
        version: 41,
        apply: apply_schema_v41,
    },
    MigrationStep {
        version: 42,
        apply: apply_schema_v42,
    },
    MigrationStep {
        version: 43,
        apply: apply_schema_v43,
    },
    MigrationStep {
        version: 44,
        apply: apply_schema_v44,
    },
    MigrationStep {
        version: 45,
        apply: apply_schema_v45,
    },
    MigrationStep {
        version: 46,
        apply: apply_schema_v46,
    },
    MigrationStep {
        version: 47,
        apply: apply_schema_v47,
    },
    MigrationStep {
        version: 48,
        apply: apply_schema_v48,
    },
    MigrationStep {
        version: 49,
        apply: apply_schema_v49,
    },
    MigrationStep {
        version: CURRENT_SCHEMA_VERSION,
        apply: apply_schema_v50,
    },
];

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
    // WP-0223: with WAL journal mode, synchronous=NORMAL is the recommended
    // setting per https://www.sqlite.org/pragma.html#pragma_synchronous —
    // still crash-safe but skips per-transaction fsync. Eliminates the
    // checkpoint-stall pattern where job-runner UPDATEs forced read queries
    // (subscription list, library list) to wait seconds under load.
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    Ok(conn)
}

// WP-0224: read-only connection used by UI list commands so they bypass
// the job-runner write queue. The DB schema must already be migrated by an
// earlier `open() + migrate()` (the app does this in startup). Read-only
// callers must NOT call `db::migrate(&conn)` — the connection cannot write,
// and the schema is already up to date when the app reaches the UI.
pub fn open_readonly(paths: &AppPaths) -> Result<Connection> {
    let db_path = paths.db_dir().join("app.sqlite");
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )?;

    conn.busy_timeout(Duration::from_millis(READ_ONLY_BUSY_TIMEOUT_MS))?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    let mut current_version = schema_user_version(conn)?;
    for step in MIGRATION_STEPS {
        if current_version >= step.version {
            continue;
        }
        let tx = conn.unchecked_transaction()?;
        (step.apply)(&tx)?;
        tx.pragma_update(None, "user_version", step.version)?;
        upsert_schema_version_meta(&tx, step.version)?;
        tx.commit()?;
        current_version = step.version;
    }
    if current_version == 0 {
        upsert_schema_version_meta(conn, 0)?;
    }
    Ok(())
}

pub fn schema_user_version(conn: &Connection) -> Result<u32> {
    let version = conn.pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0))?;
    Ok(version.max(0) as u32)
}

fn upsert_schema_version_meta(conn: &Connection, version: u32) -> Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [version.to_string()],
    )?;
    Ok(())
}

fn apply_base_schema_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
"#,
    )?;
    Ok(())
}

fn apply_schema_v10(conn: &Connection) -> Result<()> {
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

    Ok(())
}

fn apply_schema_v11(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS localization_workspace_item (
  item_id TEXT PRIMARY KEY,
  selected_at_ms INTEGER NOT NULL,
  selection_source TEXT NOT NULL,
  selection_path TEXT,
  FOREIGN KEY (item_id) REFERENCES library_item(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_localization_workspace_selected
  ON localization_workspace_item(selected_at_ms DESC, item_id);
"#,
    )?;
    Ok(())
}

fn apply_schema_v12(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS video_library (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  root_path TEXT NOT NULL UNIQUE,
  active INTEGER NOT NULL DEFAULT 1,
  kind TEXT NOT NULL DEFAULT 'custom',
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_video_library_active_updated
  ON video_library(active, updated_at_ms DESC, name COLLATE NOCASE);
"#,
    )?;
    ensure_column(conn, "youtube_subscription", "library_id", "TEXT")?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_youtube_subscription_library ON youtube_subscription(library_id)",
        [],
    )?;
    Ok(())
}

fn apply_schema_v13(conn: &Connection) -> Result<()> {
    ensure_column(
        conn,
        "youtube_subscription",
        "browser_cookie_source",
        "TEXT",
    )?;
    Ok(())
}

fn apply_schema_v14(conn: &Connection) -> Result<()> {
    ensure_column(
        conn,
        "instagram_subscription",
        "browser_cookie_source",
        "TEXT",
    )?;
    Ok(())
}

fn apply_schema_v15(conn: &Connection) -> Result<()> {
    ensure_column(conn, "job", "target_title", "TEXT")?;
    ensure_column(conn, "job", "retry_of_job_id", "TEXT")?;
    ensure_column(conn, "job", "retry_replacement_job_id", "TEXT")?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_job_retry_of ON job(retry_of_job_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_job_retry_replacement ON job(retry_replacement_job_id)",
        [],
    )?;
    Ok(())
}

fn apply_schema_v16(conn: &Connection) -> Result<()> {
    // WP-0252 Item 2c/2b: unify the "legacy" 4KVDP import and new downloads into ONE
    // library and add the indexes that make the 122k-row library list fast. Strictly
    // additive: no row deletes, no media_path rewrites, no resets.
    ensure_column(conn, "library_item", "library_id", "TEXT")?;
    ensure_column(conn, "library_item", "origin", "TEXT")?;

    // Keep the legacy/new distinction as filterable DATA instead of a separate entity.
    conn.execute(
        "UPDATE library_item SET origin = CASE WHEN source_type='url_direct' \
         THEN 'voxvulgi_download' ELSE '4kvdp_import' END WHERE origin IS NULL",
        [],
    )?;
    // Older local VoxVulgi downloads (pre-url_direct) under the legacy yt-fetch dir.
    conn.execute(
        "UPDATE library_item SET origin='voxvulgi_download' \
         WHERE origin='4kvdp_import' AND media_path LIKE '%yt fetch%'",
        [],
    )?;
    // Bind existing items to the one default library so the unified list is a single
    // indexed query. Items with NULL library_id still appear in the "all" view.
    conn.execute(
        "UPDATE library_item SET library_id = ( \
             SELECT id FROM video_library WHERE kind='default' LIMIT 1 \
         ) WHERE library_id IS NULL \
           AND EXISTS (SELECT 1 FROM video_library WHERE kind='default')",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_library_item_library_created \
         ON library_item(library_id, created_at_ms DESC)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_library_item_origin ON library_item(origin)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_library_item_source_type ON library_item(source_type)",
        [],
    )?;
    Ok(())
}

fn apply_schema_v17(conn: &Connection) -> Result<()> {
    // WP-0254: per-lane job scheduling. Add a `lane` column so the runner can give
    // single one-off downloads, conservative recurring (playlist/channel/subscription)
    // syncing, and heavy localization independent concurrency budgets instead of one
    // global FIFO pool. Strictly additive: no row deletes, no resets.
    ensure_column(conn, "job", "lane", "TEXT")?;

    // Backfill existing rows from their type. `download_direct_url` defaults to the
    // single lane here; new subscription-child downloads are stamped `recurring` at
    // enqueue time going forward (historical rows are terminal, so the default is
    // harmless). Keep the lane vocabulary in sync with jobs.rs `JobLane`.
    conn.execute(
        "UPDATE job SET lane = CASE \
           WHEN type='youtube_subscription_refresh_v1' THEN 'recurring' \
           WHEN type IN ( \
             'asr_local','translate_local','diarize_local_v1','dub_voice_preserving_v1', \
             'experimental_voice_backend_render_v1','tts_preview_pyttsx3_v1','tts_neural_local_v1', \
             'mix_dub_preview_v1','mux_dub_preview_v1','separate_audio_spleeter', \
             'separate_audio_demucs_v1','clean_vocals_v1','qc_report_v1','export_pack_v1', \
             'install_phase2_packs_v1' \
           ) THEN 'localization' \
           ELSE 'single' END \
         WHERE lane IS NULL",
        [],
    )?;

    // Per-lane queued fetch path: WHERE lane=? AND status=? ORDER BY created_at_ms.
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_job_lane_status_created \
         ON job(lane, status, created_at_ms)",
        [],
    )?;
    Ok(())
}

fn apply_schema_v18(conn: &Connection) -> Result<()> {
    // WP-0255: honest per-subscription progress. The refresh job already enumerates the
    // playlist/channel and computes these counts, but only logs them — persist them so the
    // UI can show "X of Y downloaded · N new found" and a truthful "last checked" completion
    // timestamp (distinct from last_queued_at_ms, which is set at enqueue, before any upstream
    // check). Strictly additive; all columns nullable so existing rows need no backfill.
    ensure_column(
        conn,
        "youtube_subscription",
        "last_checked_at_ms",
        "INTEGER",
    )?;
    ensure_column(conn, "youtube_subscription", "upstream_total", "INTEGER")?;
    ensure_column(conn, "youtube_subscription", "last_new_found", "INTEGER")?;
    ensure_column(
        conn,
        "youtube_subscription",
        "last_refresh_queued",
        "INTEGER",
    )?;
    Ok(())
}

fn apply_schema_v19(conn: &Connection) -> Result<()> {
    // WP-0259: the operator treats old (4K Video Downloader-imported) and new subscriptions
    // identically and wants NO "legacy" wording in the app. Rename the three app-created
    // "Legacy 4KVDP*" subscription groups to neutral names IN PLACE. This is a rename only —
    // group ids and every youtube_subscription_group_member row are preserved (no deletes, no
    // membership loss). Guarded by the UNIQUE name constraint via NOT EXISTS so it can never
    // collide, and idempotent (a no-op once renamed or if the operator never imported 4KVDP).
    // Only exact matches of the three historical auto-created names are touched; user-named
    // groups are never affected.
    let renames = [
        ("Legacy 4KVDP", "Imported"),
        ("Legacy 4KVDP Subscriptions", "Imported subscriptions"),
        ("Legacy 4KVDP Playlists", "Imported playlists"),
    ];
    for (old, new) in renames {
        conn.execute(
            "UPDATE youtube_subscription_group \
             SET name = ?1, updated_at_ms = CAST(strftime('%s','now') AS INTEGER) * 1000 \
             WHERE name = ?2 \
               AND NOT EXISTS (SELECT 1 FROM youtube_subscription_group g2 WHERE g2.name = ?1)",
            rusqlite::params![new, old],
        )?;
    }
    Ok(())
}

fn apply_schema_v20(conn: &Connection) -> Result<()> {
    // WP-0258 (2b): the Jobs list query `SELECT ... FROM job ORDER BY created_at_ms DESC
    // LIMIT ? OFFSET ?` did a full table scan + temp B-tree sort (EXPLAIN QUERY PLAN:
    // `SCAN job` + `USE TEMP B-TREE FOR ORDER BY`) because no existing index leads with
    // created_at_ms. This index turns it into `SCAN job USING INDEX idx_job_created` (no
    // sort), cutting jobs_list cost under a large job history. Additive index only; no data
    // change.
    conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_job_created ON job(created_at_ms);")?;
    Ok(())
}

fn apply_schema_v21(conn: &Connection) -> Result<()> {
    // WP-0264: failure-state telegraphing. Persist the (truncated) raw error text of the last
    // failed subscription refresh so the subscription panel can classify the failure state
    // (sign-in vs rate-limit vs dead-channel vs busy) WITHOUT a per-poll join back to the job
    // that produced it. `record_subscription_refresh_failure` writes it; a successful refresh
    // CLEARS it (sets NULL) so a recovered subscription shows no state. Strictly additive;
    // nullable, so existing rows need no backfill.
    ensure_column(conn, "youtube_subscription", "last_error_message", "TEXT")?;
    Ok(())
}

fn apply_schema_v22(conn: &Connection) -> Result<()> {
    // WP-0258 v2: historical job inspection may resolve a persisted source URL back to a
    // downloaded library title. Schema v21 had no index on either lookup predicate, so each
    // missing-title URL could scan the full library/provenance tables. The Jobs overview no
    // longer performs this hydration at all; these indexes keep explicit search/detail/backfill
    // paths bounded without changing any user data.
    conn.execute_batch(
        r#"
CREATE INDEX IF NOT EXISTS idx_library_item_source_uri_created
  ON library_item(source_uri, created_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_ingest_provenance_source_url
  ON ingest_provenance(source_url);
CREATE INDEX IF NOT EXISTS idx_job_target_title_created
  ON job(target_title, created_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_job_params_source_url_created
  ON job(
    CASE WHEN json_valid(params_json) THEN json_extract(params_json, '$.url') END,
    created_at_ms DESC
  );
"#,
    )?;
    Ok(())
}

fn apply_schema_v23(conn: &Connection) -> Result<()> {
    // WP-0268: direct-download routing must remain attributable after terminal job cleanup.
    // This table is deliberately keyed by the durable library item, not by `job`: successful
    // rows in `job` are routinely removed, while the library item is the canonical user-facing
    // record. `source_job_id` is informational only and intentionally has no foreign key.
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS library_download_lineage (
  item_id TEXT PRIMARY KEY,
  source_job_id TEXT NOT NULL,
  source_batch_id TEXT,
  source_subscription_id TEXT,
  service TEXT NOT NULL,
  origin_kind TEXT NOT NULL,
  work_track TEXT NOT NULL,
  item_created_at_ms INTEGER NOT NULL,
  recorded_at_ms INTEGER NOT NULL,
  FOREIGN KEY (item_id) REFERENCES library_item(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_library_download_lineage_service_origin_item
  ON library_download_lineage(service, origin_kind, item_created_at_ms DESC, item_id);
CREATE INDEX IF NOT EXISTS idx_library_download_lineage_work_track_item
  ON library_download_lineage(work_track, item_created_at_ms DESC, item_id);
CREATE INDEX IF NOT EXISTS idx_library_download_lineage_source_job
  ON library_download_lineage(source_job_id);
CREATE INDEX IF NOT EXISTS idx_library_download_lineage_subscription
  ON library_download_lineage(source_subscription_id);
CREATE INDEX IF NOT EXISTS idx_job_direct_success_item
  ON job(type, status, item_id, created_at_ms);
"#,
    )?;
    Ok(())
}

fn apply_schema_v24(conn: &Connection) -> Result<()> {
    // WP-0269: `lane` was the three-bucket WP-0254 scheduler vocabulary. Preserve it for
    // compatibility, but store the canonical product track separately so provider-specific
    // queues can be scheduled and observed without decoding a rendered UI projection. Do not
    // bulk-update the large existing queue here: bounded Rust backfill plus the runner's legacy
    // fallback keep startup contention-tolerant and retain every durable job row unchanged.
    ensure_column(conn, "job", "track", "TEXT")?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_job_track_status_created \
         ON job(track, status, created_at_ms)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_job_track_status_type_created \
         ON job(track, status, type, created_at_ms)",
        [],
    )?;
    Ok(())
}

fn apply_schema_v25(conn: &Connection) -> Result<()> {
    // WP-0269 validator repair: the bounded legacy fallback predicates *exactly* on
    // `(track IS NULL OR track='')`. Partial indexes with that same predicate let SQLite seek
    // the `(created_at_ms, id)` keyset directly, instead of first filtering all queued rows by
    // `track` and materialising an ORDER BY temp B-tree. Drop the previous broad candidate
    // index defensively: v25 is unshipped, but this keeps a manually-applied migration
    // idempotent as well.
    conn.execute_batch(
        r#"
DROP INDEX IF EXISTS idx_job_legacy_track_keyset;
CREATE INDEX IF NOT EXISTS idx_job_legacy_untyped_keyset
  ON job(status, created_at_ms, id)
  WHERE track IS NULL OR track='';
CREATE INDEX IF NOT EXISTS idx_job_legacy_typed_keyset
  ON job(status, type, created_at_ms, id)
  WHERE track IS NULL OR track='';
"#,
    )?;
    Ok(())
}

fn apply_schema_v26(conn: &Connection) -> Result<()> {
    // WP-0273: one durable source identity across foreground and subscription ingress. Identity,
    // aliases, the canonical library item, active claim, and repair state stay separate so legacy
    // ambiguity is preserved and a missing NAS file is never mistaken for permission to delete.
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS media_source_identity (
  service TEXT NOT NULL,
  media_id TEXT NOT NULL,
  canonical_url TEXT NOT NULL,
  library_item_id TEXT,
  active_job_id TEXT,
  repair_state TEXT NOT NULL DEFAULT 'ready',
  last_failed_url TEXT,
  last_error TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (service, media_id),
  FOREIGN KEY (library_item_id) REFERENCES library_item(id) ON DELETE SET NULL
);
CREATE INDEX IF NOT EXISTS idx_media_source_identity_library_item
  ON media_source_identity(library_item_id);
CREATE INDEX IF NOT EXISTS idx_media_source_identity_active_job
  ON media_source_identity(active_job_id) WHERE active_job_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS media_source_alias (
  service TEXT NOT NULL,
  media_id TEXT NOT NULL,
  source_url TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  PRIMARY KEY (service, media_id, source_url),
  FOREIGN KEY (service, media_id) REFERENCES media_source_identity(service, media_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_media_source_alias_url ON media_source_alias(source_url);

CREATE TABLE IF NOT EXISTS media_source_association (
  id TEXT PRIMARY KEY,
  service TEXT NOT NULL,
  media_id TEXT NOT NULL,
  origin_kind TEXT NOT NULL,
  source_subscription_id TEXT,
  source_job_id TEXT,
  created_at_ms INTEGER NOT NULL,
  FOREIGN KEY (service, media_id) REFERENCES media_source_identity(service, media_id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_media_source_association_identity_origin
  ON media_source_association(service, media_id, origin_kind, COALESCE(source_subscription_id, ''));
"#,
    )?;
    Ok(())
}

fn apply_schema_v27(conn: &Connection) -> Result<()> {
    // WP-0275: imported and current media share canonical identity while source pages remain
    // many-to-many memberships. Third-party evidence is copied into app-managed storage; the
    // source database and imported files remain read-only.
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS media_source_membership (
  service TEXT NOT NULL,
  media_id TEXT NOT NULL,
  source_subscription_id TEXT NOT NULL,
  source_kind TEXT NOT NULL,
  source_url_snapshot TEXT NOT NULL,
  source_title_snapshot TEXT NOT NULL,
  evidence_kind TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (service, media_id, source_subscription_id),
  FOREIGN KEY (service, media_id) REFERENCES media_source_identity(service, media_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_media_source_membership_source
  ON media_source_membership(source_subscription_id, source_kind, media_id);
CREATE INDEX IF NOT EXISTS idx_media_source_membership_identity
  ON media_source_membership(service, media_id, source_kind);

CREATE TABLE IF NOT EXISTS media_import_evidence (
  id TEXT PRIMARY KEY,
  library_item_id TEXT,
  service TEXT NOT NULL,
  media_id TEXT,
  evidence_kind TEXT NOT NULL,
  source_record_key TEXT NOT NULL,
  source_path_snapshot TEXT,
  source_url_snapshot TEXT,
  match_state TEXT NOT NULL,
  details_json TEXT NOT NULL DEFAULT '{}',
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  FOREIGN KEY (library_item_id) REFERENCES library_item(id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_media_import_evidence_dedupe
  ON media_import_evidence(
    evidence_kind,
    source_record_key,
    COALESCE(library_item_id, ''),
    COALESCE(media_id, '')
  );
CREATE INDEX IF NOT EXISTS idx_media_import_evidence_item_state
  ON media_import_evidence(library_item_id, match_state);
CREATE INDEX IF NOT EXISTS idx_media_import_evidence_identity
  ON media_import_evidence(service, media_id, match_state);

CREATE TABLE IF NOT EXISTS media_import_enrichment_checkpoint (
  source_path TEXT PRIMARY KEY,
  source_size INTEGER NOT NULL,
  source_modified_ms INTEGER NOT NULL,
  last_library_item_id TEXT,
  status TEXT NOT NULL,
  scanned_items INTEGER NOT NULL DEFAULT 0,
  exact_items INTEGER NOT NULL DEFAULT 0,
  ambiguous_items INTEGER NOT NULL DEFAULT 0,
  unresolved_items INTEGER NOT NULL DEFAULT 0,
  conflict_items INTEGER NOT NULL DEFAULT 0,
  updated_at_ms INTEGER NOT NULL
);
"#,
    )?;
    Ok(())
}

fn apply_schema_v28(conn: &Connection) -> Result<()> {
    // WP-0277: inventory, hashing, decisions, quarantine, and rollback are separate durable
    // stages. No inventory command can mutate media and no apply command can target an
    // unreviewed group.
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS media_cleanup_run (
  id TEXT PRIMARY KEY,
  roots_json TEXT NOT NULL,
  scan_queue_json TEXT NOT NULL,
  quarantine_root TEXT,
  status TEXT NOT NULL,
  stage TEXT NOT NULL,
  files_scanned INTEGER NOT NULL DEFAULT 0,
  bytes_scanned INTEGER NOT NULL DEFAULT 0,
  duplicate_groups INTEGER NOT NULL DEFAULT 0,
  reclaimable_bytes INTEGER NOT NULL DEFAULT 0,
  last_error TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS media_cleanup_file (
  run_id TEXT NOT NULL,
  path TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  modified_ms INTEGER NOT NULL,
  prefix_sha256 TEXT,
  suffix_sha256 TEXT,
  full_sha256 TEXT,
  library_item_id TEXT,
  media_id TEXT,
  group_id TEXT,
  state TEXT NOT NULL DEFAULT 'inventoried',
  last_error TEXT,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (run_id, path),
  FOREIGN KEY (run_id) REFERENCES media_cleanup_run(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_media_cleanup_file_stage
  ON media_cleanup_file(run_id, size_bytes, prefix_sha256, suffix_sha256, full_sha256);
CREATE INDEX IF NOT EXISTS idx_media_cleanup_file_group
  ON media_cleanup_file(run_id, group_id, path);

CREATE TABLE IF NOT EXISTS media_cleanup_group (
  run_id TEXT NOT NULL,
  group_id TEXT NOT NULL,
  full_sha256 TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  member_count INTEGER NOT NULL,
  keeper_path TEXT NOT NULL,
  keeper_library_item_id TEXT,
  reclaimable_bytes INTEGER NOT NULL,
  decision TEXT NOT NULL DEFAULT 'pending',
  decision_at_ms INTEGER,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (run_id, group_id),
  FOREIGN KEY (run_id) REFERENCES media_cleanup_run(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS media_cleanup_action (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL,
  group_id TEXT NOT NULL,
  source_path TEXT NOT NULL,
  quarantine_path TEXT NOT NULL,
  keeper_path TEXT NOT NULL,
  source_library_item_id TEXT,
  keeper_library_item_id TEXT,
  relinked_media_ids_json TEXT NOT NULL DEFAULT '[]',
  size_bytes INTEGER NOT NULL,
  full_sha256 TEXT NOT NULL,
  status TEXT NOT NULL,
  error TEXT,
  applied_at_ms INTEGER,
  rolled_back_at_ms INTEGER,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  FOREIGN KEY (run_id) REFERENCES media_cleanup_run(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_media_cleanup_action_run_status
  ON media_cleanup_action(run_id, status, created_at_ms);

CREATE TABLE IF NOT EXISTS media_file_digest_cache (
  path TEXT PRIMARY KEY,
  size_bytes INTEGER NOT NULL,
  modified_ms INTEGER NOT NULL,
  prefix_sha256 TEXT,
  suffix_sha256 TEXT,
  full_sha256 TEXT,
  verified_at_ms INTEGER NOT NULL
);
"#,
    )?;
    Ok(())
}

fn apply_schema_v29(conn: &Connection) -> Result<()> {
    // WP-0281: v27 added memberships for new discovery and import paths, but a live database
    // can already contain the equivalent canonical subscription associations. Promote those
    // VoxVulgi-owned records additively so source-priority and library projections have one
    // durable model without reading, moving, or changing any media on the NAS.
    conn.execute_batch(
        r#"
INSERT OR IGNORE INTO media_source_membership (
  service, media_id, source_subscription_id, source_kind, source_url_snapshot,
  source_title_snapshot, evidence_kind, created_at_ms, updated_at_ms
)
SELECT
  association.service,
  association.media_id,
  association.source_subscription_id,
  CASE
    WHEN INSTR(LOWER(subscription.source_url), '/playlist') > 0
      OR INSTR(LOWER(subscription.source_url), 'list=') > 0 THEN 'playlist'
    WHEN RTRIM(LOWER(subscription.source_url), '/') LIKE '%/shorts' THEN 'shorts_page'
    WHEN RTRIM(LOWER(subscription.source_url), '/') LIKE '%/videos' THEN 'videos_page'
    WHEN (LOWER(subscription.source_url) LIKE '%youtube.com/watch%'
      AND INSTR(LOWER(subscription.source_url), 'v=') > 0)
      OR LOWER(subscription.source_url) LIKE '%youtu.be/%' THEN 'direct_video'
    ELSE 'channel_page'
  END,
  subscription.source_url,
  subscription.title,
  'association_backfill_v29',
  association.created_at_ms,
  association.created_at_ms
FROM media_source_association AS association
JOIN youtube_subscription AS subscription
  ON subscription.id = association.source_subscription_id
WHERE association.service = 'youtube'
  AND association.origin_kind = 'subscription'
  AND association.source_subscription_id IS NOT NULL;
"#,
    )?;
    Ok(())
}

fn apply_schema_v30(conn: &Connection) -> Result<()> {
    // WP-0282: keep lifecycle truth separate from the historical Active/pause toggle.
    // SQLite cannot add a constrained column without rebuilding the table, so all mutation
    // paths validate the three values and the additive migration preserves every existing row.
    ensure_column(
        conn,
        "youtube_subscription",
        "source_status",
        "TEXT NOT NULL DEFAULT 'normal'",
    )?;
    ensure_column(
        conn,
        "youtube_subscription",
        "source_status_changed_at_ms",
        "INTEGER",
    )?;
    ensure_column(
        conn,
        "youtube_subscription",
        "source_status_change_source",
        "TEXT",
    )?;
    conn.execute(
        r#"
UPDATE youtube_subscription
SET
  source_status = 'unavailable',
  source_status_changed_at_ms = COALESCE(last_error_at_ms, updated_at_ms),
  source_status_change_source = 'migration_404'
WHERE source_status = 'normal'
  AND (
    INSTR(LOWER(COALESCE(last_error_message, '')), 'http error 404') > 0
    OR INSTR(LOWER(COALESCE(last_error_message, '')), 'http response error 404') > 0
    OR INSTR(LOWER(COALESCE(last_error_message, '')), '404: not found') > 0
    OR INSTR(LOWER(COALESCE(last_error_message, '')), 'status code 404') > 0
    OR INSTR(LOWER(COALESCE(last_error_message, '')), 'status=404') > 0
    OR INSTR(LOWER(COALESCE(last_error_message, '')), 'status: 404') > 0
  )
"#,
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_youtube_subscription_source_status \
         ON youtube_subscription(source_status, active, updated_at_ms)",
        [],
    )?;
    Ok(())
}

fn apply_schema_v31(conn: &Connection) -> Result<()> {
    // WP-0284: operator intent must remain distinct from a filesystem observation. A deleted
    // canonical item stays linked to every source/membership while every generic download path
    // treats it as a tombstone. The exact authorized job id is a one-attempt capability.
    ensure_column(
        conn,
        "library_item",
        "file_status",
        "TEXT NOT NULL DEFAULT 'available'",
    )?;
    ensure_column(conn, "library_item", "file_status_changed_at_ms", "INTEGER")?;
    ensure_column(conn, "library_item", "file_status_change_source", "TEXT")?;
    ensure_column(conn, "library_item", "file_delete_method", "TEXT")?;
    ensure_column(
        conn,
        "library_item",
        "file_redownload_authorized_job_id",
        "TEXT",
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_library_item_file_status_created \
         ON library_item(file_status, created_at_ms DESC)",
        [],
    )?;
    Ok(())
}

fn apply_schema_v32(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS media_availability_observation (
  path TEXT PRIMARY KEY,
  state TEXT NOT NULL CHECK(state IN ('present','missing','unreachable','slow')),
  observed_at_ms INTEGER NOT NULL,
  source TEXT NOT NULL,
  duration_ms INTEGER NOT NULL,
  next_refresh_at_ms INTEGER NOT NULL,
  invalidated_at_ms INTEGER
);
CREATE INDEX IF NOT EXISTS idx_media_availability_refresh
  ON media_availability_observation(next_refresh_at_ms, invalidated_at_ms);

CREATE TABLE IF NOT EXISTS youtube_subscription_archive_member (
  subscription_id TEXT NOT NULL,
  video_id TEXT NOT NULL,
  discovered_at_ms INTEGER NOT NULL,
  PRIMARY KEY(subscription_id, video_id)
);
CREATE TABLE IF NOT EXISTS youtube_subscription_archive_rollup (
  subscription_id TEXT PRIMARY KEY,
  video_count INTEGER NOT NULL,
  rebuilt_at_ms INTEGER NOT NULL,
  source TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS subscription_activity_rollup (
  subscription_id TEXT PRIMARY KEY,
  queued INTEGER NOT NULL DEFAULT 0,
  running INTEGER NOT NULL DEFAULT 0,
  succeeded INTEGER NOT NULL DEFAULT 0,
  failed INTEGER NOT NULL DEFAULT 0,
  current_title TEXT,
  current_progress REAL,
  rebuilt_at_ms INTEGER NOT NULL,
  source TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS derived_projection_state (
  projection TEXT PRIMARY KEY,
  dirty INTEGER NOT NULL DEFAULT 1,
  updated_at_ms INTEGER NOT NULL
);
INSERT OR IGNORE INTO derived_projection_state(projection, dirty, updated_at_ms)
VALUES ('subscription_activity', 1, 0), ('youtube_archive', 1, 0);
CREATE TRIGGER IF NOT EXISTS trg_job_activity_rollup_dirty_insert
AFTER INSERT ON job BEGIN
  UPDATE derived_projection_state SET dirty=1 WHERE projection='subscription_activity';
END;
CREATE TRIGGER IF NOT EXISTS trg_job_activity_rollup_dirty_update
AFTER UPDATE OF status, progress, error, params_json ON job BEGIN
  UPDATE derived_projection_state SET dirty=1 WHERE projection='subscription_activity';
END;
CREATE TRIGGER IF NOT EXISTS trg_job_activity_rollup_dirty_delete
AFTER DELETE ON job BEGIN
  UPDATE derived_projection_state SET dirty=1 WHERE projection='subscription_activity';
END;
"#,
    )?;
    Ok(())
}

fn apply_schema_v33(conn: &Connection) -> Result<()> {
    conn.execute_batch(r#"
DROP TRIGGER IF EXISTS trg_job_activity_rollup_dirty_insert;
DROP TRIGGER IF EXISTS trg_job_activity_rollup_dirty_update;
DROP TRIGGER IF EXISTS trg_job_activity_rollup_dirty_delete;
CREATE TRIGGER trg_job_activity_rollup_dirty_insert AFTER INSERT ON job BEGIN UPDATE derived_projection_state SET dirty=1, updated_at_ms=updated_at_ms+1 WHERE projection='subscription_activity'; END;
CREATE TRIGGER trg_job_activity_rollup_dirty_update AFTER UPDATE OF status, progress, error, params_json ON job BEGIN UPDATE derived_projection_state SET dirty=1, updated_at_ms=updated_at_ms+1 WHERE projection='subscription_activity'; END;
CREATE TRIGGER trg_job_activity_rollup_dirty_delete AFTER DELETE ON job BEGIN UPDATE derived_projection_state SET dirty=1, updated_at_ms=updated_at_ms+1 WHERE projection='subscription_activity'; END;
CREATE TRIGGER IF NOT EXISTS trg_archive_rollup_dirty_insert AFTER INSERT ON youtube_subscription_archive_member BEGIN UPDATE derived_projection_state SET dirty=1, updated_at_ms=updated_at_ms+1 WHERE projection='youtube_archive'; END;
CREATE TRIGGER IF NOT EXISTS trg_archive_rollup_dirty_update AFTER UPDATE ON youtube_subscription_archive_member BEGIN UPDATE derived_projection_state SET dirty=1, updated_at_ms=updated_at_ms+1 WHERE projection='youtube_archive'; END;
CREATE TRIGGER IF NOT EXISTS trg_archive_rollup_dirty_delete AFTER DELETE ON youtube_subscription_archive_member BEGIN UPDATE derived_projection_state SET dirty=1, updated_at_ms=updated_at_ms+1 WHERE projection='youtube_archive'; END;
"#)?;
    Ok(())
}

fn apply_schema_v34(conn: &Connection) -> Result<()> {
    // Archive member rows are a derived cache. Its authoritative writer updates the generation
    // explicitly, so row triggers would make a canonical rebuild invalidate its own CAS.
    conn.execute_batch(
        r#"
DROP TRIGGER IF EXISTS trg_archive_rollup_dirty_insert;
DROP TRIGGER IF EXISTS trg_archive_rollup_dirty_update;
DROP TRIGGER IF EXISTS trg_archive_rollup_dirty_delete;
"#,
    )?;
    Ok(())
}

fn apply_schema_v35(conn: &Connection) -> Result<()> {
    // WP-0299: raw provider outcomes are append-only evidence. Current policy and its
    // transition history are separate so adaptation is explainable, replayable, and cannot
    // silently rewrite the operator's saved downloader preset.
    conn.execute_batch(r#"
CREATE TABLE IF NOT EXISTS downloader_outcome (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  operation TEXT NOT NULL,
  target_fingerprint TEXT NOT NULL,
  auth_fingerprint TEXT NOT NULL,
  runtime_epoch TEXT NOT NULL,
  baseline_policy_json TEXT NOT NULL,
  effective_policy_json TEXT NOT NULL,
  occurred_at_ms INTEGER NOT NULL,
  outcome_class TEXT NOT NULL,
  error_signature TEXT,
  incident_id TEXT,
  duration_ms INTEGER
);
CREATE INDEX IF NOT EXISTS idx_downloader_outcome_policy_evidence
  ON downloader_outcome(provider, operation, auth_fingerprint, runtime_epoch, outcome_class, occurred_at_ms, id);
CREATE INDEX IF NOT EXISTS idx_downloader_outcome_retention
  ON downloader_outcome(occurred_at_ms, id);

CREATE TABLE IF NOT EXISTS downloader_policy_state (
  provider TEXT NOT NULL,
  operation TEXT NOT NULL,
  auth_fingerprint TEXT NOT NULL,
  runtime_epoch TEXT NOT NULL,
  mode TEXT NOT NULL,
  corroboration_count INTEGER NOT NULL DEFAULT 0,
  success_streak INTEGER NOT NULL DEFAULT 0,
  entered_at_ms INTEGER NOT NULL,
  last_evidence_at_ms INTEGER,
  next_eligible_probe_at_ms INTEGER,
  version INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY(provider, operation, auth_fingerprint, runtime_epoch)
);

CREATE TABLE IF NOT EXISTS downloader_policy_transition (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  operation TEXT NOT NULL,
  auth_fingerprint TEXT NOT NULL,
  runtime_epoch TEXT NOT NULL,
  before_mode TEXT NOT NULL,
  after_mode TEXT NOT NULL,
  reason TEXT NOT NULL,
  evidence_ids_json TEXT NOT NULL,
  occurred_at_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_downloader_policy_transition_history
  ON downloader_policy_transition(provider, operation, auth_fingerprint, runtime_epoch, occurred_at_ms, id);

CREATE TABLE IF NOT EXISTS downloader_outcome_rollup (
  day_utc TEXT NOT NULL,
  provider TEXT NOT NULL,
  operation TEXT NOT NULL,
  auth_fingerprint TEXT NOT NULL,
  runtime_epoch TEXT NOT NULL,
  policy_mode TEXT NOT NULL,
  outcome_class TEXT NOT NULL,
  event_count INTEGER NOT NULL,
  duration_ms_total INTEGER NOT NULL DEFAULT 0,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY(day_utc, provider, operation, auth_fingerprint, runtime_epoch, policy_mode, outcome_class)
);
"#)?;
    Ok(())
}

fn apply_schema_v36(conn: &Connection) -> Result<()> {
    // WP-0299: a controlled cooldown probe is leased rather than advancing the full cooldown.
    // The primary key makes the reservation atomic; expiry makes abandoned claims recoverable.
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS downloader_canary_lease (
  lease_id TEXT NOT NULL,
  job_id TEXT NOT NULL DEFAULT '',
  provider TEXT NOT NULL,
  operation TEXT NOT NULL,
  auth_fingerprint TEXT NOT NULL,
  runtime_epoch TEXT NOT NULL,
  claimed_at_ms INTEGER NOT NULL,
  expires_at_ms INTEGER NOT NULL,
  PRIMARY KEY(provider, operation, auth_fingerprint, runtime_epoch)
);
CREATE INDEX IF NOT EXISTS idx_downloader_canary_lease_expiry
  ON downloader_canary_lease(expires_at_ms);
CREATE TABLE IF NOT EXISTS downloader_history_reset (
  reset_id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  operation TEXT NOT NULL,
  auth_fingerprint TEXT NOT NULL,
  runtime_epoch TEXT NOT NULL,
  outcome_max_rowid INTEGER NOT NULL,
  transition_max_rowid INTEGER NOT NULL,
  outcomes_deleted INTEGER NOT NULL DEFAULT 0,
  transitions_deleted INTEGER NOT NULL DEFAULT 0,
  rollups_deleted INTEGER NOT NULL DEFAULT 0,
  states_deleted INTEGER NOT NULL DEFAULT 0,
  leases_deleted INTEGER NOT NULL DEFAULT 0,
  UNIQUE(provider,operation,auth_fingerprint,runtime_epoch)
);
CREATE INDEX IF NOT EXISTS idx_library_item_media_path
  ON library_item(media_path COLLATE NOCASE);
DROP INDEX IF EXISTS idx_downloader_outcome_policy_evidence;
CREATE INDEX idx_downloader_outcome_policy_evidence
  ON downloader_outcome(provider, operation, auth_fingerprint, runtime_epoch, outcome_class, occurred_at_ms, id);
CREATE INDEX IF NOT EXISTS idx_downloader_outcome_history
  ON downloader_outcome(provider, operation, auth_fingerprint, runtime_epoch, occurred_at_ms, id);
DROP INDEX IF EXISTS idx_downloader_outcome_retention;
CREATE INDEX idx_downloader_outcome_retention
  ON downloader_outcome(occurred_at_ms, id);
DROP INDEX IF EXISTS idx_downloader_policy_transition_history;
CREATE INDEX idx_downloader_policy_transition_history
  ON downloader_policy_transition(provider, operation, auth_fingerprint, runtime_epoch, occurred_at_ms, id);
"#,
    )?;
    ensure_column(
        conn,
        "downloader_canary_lease",
        "job_id",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "downloader_policy_transition",
        "evidence_snapshot_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    Ok(())
}

fn apply_schema_v37(conn: &Connection) -> Result<()> {
    // WP-0306: immutable localization preview generations are authorized by transactional
    // database lineage. Editable manifests/receipts are never an overwrite authority.
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS localization_preview_publication (
  generation_id TEXT PRIMARY KEY,
  item_id TEXT NOT NULL,
  variant_key TEXT NOT NULL,
  input_fingerprint_sha256 TEXT NOT NULL,
  input_fingerprint_json TEXT NOT NULL,
  artifact_path TEXT NOT NULL UNIQUE,
  artifact_bytes INTEGER NOT NULL,
  artifact_sha256 TEXT NOT NULL,
  staging_path TEXT NOT NULL,
  source_job_id TEXT NOT NULL,
  phase TEXT NOT NULL CHECK(phase IN ('prepared','published','committed')),
  qc_intent_json TEXT,
  export_intent_json TEXT,
  qc_job_id TEXT,
  export_job_id TEXT,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  UNIQUE(item_id, variant_key, input_fingerprint_sha256),
  FOREIGN KEY (item_id) REFERENCES library_item(id) ON DELETE CASCADE,
  FOREIGN KEY (source_job_id) REFERENCES job(id) ON DELETE RESTRICT,
  FOREIGN KEY (qc_job_id) REFERENCES job(id) ON DELETE RESTRICT,
  FOREIGN KEY (export_job_id) REFERENCES job(id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS idx_localization_preview_publication_item_phase
  ON localization_preview_publication(item_id, variant_key, phase, updated_at_ms DESC);
CREATE TRIGGER IF NOT EXISTS trg_localization_preview_publication_immutable_lineage
BEFORE UPDATE ON localization_preview_publication
WHEN NEW.generation_id<>OLD.generation_id
  OR NEW.item_id<>OLD.item_id
  OR NEW.variant_key<>OLD.variant_key
  OR NEW.input_fingerprint_sha256<>OLD.input_fingerprint_sha256
  OR NEW.input_fingerprint_json<>OLD.input_fingerprint_json
  OR NEW.artifact_path<>OLD.artifact_path
  OR NEW.artifact_bytes<>OLD.artifact_bytes
  OR NEW.artifact_sha256<>OLD.artifact_sha256
  OR NEW.staging_path<>OLD.staging_path
  OR NEW.source_job_id<>OLD.source_job_id
  OR COALESCE(NEW.qc_intent_json,'')<>COALESCE(OLD.qc_intent_json,'')
  OR COALESCE(NEW.export_intent_json,'')<>COALESCE(OLD.export_intent_json,'')
  OR NEW.created_at_ms<>OLD.created_at_ms
BEGIN
  SELECT RAISE(ABORT, 'localization publication lineage is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_localization_preview_publication_legal_phase
BEFORE UPDATE OF phase ON localization_preview_publication
WHEN NOT (
  NEW.phase=OLD.phase
  OR (OLD.phase='prepared' AND NEW.phase='published')
  OR (OLD.phase='published' AND NEW.phase='committed')
)
BEGIN
  SELECT RAISE(ABORT, 'illegal localization publication phase transition');
END;

CREATE TABLE IF NOT EXISTS localization_preview_active (
  item_id TEXT NOT NULL,
  variant_key TEXT NOT NULL,
  generation_id TEXT NOT NULL,
  source_job_created_at_ms INTEGER NOT NULL,
  source_job_id TEXT NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY(item_id, variant_key),
  FOREIGN KEY (item_id) REFERENCES library_item(id) ON DELETE CASCADE,
  FOREIGN KEY (generation_id) REFERENCES localization_preview_publication(generation_id) ON DELETE RESTRICT,
  FOREIGN KEY (source_job_id) REFERENCES job(id) ON DELETE RESTRICT
);
"#,
    )?;
    Ok(())
}

fn apply_schema_v38(conn: &Connection) -> Result<()> {
    // WP-0306: the database is the publication authority. Continuation ownership becomes
    // immutable once recorded, and an active pointer must reference one internally consistent
    // publication lineage rather than four independently valid foreign keys.
    conn.execute_batch(
        r#"
DROP TRIGGER IF EXISTS trg_localization_preview_publication_immutable_lineage;
CREATE TRIGGER trg_localization_preview_publication_immutable_lineage
BEFORE UPDATE ON localization_preview_publication
WHEN NEW.generation_id<>OLD.generation_id
  OR NEW.item_id<>OLD.item_id
  OR NEW.variant_key<>OLD.variant_key
  OR NEW.input_fingerprint_sha256<>OLD.input_fingerprint_sha256
  OR NEW.input_fingerprint_json<>OLD.input_fingerprint_json
  OR NEW.artifact_path<>OLD.artifact_path
  OR NEW.artifact_bytes<>OLD.artifact_bytes
  OR NEW.artifact_sha256<>OLD.artifact_sha256
  OR NEW.staging_path<>OLD.staging_path
  OR NEW.source_job_id<>OLD.source_job_id
  OR COALESCE(NEW.qc_intent_json,'')<>COALESCE(OLD.qc_intent_json,'')
  OR COALESCE(NEW.export_intent_json,'')<>COALESCE(OLD.export_intent_json,'')
  OR (
    (COALESCE(NEW.qc_job_id,'')<>COALESCE(OLD.qc_job_id,'')
      OR COALESCE(NEW.export_job_id,'')<>COALESCE(OLD.export_job_id,''))
    AND NOT (OLD.phase='published' AND NEW.phase='committed')
  )
  OR NEW.created_at_ms<>OLD.created_at_ms
BEGIN
  SELECT RAISE(ABORT, 'localization publication lineage is immutable');
END;

CREATE UNIQUE INDEX IF NOT EXISTS idx_localization_preview_publication_active_lineage
  ON localization_preview_publication(generation_id,item_id,variant_key,source_job_id);

CREATE TABLE localization_preview_active_v38 (
  item_id TEXT NOT NULL,
  variant_key TEXT NOT NULL,
  generation_id TEXT NOT NULL,
  source_job_created_at_ms INTEGER NOT NULL,
  source_job_id TEXT NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY(item_id, variant_key),
  FOREIGN KEY (item_id) REFERENCES library_item(id) ON DELETE CASCADE,
  FOREIGN KEY (source_job_id) REFERENCES job(id) ON DELETE RESTRICT,
  FOREIGN KEY (generation_id,item_id,variant_key,source_job_id)
    REFERENCES localization_preview_publication(generation_id,item_id,variant_key,source_job_id)
    ON DELETE RESTRICT
);
INSERT INTO localization_preview_active_v38(
  item_id,variant_key,generation_id,source_job_created_at_ms,source_job_id,updated_at_ms
)
SELECT item_id,variant_key,generation_id,source_job_created_at_ms,source_job_id,updated_at_ms
FROM localization_preview_active;
DROP TABLE localization_preview_active;
ALTER TABLE localization_preview_active_v38 RENAME TO localization_preview_active;
"#,
    )?;
    Ok(())
}

fn apply_schema_v39(conn: &Connection) -> Result<()> {
    // WP-0298: a bounded timeout/saturated probe is a latency observation, not proof that the
    // storage endpoint is unreachable. SQLite cannot alter a CHECK constraint in place, so retain
    // every observation while rebuilding the table transactionally with the distinct `slow` state.
    conn.execute_batch(
        r#"
DROP INDEX IF EXISTS idx_media_availability_refresh;
ALTER TABLE media_availability_observation RENAME TO media_availability_observation_v38;
CREATE TABLE media_availability_observation (
  path TEXT PRIMARY KEY,
  state TEXT NOT NULL CHECK(state IN ('present','missing','unreachable','slow')),
  observed_at_ms INTEGER NOT NULL,
  source TEXT NOT NULL,
  duration_ms INTEGER NOT NULL,
  next_refresh_at_ms INTEGER NOT NULL,
  invalidated_at_ms INTEGER
);
INSERT INTO media_availability_observation(
  path,state,observed_at_ms,source,duration_ms,next_refresh_at_ms,invalidated_at_ms
)
SELECT path,state,observed_at_ms,source,duration_ms,next_refresh_at_ms,invalidated_at_ms
FROM media_availability_observation_v38;
DROP TABLE media_availability_observation_v38;
CREATE INDEX idx_media_availability_refresh
  ON media_availability_observation(next_refresh_at_ms, invalidated_at_ms);
"#,
    )?;
    Ok(())
}

fn apply_schema_v40(conn: &Connection) -> Result<()> {
    // WP-0299: recovery ownership and settings mutation ordering must survive process restarts.
    // Editable filesystem receipts/localStorage values are audit hints, never canonical authority.
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS provider_install_lineage (
  attempt_id TEXT PRIMARY KEY,
  stage_root TEXT NOT NULL,
  phase TEXT NOT NULL CHECK(phase IN (
    'prepared','node_publish_intent','node_published',
    'provider_publish_intent','provider_published','committed'
  )),
  updated_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS youtube_protection_mutation_generation (
  operation TEXT PRIMARY KEY,
  generation INTEGER NOT NULL CHECK(generation > 0),
  updated_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS youtube_retention_continuation (
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
  pending INTEGER NOT NULL CHECK(pending IN (0,1)),
  consecutive_failures INTEGER NOT NULL DEFAULT 0 CHECK(consecutive_failures >= 0),
  updated_at_ms INTEGER NOT NULL
);
INSERT OR IGNORE INTO youtube_retention_continuation(singleton,pending,consecutive_failures,updated_at_ms)
VALUES(1,1,0,0);
"#,
    )?;
    Ok(())
}

fn apply_schema_v41(conn: &Connection) -> Result<()> {
    // WP-0298: SQLite is the authority for interrupted YouTube archive publication. The
    // filesystem journal is only a recovery carrier and cannot mint archive membership.
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS youtube_archive_merge_intent (
  intent_id TEXT PRIMARY KEY,
  subscription_id TEXT NOT NULL,
  target_archive_path TEXT NOT NULL,
  source_archive_sha256 TEXT NOT NULL,
  intended_archive_sha256 TEXT NOT NULL,
  intended_video_ids_json TEXT NOT NULL,
  phase TEXT NOT NULL CHECK(phase IN ('prepared','published','committed')),
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  FOREIGN KEY (subscription_id) REFERENCES youtube_subscription(id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_youtube_archive_merge_intent_pending_subscription
  ON youtube_archive_merge_intent(subscription_id)
  WHERE phase <> 'committed';
CREATE INDEX IF NOT EXISTS idx_youtube_archive_merge_intent_pending
  ON youtube_archive_merge_intent(phase, created_at_ms, intent_id);
CREATE TRIGGER IF NOT EXISTS trg_youtube_archive_merge_intent_immutable
BEFORE UPDATE ON youtube_archive_merge_intent
WHEN NEW.intent_id<>OLD.intent_id
  OR NEW.subscription_id<>OLD.subscription_id
  OR NEW.target_archive_path<>OLD.target_archive_path
  OR NEW.source_archive_sha256<>OLD.source_archive_sha256
  OR NEW.intended_archive_sha256<>OLD.intended_archive_sha256
  OR NEW.intended_video_ids_json<>OLD.intended_video_ids_json
  OR NEW.created_at_ms<>OLD.created_at_ms
BEGIN
  SELECT RAISE(ABORT, 'youtube archive merge intent lineage is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_youtube_archive_merge_intent_legal_phase
BEFORE UPDATE OF phase ON youtube_archive_merge_intent
WHEN NOT (
  NEW.phase=OLD.phase
  OR (OLD.phase='prepared' AND NEW.phase='published')
  OR (OLD.phase='published' AND NEW.phase='committed')
)
BEGIN
  SELECT RAISE(ABORT, 'illegal youtube archive merge intent phase transition');
END;
"#,
    )?;
    Ok(())
}

fn apply_schema_v42(conn: &Connection) -> Result<()> {
    // WP-0298: one damaged archive intent must remain diagnosable without poisoning every
    // unrelated subscription. Keep durable failure evidence separate from immutable lineage;
    // a later successful retry resolves, but does not erase, the historical failure receipt.
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS youtube_archive_merge_intent_failure (
  intent_id TEXT PRIMARY KEY,
  subscription_id TEXT NOT NULL,
  error_message TEXT NOT NULL,
  failure_count INTEGER NOT NULL CHECK(failure_count > 0),
  first_failed_at_ms INTEGER NOT NULL,
  last_failed_at_ms INTEGER NOT NULL,
  resolved_at_ms INTEGER,
  FOREIGN KEY (intent_id) REFERENCES youtube_archive_merge_intent(intent_id) ON DELETE CASCADE,
  FOREIGN KEY (subscription_id) REFERENCES youtube_subscription(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_youtube_archive_merge_intent_failure_active
  ON youtube_archive_merge_intent_failure(resolved_at_ms, last_failed_at_ms, intent_id);
CREATE TRIGGER IF NOT EXISTS trg_youtube_archive_merge_intent_failure_binding
BEFORE INSERT ON youtube_archive_merge_intent_failure
WHEN NOT EXISTS (
  SELECT 1 FROM youtube_archive_merge_intent intent
  WHERE intent.intent_id=NEW.intent_id
    AND intent.subscription_id=NEW.subscription_id
)
BEGIN
  SELECT RAISE(ABORT, 'youtube archive merge failure binding is invalid');
END;
CREATE TRIGGER IF NOT EXISTS trg_youtube_archive_merge_intent_failure_rebind
BEFORE UPDATE OF intent_id,subscription_id ON youtube_archive_merge_intent_failure
WHEN NEW.intent_id<>OLD.intent_id OR NEW.subscription_id<>OLD.subscription_id
BEGIN
  SELECT RAISE(ABORT, 'youtube archive merge failure binding is immutable');
END;
"#,
    )?;
    Ok(())
}

fn apply_schema_v43(conn: &Connection) -> Result<()> {
    // WP-0299: the lineage table describes immutable attempt state, while this singleton
    // row is the cross-process install lease. SQLite therefore rejects a second attempt
    // before it can create staging files or perform network work. A single pre-v43 lineage
    // is adopted during migration; ambiguous legacy multi-lineage state remains unowned and
    // must be reconciled explicitly instead of being guessed at during schema migration.
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS provider_install_owner (
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
  attempt_id TEXT NOT NULL UNIQUE,
  acquired_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  FOREIGN KEY (attempt_id) REFERENCES provider_install_lineage(attempt_id) ON DELETE CASCADE
);
INSERT OR IGNORE INTO provider_install_owner(singleton,attempt_id,acquired_at_ms,updated_at_ms)
SELECT 1,attempt_id,updated_at_ms,updated_at_ms
FROM provider_install_lineage
WHERE (SELECT COUNT(*) FROM provider_install_lineage)=1;
CREATE TRIGGER IF NOT EXISTS trg_provider_install_owner_singleton_immutable
BEFORE UPDATE OF singleton,attempt_id ON provider_install_owner
WHEN NEW.singleton<>OLD.singleton OR NEW.attempt_id<>OLD.attempt_id
BEGIN
  SELECT RAISE(ABORT, 'provider install owner identity is immutable');
END;
"#,
    )?;
    Ok(())
}

fn apply_schema_v44(conn: &Connection) -> Result<()> {
    // WP-0306: canonical absence means no active pointer. Once a pointer exists it may only
    // reference a committed immutable publication; prepared/published lineage must fail closed.
    // Existing invalid rows are deliberately retained for resolver diagnostics rather than
    // silently deleted or reclassified during migration.
    conn.execute_batch(
        r#"
CREATE TRIGGER IF NOT EXISTS trg_localization_preview_active_committed_insert
BEFORE INSERT ON localization_preview_active
WHEN NOT EXISTS (
  SELECT 1 FROM localization_preview_publication publication
  WHERE publication.generation_id=NEW.generation_id
    AND publication.item_id=NEW.item_id
    AND publication.variant_key=NEW.variant_key
    AND publication.source_job_id=NEW.source_job_id
    AND publication.phase='committed'
)
BEGIN
  SELECT RAISE(ABORT, 'localization active pointer requires a committed exact publication');
END;
CREATE TRIGGER IF NOT EXISTS trg_localization_preview_active_committed_update
BEFORE UPDATE ON localization_preview_active
WHEN NOT EXISTS (
  SELECT 1 FROM localization_preview_publication publication
  WHERE publication.generation_id=NEW.generation_id
    AND publication.item_id=NEW.item_id
    AND publication.variant_key=NEW.variant_key
    AND publication.source_job_id=NEW.source_job_id
    AND publication.phase='committed'
)
BEGIN
  SELECT RAISE(ABORT, 'localization active pointer requires a committed exact publication');
END;
"#,
    )?;
    Ok(())
}

fn apply_schema_v45(conn: &Connection) -> Result<()> {
    // WP-0298: committed archive-carrier cleanup pages by intent_id. The older
    // (phase,created_at_ms,intent_id) index cannot satisfy that ordering and forces SQLite to
    // scan/sort the full committed history before applying the page limit.
    conn.execute_batch(
        r#"
CREATE INDEX IF NOT EXISTS idx_youtube_archive_merge_intent_phase_intent_cleanup
  ON youtube_archive_merge_intent(phase, intent_id, subscription_id);
"#,
    )?;
    Ok(())
}

fn apply_schema_v46(conn: &Connection) -> Result<()> {
    // WP-0298: archive carrier cleanup can span many bounded pages and must continue after an
    // application restart. This singleton belongs to the app database, so it identifies the
    // cleanup stream without persisting a machine-specific archive path.
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS youtube_archive_carrier_cleanup_cursor (
  singleton INTEGER PRIMARY KEY CHECK(singleton=1),
  after_intent_id TEXT,
  generation INTEGER NOT NULL DEFAULT 0,
  updated_at_ms INTEGER NOT NULL
);
INSERT OR IGNORE INTO youtube_archive_carrier_cleanup_cursor(
  singleton,after_intent_id,generation,updated_at_ms
) VALUES(1,NULL,0,0);
CREATE TRIGGER IF NOT EXISTS trg_youtube_archive_carrier_cleanup_cursor_identity
BEFORE UPDATE OF singleton ON youtube_archive_carrier_cleanup_cursor
WHEN NEW.singleton<>OLD.singleton
BEGIN
  SELECT RAISE(ABORT, 'youtube archive carrier cleanup cursor identity is immutable');
END;
"#,
    )?;
    Ok(())
}

fn apply_schema_v47(conn: &Connection) -> Result<()> {
    // WP-0299: destructive provider recovery is authorized by durable process and filesystem
    // object identities, not by copyable receipts/markers. Complete pinned-derived tree roots
    // remain authoritative after the transient install lineage is cleared.
    ensure_column(
        conn,
        "provider_install_lineage",
        "ownership_token_digest",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "provider_install_lineage",
        "node_directory_identity",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "provider_install_lineage",
        "provider_directory_identity",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "provider_install_lineage",
        "node_tree_sha256",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "provider_install_lineage",
        "provider_tree_sha256",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "provider_install_owner",
        "owner_pid",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "provider_install_owner",
        "owner_process_identity",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS provider_installed_identity (
  singleton INTEGER PRIMARY KEY CHECK(singleton=1),
  install_generation TEXT NOT NULL,
  node_directory_identity TEXT NOT NULL,
  provider_directory_identity TEXT NOT NULL,
  node_tree_sha256 TEXT NOT NULL,
  provider_tree_sha256 TEXT NOT NULL,
  committed_at_ms INTEGER NOT NULL
);
CREATE TRIGGER IF NOT EXISTS trg_provider_installed_identity_singleton_immutable
BEFORE UPDATE OF singleton ON provider_installed_identity
WHEN NEW.singleton<>OLD.singleton
BEGIN
  SELECT RAISE(ABORT, 'provider installed identity singleton is immutable');
END;
CREATE TRIGGER IF NOT EXISTS trg_provider_installed_identity_requires_committed_lineage_insert
BEFORE INSERT ON provider_installed_identity
WHEN NOT EXISTS (
  SELECT 1 FROM provider_install_lineage lineage
  WHERE lineage.phase='committed'
    AND lineage.node_directory_identity=NEW.node_directory_identity
    AND lineage.provider_directory_identity=NEW.provider_directory_identity
    AND lineage.node_tree_sha256=NEW.node_tree_sha256
    AND lineage.provider_tree_sha256=NEW.provider_tree_sha256
)
BEGIN
  SELECT RAISE(ABORT, 'provider installed identity requires committed exact lineage');
END;
CREATE TRIGGER IF NOT EXISTS trg_provider_installed_identity_requires_committed_lineage_update
BEFORE UPDATE ON provider_installed_identity
WHEN NOT EXISTS (
  SELECT 1 FROM provider_install_lineage lineage
  WHERE lineage.phase='committed'
    AND lineage.node_directory_identity=NEW.node_directory_identity
    AND lineage.provider_directory_identity=NEW.provider_directory_identity
    AND lineage.node_tree_sha256=NEW.node_tree_sha256
    AND lineage.provider_tree_sha256=NEW.provider_tree_sha256
)
BEGIN
  SELECT RAISE(ABORT, 'provider installed identity update requires committed exact lineage');
END;
"#,
    )?;
    Ok(())
}

fn apply_schema_v48(conn: &Connection) -> Result<()> {
    // WP-0299: a committed provider identity is a durable lineage reference, not a free-standing
    // row. Random commit nonces bind the owner, legal phase chain, and installed identity. The
    // committed lineage remains after owner release so upgrades can prove or replace it without
    // manufacturing authority from an editable filesystem receipt.
    ensure_column(
        conn,
        "provider_install_lineage",
        "commit_nonce",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "provider_install_lineage",
        "install_generation",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "provider_install_owner",
        "commit_nonce",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "provider_installed_identity",
        "lineage_attempt_id",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "provider_installed_identity",
        "commit_nonce",
        "TEXT NOT NULL DEFAULT ''",
    )?;

    // Bind rows created by v47 when their exact committed lineage still exists. An identity whose
    // transient v47 lineage was already deleted intentionally remains unbound; the engine must
    // re-authenticate its complete destination trees before adopting it under the lifecycle lock.
    conn.execute_batch(
        r#"
UPDATE provider_install_lineage
SET commit_nonce=lower(hex(randomblob(32)))
WHERE commit_nonce='';
UPDATE provider_install_owner
SET commit_nonce=COALESCE(
  (SELECT lineage.commit_nonce FROM provider_install_lineage lineage
   WHERE lineage.attempt_id=provider_install_owner.attempt_id),
  lower(hex(randomblob(32)))
)
WHERE commit_nonce='';
UPDATE provider_install_lineage
SET install_generation=COALESCE((
  SELECT identity.install_generation FROM provider_installed_identity identity
  WHERE identity.node_directory_identity=provider_install_lineage.node_directory_identity
    AND identity.provider_directory_identity=provider_install_lineage.provider_directory_identity
    AND identity.node_tree_sha256=provider_install_lineage.node_tree_sha256
    AND identity.provider_tree_sha256=provider_install_lineage.provider_tree_sha256
  LIMIT 1
), install_generation)
WHERE install_generation='';
UPDATE provider_installed_identity
SET lineage_attempt_id=COALESCE((
      SELECT lineage.attempt_id FROM provider_install_lineage lineage
      WHERE lineage.phase='committed'
        AND lineage.node_directory_identity=provider_installed_identity.node_directory_identity
        AND lineage.provider_directory_identity=provider_installed_identity.provider_directory_identity
        AND lineage.node_tree_sha256=provider_installed_identity.node_tree_sha256
        AND lineage.provider_tree_sha256=provider_installed_identity.provider_tree_sha256
      ORDER BY lineage.updated_at_ms DESC LIMIT 1
    ), ''),
    commit_nonce=COALESCE((
      SELECT lineage.commit_nonce FROM provider_install_lineage lineage
      WHERE lineage.phase='committed'
        AND lineage.node_directory_identity=provider_installed_identity.node_directory_identity
        AND lineage.provider_directory_identity=provider_installed_identity.provider_directory_identity
        AND lineage.node_tree_sha256=provider_installed_identity.node_tree_sha256
        AND lineage.provider_tree_sha256=provider_installed_identity.provider_tree_sha256
      ORDER BY lineage.updated_at_ms DESC LIMIT 1
    ), '')
WHERE lineage_attempt_id='' OR commit_nonce='';

CREATE TABLE IF NOT EXISTS provider_installed_identity_mutation_guard (
  singleton INTEGER PRIMARY KEY CHECK(singleton=1),
  lineage_attempt_id TEXT NOT NULL,
  commit_nonce TEXT NOT NULL,
  operation TEXT NOT NULL CHECK(operation IN ('invalidate','uninstall')),
  created_at_ms INTEGER NOT NULL
);

DROP TRIGGER IF EXISTS trg_provider_installed_identity_requires_committed_lineage_insert;
DROP TRIGGER IF EXISTS trg_provider_installed_identity_requires_committed_lineage_update;

CREATE TRIGGER IF NOT EXISTS trg_provider_install_lineage_v48_insert_prepared
BEFORE INSERT ON provider_install_lineage
WHEN NEW.phase<>'prepared' OR length(NEW.commit_nonce)<>64
  OR NEW.commit_nonce GLOB '*[^0-9A-Fa-f]*'
BEGIN
  SELECT RAISE(ABORT, 'provider lineage must begin prepared with a commit nonce');
END;

CREATE TRIGGER IF NOT EXISTS trg_provider_install_owner_v48_requires_lineage_nonce
BEFORE INSERT ON provider_install_owner
WHEN length(NEW.commit_nonce)<>64 OR NEW.commit_nonce GLOB '*[^0-9A-Fa-f]*' OR NOT EXISTS (
  SELECT 1 FROM provider_install_lineage lineage
  WHERE lineage.attempt_id=NEW.attempt_id
    AND lineage.phase='prepared'
    AND lineage.commit_nonce=NEW.commit_nonce
)
BEGIN
  SELECT RAISE(ABORT, 'provider owner requires exact prepared lineage nonce');
END;

CREATE TRIGGER IF NOT EXISTS trg_provider_install_lineage_v48_immutable_authority
BEFORE UPDATE ON provider_install_lineage
WHEN NEW.attempt_id<>OLD.attempt_id
  OR NEW.stage_root<>OLD.stage_root
  OR NEW.ownership_token_digest<>OLD.ownership_token_digest
  OR NEW.commit_nonce<>OLD.commit_nonce
  OR NEW.install_generation<>OLD.install_generation
  OR ((OLD.node_directory_identity<>'' OR OLD.provider_directory_identity<>''
       OR OLD.node_tree_sha256<>'' OR OLD.provider_tree_sha256<>'')
      AND (NEW.node_directory_identity<>OLD.node_directory_identity
       OR NEW.provider_directory_identity<>OLD.provider_directory_identity
       OR NEW.node_tree_sha256<>OLD.node_tree_sha256
       OR NEW.provider_tree_sha256<>OLD.provider_tree_sha256))
  OR ((NEW.node_directory_identity<>OLD.node_directory_identity
       OR NEW.provider_directory_identity<>OLD.provider_directory_identity
       OR NEW.node_tree_sha256<>OLD.node_tree_sha256
       OR NEW.provider_tree_sha256<>OLD.provider_tree_sha256)
      AND (OLD.phase<>'prepared'
       OR NEW.node_directory_identity='' OR NEW.provider_directory_identity=''
       OR NEW.node_tree_sha256='' OR NEW.provider_tree_sha256=''))
BEGIN
  SELECT RAISE(ABORT, 'provider lineage authority and sealed identity are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_provider_install_lineage_v48_prepared_must_be_sealed
BEFORE UPDATE OF phase ON provider_install_lineage
WHEN OLD.phase='prepared' AND NEW.phase<>'prepared' AND (
  length(OLD.install_generation)<>64
  OR OLD.install_generation GLOB '*[^0-9A-Fa-f]*'
  OR length(OLD.ownership_token_digest)<>64
  OR OLD.ownership_token_digest GLOB '*[^0-9A-Fa-f]*'
  OR length(OLD.node_tree_sha256)<>64
  OR OLD.node_tree_sha256 GLOB '*[^0-9A-Fa-f]*'
  OR length(OLD.provider_tree_sha256)<>64
  OR OLD.provider_tree_sha256 GLOB '*[^0-9A-Fa-f]*'
  OR NOT (
    (length(OLD.node_directory_identity)=57
      AND substr(OLD.node_directory_identity,1,8)='windows:'
      AND substr(OLD.node_directory_identity,9,16) NOT GLOB '*[^0-9A-Fa-f]*'
      AND substr(OLD.node_directory_identity,25,1)=':'
      AND substr(OLD.node_directory_identity,26,32) NOT GLOB '*[^0-9A-Fa-f]*')
    OR
    (substr(OLD.node_directory_identity,1,5)='unix:'
      AND instr(substr(OLD.node_directory_identity,6),':')>1
      AND substr(substr(OLD.node_directory_identity,6),1,instr(substr(OLD.node_directory_identity,6),':')-1) NOT GLOB '*[^0-9]*'
      AND substr(substr(OLD.node_directory_identity,6),instr(substr(OLD.node_directory_identity,6),':')+1)<>''
      AND substr(substr(OLD.node_directory_identity,6),instr(substr(OLD.node_directory_identity,6),':')+1) NOT GLOB '*[^0-9]*')
  )
  OR NOT (
    (length(OLD.provider_directory_identity)=57
      AND substr(OLD.provider_directory_identity,1,8)='windows:'
      AND substr(OLD.provider_directory_identity,9,16) NOT GLOB '*[^0-9A-Fa-f]*'
      AND substr(OLD.provider_directory_identity,25,1)=':'
      AND substr(OLD.provider_directory_identity,26,32) NOT GLOB '*[^0-9A-Fa-f]*')
    OR
    (substr(OLD.provider_directory_identity,1,5)='unix:'
      AND instr(substr(OLD.provider_directory_identity,6),':')>1
      AND substr(substr(OLD.provider_directory_identity,6),1,instr(substr(OLD.provider_directory_identity,6),':')-1) NOT GLOB '*[^0-9]*'
      AND substr(substr(OLD.provider_directory_identity,6),instr(substr(OLD.provider_directory_identity,6),':')+1)<>''
      AND substr(substr(OLD.provider_directory_identity,6),instr(substr(OLD.provider_directory_identity,6),':')+1) NOT GLOB '*[^0-9]*')
  )
)
BEGIN
  SELECT RAISE(ABORT, 'provider lineage cannot leave prepared before exact sealed authority');
END;

CREATE TRIGGER IF NOT EXISTS trg_provider_install_lineage_v48_legal_phase
BEFORE UPDATE OF phase ON provider_install_lineage
WHEN NOT (
  NEW.phase=OLD.phase
  OR (OLD.phase='prepared' AND NEW.phase='node_publish_intent')
  OR (OLD.phase='node_publish_intent' AND NEW.phase='node_published')
  OR (OLD.phase='node_published' AND NEW.phase='provider_publish_intent')
  OR (OLD.phase='provider_publish_intent' AND NEW.phase='provider_published')
  OR (OLD.phase='provider_published' AND NEW.phase='committed')
)
BEGIN
  SELECT RAISE(ABORT, 'illegal provider lineage phase transition');
END;

CREATE TRIGGER IF NOT EXISTS trg_provider_install_lineage_v48_phase_requires_owner
BEFORE UPDATE OF phase ON provider_install_lineage
WHEN NEW.phase<>OLD.phase AND NOT EXISTS (
  SELECT 1 FROM provider_install_owner owner
  WHERE owner.singleton=1
    AND owner.attempt_id=OLD.attempt_id
    AND owner.commit_nonce=OLD.commit_nonce
)
BEGIN
  SELECT RAISE(ABORT, 'provider lineage transition requires exact owner nonce');
END;

CREATE TRIGGER IF NOT EXISTS trg_provider_installed_identity_v48_requires_lineage_insert
BEFORE INSERT ON provider_installed_identity
WHEN length(NEW.commit_nonce)<>64 OR NEW.commit_nonce GLOB '*[^0-9A-Fa-f]*'
  OR length(NEW.install_generation)<>64 OR NEW.install_generation GLOB '*[^0-9A-Fa-f]*'
  OR length(NEW.node_tree_sha256)<>64 OR NEW.node_tree_sha256 GLOB '*[^0-9A-Fa-f]*'
  OR length(NEW.provider_tree_sha256)<>64 OR NEW.provider_tree_sha256 GLOB '*[^0-9A-Fa-f]*'
  OR NOT (
    (length(NEW.node_directory_identity)=57 AND substr(NEW.node_directory_identity,1,8)='windows:'
      AND substr(NEW.node_directory_identity,9,16) NOT GLOB '*[^0-9A-Fa-f]*'
      AND substr(NEW.node_directory_identity,25,1)=':'
      AND substr(NEW.node_directory_identity,26,32) NOT GLOB '*[^0-9A-Fa-f]*')
    OR (substr(NEW.node_directory_identity,1,5)='unix:'
      AND instr(substr(NEW.node_directory_identity,6),':')>1
      AND substr(substr(NEW.node_directory_identity,6),1,instr(substr(NEW.node_directory_identity,6),':')-1) NOT GLOB '*[^0-9]*'
      AND substr(substr(NEW.node_directory_identity,6),instr(substr(NEW.node_directory_identity,6),':')+1)<>''
      AND substr(substr(NEW.node_directory_identity,6),instr(substr(NEW.node_directory_identity,6),':')+1) NOT GLOB '*[^0-9]*')
  )
  OR NOT (
    (length(NEW.provider_directory_identity)=57 AND substr(NEW.provider_directory_identity,1,8)='windows:'
      AND substr(NEW.provider_directory_identity,9,16) NOT GLOB '*[^0-9A-Fa-f]*'
      AND substr(NEW.provider_directory_identity,25,1)=':'
      AND substr(NEW.provider_directory_identity,26,32) NOT GLOB '*[^0-9A-Fa-f]*')
    OR (substr(NEW.provider_directory_identity,1,5)='unix:'
      AND instr(substr(NEW.provider_directory_identity,6),':')>1
      AND substr(substr(NEW.provider_directory_identity,6),1,instr(substr(NEW.provider_directory_identity,6),':')-1) NOT GLOB '*[^0-9]*'
      AND substr(substr(NEW.provider_directory_identity,6),instr(substr(NEW.provider_directory_identity,6),':')+1)<>''
      AND substr(substr(NEW.provider_directory_identity,6),instr(substr(NEW.provider_directory_identity,6),':')+1) NOT GLOB '*[^0-9]*')
  )
  OR NEW.lineage_attempt_id='' OR NOT EXISTS (
  SELECT 1 FROM provider_install_lineage lineage
  WHERE lineage.attempt_id=NEW.lineage_attempt_id
    AND lineage.phase='committed'
    AND lineage.commit_nonce=NEW.commit_nonce
    AND lineage.install_generation=NEW.install_generation
    AND lineage.node_directory_identity=NEW.node_directory_identity
    AND lineage.provider_directory_identity=NEW.provider_directory_identity
    AND lineage.node_tree_sha256=NEW.node_tree_sha256
    AND lineage.provider_tree_sha256=NEW.provider_tree_sha256
)
BEGIN
  SELECT RAISE(ABORT, 'provider installed identity requires exact committed lineage nonce');
END;

CREATE TRIGGER IF NOT EXISTS trg_provider_installed_identity_v48_requires_lineage_update
BEFORE UPDATE ON provider_installed_identity
WHEN NEW.singleton<>OLD.singleton
  OR length(NEW.commit_nonce)<>64 OR NEW.commit_nonce GLOB '*[^0-9A-Fa-f]*'
  OR length(NEW.install_generation)<>64 OR NEW.install_generation GLOB '*[^0-9A-Fa-f]*'
  OR length(NEW.node_tree_sha256)<>64 OR NEW.node_tree_sha256 GLOB '*[^0-9A-Fa-f]*'
  OR length(NEW.provider_tree_sha256)<>64 OR NEW.provider_tree_sha256 GLOB '*[^0-9A-Fa-f]*'
  OR NOT (
    (length(NEW.node_directory_identity)=57 AND substr(NEW.node_directory_identity,1,8)='windows:'
      AND substr(NEW.node_directory_identity,9,16) NOT GLOB '*[^0-9A-Fa-f]*'
      AND substr(NEW.node_directory_identity,25,1)=':'
      AND substr(NEW.node_directory_identity,26,32) NOT GLOB '*[^0-9A-Fa-f]*')
    OR (substr(NEW.node_directory_identity,1,5)='unix:'
      AND instr(substr(NEW.node_directory_identity,6),':')>1
      AND substr(substr(NEW.node_directory_identity,6),1,instr(substr(NEW.node_directory_identity,6),':')-1) NOT GLOB '*[^0-9]*'
      AND substr(substr(NEW.node_directory_identity,6),instr(substr(NEW.node_directory_identity,6),':')+1)<>''
      AND substr(substr(NEW.node_directory_identity,6),instr(substr(NEW.node_directory_identity,6),':')+1) NOT GLOB '*[^0-9]*')
  )
  OR NOT (
    (length(NEW.provider_directory_identity)=57 AND substr(NEW.provider_directory_identity,1,8)='windows:'
      AND substr(NEW.provider_directory_identity,9,16) NOT GLOB '*[^0-9A-Fa-f]*'
      AND substr(NEW.provider_directory_identity,25,1)=':'
      AND substr(NEW.provider_directory_identity,26,32) NOT GLOB '*[^0-9A-Fa-f]*')
    OR (substr(NEW.provider_directory_identity,1,5)='unix:'
      AND instr(substr(NEW.provider_directory_identity,6),':')>1
      AND substr(substr(NEW.provider_directory_identity,6),1,instr(substr(NEW.provider_directory_identity,6),':')-1) NOT GLOB '*[^0-9]*'
      AND substr(substr(NEW.provider_directory_identity,6),instr(substr(NEW.provider_directory_identity,6),':')+1)<>''
      AND substr(substr(NEW.provider_directory_identity,6),instr(substr(NEW.provider_directory_identity,6),':')+1) NOT GLOB '*[^0-9]*')
  )
  OR NEW.lineage_attempt_id='' OR NOT EXISTS (
    SELECT 1 FROM provider_install_lineage lineage
    WHERE lineage.attempt_id=NEW.lineage_attempt_id
      AND lineage.phase='committed'
      AND lineage.commit_nonce=NEW.commit_nonce
      AND lineage.install_generation=NEW.install_generation
      AND lineage.node_directory_identity=NEW.node_directory_identity
      AND lineage.provider_directory_identity=NEW.provider_directory_identity
      AND lineage.node_tree_sha256=NEW.node_tree_sha256
      AND lineage.provider_tree_sha256=NEW.provider_tree_sha256
  )
BEGIN
  SELECT RAISE(ABORT, 'provider installed identity update requires exact committed lineage nonce');
END;

CREATE TRIGGER IF NOT EXISTS trg_provider_installed_identity_v48_delete_guarded
BEFORE DELETE ON provider_installed_identity
WHEN NOT EXISTS (
  SELECT 1 FROM provider_installed_identity_mutation_guard guard
  WHERE guard.singleton=1
    AND guard.lineage_attempt_id=OLD.lineage_attempt_id
    AND guard.commit_nonce=OLD.commit_nonce
)
BEGIN
  SELECT RAISE(ABORT, 'provider installed identity deletion requires governed invalidation');
END;

CREATE TRIGGER IF NOT EXISTS trg_provider_install_lineage_v48_preserve_installed_reference
BEFORE DELETE ON provider_install_lineage
WHEN EXISTS (
  SELECT 1 FROM provider_installed_identity identity
  WHERE identity.singleton=1
    AND identity.lineage_attempt_id=OLD.attempt_id
    AND identity.commit_nonce=OLD.commit_nonce
)
BEGIN
  SELECT RAISE(ABORT, 'provider installed lineage cannot be deleted while referenced');
END;
"#,
    )?;
    Ok(())
}

fn apply_schema_v49(conn: &Connection) -> Result<()> {
    // WP-0300: provider metadata is canonical by provider identity, independent of a job
    // attempt, library filename, or rendered row. Operator title overrides live in a separate
    // table so ingestion and repair cannot accidentally acquire authority over them.
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS media_provider_metadata (
  service TEXT NOT NULL,
  media_id TEXT NOT NULL,
  raw_title TEXT,
  normalized_title TEXT,
  uploader_id TEXT,
  uploader_name TEXT,
  canonical_url TEXT,
  source_url TEXT,
  published_at_ms INTEGER,
  thumbnail_url TEXT,
  provider_name TEXT NOT NULL,
  provider_version TEXT,
  capability_epoch INTEGER NOT NULL DEFAULT 0,
  quality_class TEXT NOT NULL,
  quality_rank INTEGER NOT NULL,
  source_operation TEXT NOT NULL,
  source_job_id TEXT,
  source_subscription_id TEXT,
  observed_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY(service, media_id),
  CHECK(length(trim(service)) > 0),
  CHECK(length(trim(media_id)) > 0),
  CHECK(length(trim(provider_name)) > 0),
  CHECK(quality_rank >= 0),
  CHECK(observed_at_ms >= 0),
  CHECK(updated_at_ms >= 0)
);
CREATE INDEX IF NOT EXISTS idx_media_provider_metadata_normalized_title
  ON media_provider_metadata(normalized_title COLLATE NOCASE, service, media_id);
CREATE INDEX IF NOT EXISTS idx_media_provider_metadata_uploader
  ON media_provider_metadata(service, uploader_id, published_at_ms DESC, media_id);
CREATE INDEX IF NOT EXISTS idx_media_provider_metadata_source_job
  ON media_provider_metadata(source_job_id) WHERE source_job_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS media_title_override (
  service TEXT NOT NULL,
  media_id TEXT NOT NULL,
  title TEXT NOT NULL,
  attribution TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY(service, media_id),
  CHECK(length(trim(service)) > 0),
  CHECK(length(trim(media_id)) > 0),
  CHECK(length(trim(title)) > 0),
  CHECK(length(trim(attribution)) > 0)
);

CREATE TABLE IF NOT EXISTS media_provider_metadata_observation (
  observation_id TEXT PRIMARY KEY,
  service TEXT NOT NULL,
  media_id TEXT NOT NULL,
  observed_at_ms INTEGER NOT NULL,
  provider_name TEXT NOT NULL,
  provider_version TEXT,
  capability_epoch INTEGER NOT NULL DEFAULT 0,
  quality_class TEXT NOT NULL,
  quality_rank INTEGER NOT NULL,
  source_operation TEXT NOT NULL,
  source_job_id TEXT,
  source_subscription_id TEXT,
  payload_json TEXT NOT NULL,
  accepted INTEGER NOT NULL CHECK(accepted IN (0,1)),
  decision_reason TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_media_provider_metadata_observation_identity
  ON media_provider_metadata_observation(service, media_id, observed_at_ms DESC, observation_id);

CREATE TABLE IF NOT EXISTS media_provider_metadata_repair_checkpoint (
  singleton INTEGER PRIMARY KEY CHECK(singleton=1),
  state TEXT NOT NULL CHECK(state IN ('idle','running','paused','completed','failed')),
  after_job_created_at_ms INTEGER,
  after_job_id TEXT,
  scanned_count INTEGER NOT NULL DEFAULT 0,
  repaired_count INTEGER NOT NULL DEFAULT 0,
  conflict_count INTEGER NOT NULL DEFAULT 0,
  unavailable_count INTEGER NOT NULL DEFAULT 0,
  updated_at_ms INTEGER NOT NULL,
  last_error TEXT
);
INSERT OR IGNORE INTO media_provider_metadata_repair_checkpoint(
  singleton,state,after_job_created_at_ms,after_job_id,updated_at_ms
) VALUES(1,'idle',NULL,NULL,0);

CREATE TABLE IF NOT EXISTS media_provider_metadata_repair_change (
  change_id TEXT PRIMARY KEY,
  job_id TEXT NOT NULL,
  service TEXT NOT NULL,
  media_id TEXT NOT NULL,
  classification TEXT NOT NULL,
  before_title TEXT,
  after_title TEXT NOT NULL,
  title_provenance TEXT NOT NULL,
  changed_at_ms INTEGER NOT NULL,
  UNIQUE(job_id,after_title),
  FOREIGN KEY(job_id) REFERENCES job(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_media_provider_metadata_repair_change_job
  ON media_provider_metadata_repair_change(job_id,changed_at_ms DESC);
"#,
    )?;
    Ok(())
}

fn apply_schema_v50(conn: &Connection) -> Result<()> {
    // WP-0226: `list_jobs_for_item` filters by item_id and orders by newest job.
    // Without this composite shape SQLite scans the full created-at index and rejects
    // unrelated rows, which took 382-389 ms directly on the 320k-row operator DB and
    // exceeded 800 ms through the packaged command under host load.
    // Historical import/repair fixtures can be intentionally partial databases. They
    // still pass through the shared migrator, but have no job registry to index. A
    // canonical app database always creates `job` in v1 before reaching this step.
    let job_table_exists = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='job')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if job_table_exists {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_job_item_created \
             ON job(item_id, created_at_ms DESC);",
        )?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    use rusqlite::{params, OptionalExtension};

    #[test]
    fn readonly_open_does_not_create_uninitialized_app_dirs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let app_root = dir.path().join("uninitialized-app");
        let paths = AppPaths::new(app_root.clone());

        assert!(!app_root.exists());
        assert!(
            open_readonly(&paths).is_err(),
            "read-only open must fail when startup has not created the database"
        );
        assert!(
            !app_root.exists(),
            "read-only callers must not create app directories as a side effect"
        );
    }

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
        assert_eq!(
            schema_user_version(&conn).expect("schema version"),
            CURRENT_SCHEMA_VERSION as u32
        );
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
    fn migrate_v29_backfills_subscription_memberships_idempotently() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");

        let sources = [
            (
                "sub-playlist",
                "https://www.youtube.com/playlist?list=PLplaylist",
                "playlist",
                "video-playlist",
            ),
            (
                "sub-videos",
                "https://www.youtube.com/@creator/videos",
                "videos_page",
                "video-videos",
            ),
            (
                "sub-shorts",
                "https://www.youtube.com/@creator/shorts/",
                "shorts_page",
                "video-shorts",
            ),
            (
                "sub-channel",
                "https://www.youtube.com/@creator",
                "channel_page",
                "video-channel",
            ),
        ];

        for (subscription_id, source_url, _source_kind, media_id) in sources {
            conn.execute(
                "INSERT INTO youtube_subscription \
                 (id, title, source_url, folder_map, created_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, '', 1, 1)",
                params![
                    subscription_id,
                    format!("title-{subscription_id}"),
                    source_url
                ],
            )
            .expect("insert subscription");
            conn.execute(
                "INSERT INTO media_source_identity \
                 (service, media_id, canonical_url, created_at_ms, updated_at_ms) \
                 VALUES ('youtube', ?1, ?2, 1, 1)",
                params![
                    media_id,
                    format!("https://www.youtube.com/watch?v={media_id}")
                ],
            )
            .expect("insert identity");
            conn.execute(
                "INSERT INTO media_source_association \
                 (id, service, media_id, origin_kind, source_subscription_id, created_at_ms) \
                 VALUES (?1, 'youtube', ?2, 'subscription', ?3, 1)",
                params![
                    format!("association-{subscription_id}"),
                    media_id,
                    subscription_id
                ],
            )
            .expect("insert association");
        }

        apply_schema_v29(&conn).expect("first backfill");
        apply_schema_v29(&conn).expect("idempotent backfill");

        let rows = conn
            .prepare(
                "SELECT source_subscription_id, source_kind, evidence_kind \
                 FROM media_source_membership ORDER BY source_subscription_id",
            )
            .expect("prepare memberships")
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .expect("query memberships")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect memberships");

        assert_eq!(
            rows,
            vec![
                (
                    "sub-channel".to_string(),
                    "channel_page".to_string(),
                    "association_backfill_v29".to_string(),
                ),
                (
                    "sub-playlist".to_string(),
                    "playlist".to_string(),
                    "association_backfill_v29".to_string(),
                ),
                (
                    "sub-shorts".to_string(),
                    "shorts_page".to_string(),
                    "association_backfill_v29".to_string(),
                ),
                (
                    "sub-videos".to_string(),
                    "videos_page".to_string(),
                    "association_backfill_v29".to_string(),
                ),
            ],
            "backfill retains every historical source association exactly once"
        );
    }

    #[test]
    fn migrate_v22_indexes_job_title_fallback_lookups() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");

        for index_name in [
            "idx_library_item_source_uri_created",
            "idx_ingest_provenance_source_url",
            "idx_job_target_title_created",
            "idx_job_params_source_url_created",
        ] {
            let found: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type='index' AND name=?1",
                    [index_name],
                    |row| row.get(0),
                )
                .optional()
                .expect("query index");
            assert_eq!(found.as_deref(), Some(index_name));
        }
    }

    #[test]
    fn migrate_v25_legacy_keyset_indexes_cover_all_scheduler_query_shapes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");

        // Keep these predicate and ORDER BY shapes in lockstep with
        // `fetch_queued_jobs_for_track_inner`: the partial-index predicate is deliberately
        // byte-for-byte identical to the production legacy row definition.
        let plans = [
            (
                "untyped first page",
                "EXPLAIN QUERY PLAN SELECT id, type, params_json, lane, created_at_ms FROM job \
                 WHERE status='queued' AND (track IS NULL OR track='') \
                 ORDER BY created_at_ms ASC, id ASC LIMIT 4096",
                "idx_job_legacy_untyped_keyset",
            ),
            (
                "untyped cursor",
                "EXPLAIN QUERY PLAN SELECT id, type, params_json, lane, created_at_ms FROM job \
                 WHERE status='queued' AND (track IS NULL OR track='') \
                   AND (created_at_ms>100 OR (created_at_ms=100 AND id>'cursor')) \
                 ORDER BY created_at_ms ASC, id ASC LIMIT 4096",
                "idx_job_legacy_untyped_keyset",
            ),
            (
                "typed first page",
                "EXPLAIN QUERY PLAN SELECT id, type, params_json, lane, created_at_ms FROM job \
                 WHERE status='queued' AND (track IS NULL OR track='') AND type='youtube_refresh' \
                 ORDER BY created_at_ms ASC, id ASC LIMIT 4096",
                "idx_job_legacy_typed_keyset",
            ),
            (
                "typed cursor",
                "EXPLAIN QUERY PLAN SELECT id, type, params_json, lane, created_at_ms FROM job \
                 WHERE status='queued' AND (track IS NULL OR track='') AND type='youtube_refresh' \
                   AND (created_at_ms>100 OR (created_at_ms=100 AND id>'cursor')) \
                 ORDER BY created_at_ms ASC, id ASC LIMIT 4096",
                "idx_job_legacy_typed_keyset",
            ),
        ];

        for (shape, query, expected_index) in plans {
            let detail = conn
                .prepare(query)
                .expect("prepare explain")
                .query_map([], |row| row.get::<_, String>(3))
                .expect("run explain")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("collect explain rows");
            let plan = detail.join(" | ");
            assert!(
                plan.contains(expected_index),
                "{shape} must use {expected_index}; plan: {plan}"
            );
            assert!(
                !plan.contains("USE TEMP B-TREE"),
                "{shape} must preserve indexed keyset ordering; plan: {plan}"
            );
        }
    }

    #[test]
    fn migrate_creates_video_library_registry_and_subscription_binding() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");

        let video_library_table: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='video_library'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query video_library table");
        assert_eq!(video_library_table.as_deref(), Some("video_library"));

        let mut col_stmt = conn
            .prepare("PRAGMA table_info(youtube_subscription)")
            .expect("table_info");
        let cols = col_stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect");
        assert!(cols.iter().any(|col| col == "library_id"));
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

    #[test]
    fn migrate_creates_localization_workspace_table() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");

        let names: Vec<String> = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='localization_workspace_item'",
            )
            .expect("prepare")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect rows");

        assert_eq!(names, vec!["localization_workspace_item".to_string()]);
    }

    #[test]
    fn migrate_sets_user_version_and_meta_schema_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");

        assert_eq!(
            schema_user_version(&conn).expect("schema version"),
            CURRENT_SCHEMA_VERSION as u32
        );
        let meta: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key='schema_version'",
                [],
                |row| row.get(0),
            )
            .expect("meta schema version");
        assert_eq!(meta, CURRENT_SCHEMA_VERSION.to_string());
    }

    #[test]
    fn migrate_v50_indexes_jobs_for_item_filter_and_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");

        let plan = conn
            .prepare(
                r#"
EXPLAIN QUERY PLAN
SELECT
  id, item_id, batch_id, type, status, progress, error, created_at_ms,
  started_at_ms, finished_at_ms, logs_path, params_json, target_title,
  retry_of_job_id, retry_replacement_job_id, track
FROM job
WHERE item_id=?1
ORDER BY created_at_ms DESC
LIMIT ?2 OFFSET ?3
"#,
            )
            .expect("prepare query plan")
            .query_map(params!["item-1", 100_i64, 0_i64], |row| row.get::<_, String>(3))
            .expect("query plan")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect query plan");

        assert!(
            plan.iter()
                .any(|detail| detail.contains("idx_job_item_created")),
            "jobs-for-item query must use the composite item/created index: {plan:?}"
        );
        assert!(
            plan.iter()
                .all(|detail| !detail.contains("SCAN job USING INDEX idx_job_created")),
            "jobs-for-item query must not scan the full created-at index: {plan:?}"
        );
    }

    #[test]
    fn apply_schema_v50_tolerates_partial_database_without_job_registry() {
        let conn = Connection::open_in_memory().expect("open");
        apply_schema_v50(&conn).expect("partial database migration");
        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_job_item_created'",
                [],
                |row| row.get(0),
            )
            .expect("query index count");
        assert_eq!(index_count, 0);
    }

    #[test]
    fn migrate_v26_creates_canonical_media_identity_surfaces() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");
        let names = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('media_source_identity','media_source_alias','media_source_association') ORDER BY name",
            )
            .expect("prepare")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect");
        assert_eq!(
            names,
            vec![
                "media_source_alias".to_string(),
                "media_source_association".to_string(),
                "media_source_identity".to_string(),
            ]
        );
    }

    #[test]
    fn migrate_v27_creates_import_identity_and_membership_surfaces() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");
        let names = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('media_source_membership','media_import_evidence','media_import_enrichment_checkpoint') ORDER BY name",
            )
            .expect("prepare")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect");
        assert_eq!(
            names,
            vec![
                "media_import_enrichment_checkpoint".to_string(),
                "media_import_evidence".to_string(),
                "media_source_membership".to_string(),
            ]
        );
    }

    #[test]
    fn migrate_v28_creates_recoverable_cleanup_surfaces() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('media_cleanup_run','media_cleanup_file','media_cleanup_group','media_cleanup_action','media_file_digest_cache')",
                [],
                |row| row.get(0),
            )
            .expect("tables");
        assert_eq!(count, 5);
    }

    #[test]
    fn migrate_v30_adds_subscription_status_and_backfills_exact_404_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");

        conn.execute(
            "INSERT INTO youtube_subscription \
             (id, title, source_url, folder_map, last_error_at_ms, last_error_message, \
              created_at_ms, updated_at_ms) \
             VALUES ('sub-404', '404', 'https://www.youtube.com/playlist?list=PL404', '', \
                     40, 'Unable to download API page: HTTP Error 404: Not Found', 1, 40)",
            [],
        )
        .expect("insert 404");
        conn.execute(
            "INSERT INTO youtube_subscription \
             (id, title, source_url, folder_map, last_error_at_ms, last_error_message, \
              created_at_ms, updated_at_ms) \
             VALUES ('sub-network', 'Network', 'https://www.youtube.com/@network/videos', '', \
                     50, 'network connection timed out', 1, 50)",
            [],
        )
        .expect("insert network");

        apply_schema_v30(&conn).expect("v30 idempotent backfill");
        let status_404: (String, Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT source_status, source_status_changed_at_ms, source_status_change_source \
                 FROM youtube_subscription WHERE id='sub-404'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("404 status");
        assert_eq!(status_404.0, "unavailable");
        assert_eq!(status_404.1, Some(40));
        assert_eq!(status_404.2.as_deref(), Some("migration_404"));

        let network_status: String = conn
            .query_row(
                "SELECT source_status FROM youtube_subscription WHERE id='sub-network'",
                [],
                |row| row.get(0),
            )
            .expect("network status");
        assert_eq!(network_status, "normal");
    }

    #[test]
    fn projection_generation_contract_uses_job_triggers_but_not_archive_cache_triggers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");
        let before: i64 = conn.query_row("SELECT updated_at_ms FROM derived_projection_state WHERE projection='youtube_archive'", [], |r| r.get(0)).unwrap();
        conn.execute("INSERT INTO youtube_subscription_archive_member(subscription_id,video_id,discovered_at_ms) VALUES('s','v',1)", []).unwrap();
        let after: (i64, i64) = conn.query_row("SELECT dirty,updated_at_ms FROM derived_projection_state WHERE projection='youtube_archive'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(
            after.1, before,
            "derived archive cache writes must not self-invalidate generation CAS"
        );
        let archive_triggers: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name LIKE 'trg_archive_rollup_dirty_%'", [], |r| r.get(0)).unwrap();
        assert_eq!(archive_triggers, 0);
        let activity_triggers: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name LIKE 'trg_job_activity_rollup_dirty_%'", [], |r| r.get(0)).unwrap();
        assert_eq!(activity_triggers, 3);
    }

    #[test]
    fn v36_idempotently_repairs_pre_owner_and_pre_snapshot_candidate_shapes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate baseline");
        conn.execute_batch(
            "DROP TABLE downloader_canary_lease;
             CREATE TABLE downloader_canary_lease (
               lease_id TEXT NOT NULL,
               provider TEXT NOT NULL,
               operation TEXT NOT NULL,
               auth_fingerprint TEXT NOT NULL,
               runtime_epoch TEXT NOT NULL,
               claimed_at_ms INTEGER NOT NULL,
               expires_at_ms INTEGER NOT NULL,
               PRIMARY KEY(provider, operation, auth_fingerprint, runtime_epoch)
             );
             ALTER TABLE downloader_policy_transition DROP COLUMN evidence_snapshot_json;",
        )
        .expect("simulate earlier v36 candidate");
        apply_schema_v36(&conn).expect("repair candidate shape");
        let has_column = |table: &str, column: &str| -> bool {
            let mut statement = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .expect("table info");
            let names = statement
                .query_map([], |row| row.get::<_, String>(1))
                .expect("columns")
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect("collect columns");
            names.iter().any(|name| name == column)
        };
        assert!(has_column("downloader_canary_lease", "job_id"));
        assert!(has_column(
            "downloader_policy_transition",
            "evidence_snapshot_json"
        ));
        let job_default: String = conn
            .query_row(
                "SELECT dflt_value FROM pragma_table_info('downloader_canary_lease') WHERE name='job_id'",
                [],
                |row| row.get(0),
            )
            .expect("job owner default");
        assert_eq!(job_default, "''");
    }

    #[test]
    fn v38_enforces_immutable_continuations_and_composite_active_lineage() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");
        for (item_id, job_id) in [("item-a", "mux-a"), ("item-b", "mux-b")] {
            conn.execute(
                "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES(?1,1,'local','source','title','media.mkv')",
                [item_id],
            )
            .expect("insert item");
            conn.execute(
                "INSERT INTO job(id,item_id,type,status,progress,params_json,created_at_ms,logs_path) VALUES(?1,?2,'mux_dub_preview_v1','succeeded',1.0,'{}',1,'job.log')",
                params![job_id, item_id],
            )
            .expect("insert mux job");
        }
        for job_id in ["qc-good", "qc-other"] {
            conn.execute(
                "INSERT INTO job(id,item_id,type,status,progress,params_json,created_at_ms,logs_path) VALUES(?1,'item-a','qc_dub_preview_v1','queued',0.0,'{}',2,'job.log')",
                [job_id],
            )
            .expect("insert qc job");
        }
        conn.execute(
            "INSERT INTO localization_preview_publication(generation_id,item_id,variant_key,input_fingerprint_sha256,input_fingerprint_json,artifact_path,artifact_bytes,artifact_sha256,staging_path,source_job_id,phase,qc_intent_json,created_at_ms,updated_at_ms) VALUES('gen-a','item-a','','fingerprint','{}','generation-a.mkv',10,'artifact-hash','staging-a.mkv','mux-a','published','{}',1,1)",
            [],
        )
        .expect("insert publication");

        conn.execute(
            "UPDATE localization_preview_publication SET phase='committed',qc_job_id='qc-good',updated_at_ms=2 WHERE generation_id='gen-a'",
            [],
        )
        .expect("published commit may record the deterministic continuation owner");
        assert!(conn
            .execute(
                "UPDATE localization_preview_publication SET qc_job_id='qc-other' WHERE generation_id='gen-a'",
                [],
            )
            .is_err(), "committed continuation ownership must be immutable");

        conn.execute(
            "INSERT INTO localization_preview_active(item_id,variant_key,generation_id,source_job_created_at_ms,source_job_id,updated_at_ms) VALUES('item-a','','gen-a',1,'mux-a',2)",
            [],
        )
        .expect("matching active lineage");
        assert!(conn
            .execute(
                "INSERT INTO localization_preview_active(item_id,variant_key,generation_id,source_job_created_at_ms,source_job_id,updated_at_ms) VALUES('item-b','','gen-a',1,'mux-b',2)",
                [],
            )
            .is_err(), "an active pointer must not cross item/source-job publication lineage");
    }

    #[test]
    fn v44_active_pointer_requires_committed_exact_publication() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = open(&paths).expect("open");
        migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path)
             VALUES('item-v44',1,'local','source','title','media.mkv')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO job(id,item_id,type,status,progress,params_json,created_at_ms,logs_path)
             VALUES('mux-v44','item-v44','mux_dub_preview_v1','succeeded',1.0,'{}',1,'job.log')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO localization_preview_publication(
               generation_id,item_id,variant_key,input_fingerprint_sha256,input_fingerprint_json,
               artifact_path,artifact_bytes,artifact_sha256,staging_path,source_job_id,phase,
               created_at_ms,updated_at_ms
             ) VALUES('gen-v44-published','item-v44','','fingerprint-a','{}','published.mkv',1,
                      'hash-a','published-stage.mkv','mux-v44','published',1,1)",
            [],
        )
        .unwrap();
        assert!(
            conn.execute(
                "INSERT INTO localization_preview_active(
                   item_id,variant_key,generation_id,source_job_created_at_ms,source_job_id,updated_at_ms
                 ) VALUES('item-v44','','gen-v44-published',1,'mux-v44',1)",
                [],
            )
            .is_err(),
            "published lineage must not become active"
        );

        conn.execute(
            "UPDATE localization_preview_publication SET phase='committed',updated_at_ms=2
             WHERE generation_id='gen-v44-published'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO localization_preview_active(
               item_id,variant_key,generation_id,source_job_created_at_ms,source_job_id,updated_at_ms
             ) VALUES('item-v44','','gen-v44-published',1,'mux-v44',2)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO localization_preview_publication(
               generation_id,item_id,variant_key,input_fingerprint_sha256,input_fingerprint_json,
               artifact_path,artifact_bytes,artifact_sha256,staging_path,source_job_id,phase,
               created_at_ms,updated_at_ms
             ) VALUES('gen-v44-next','item-v44','','fingerprint-b','{}','next.mkv',1,
                      'hash-b','next-stage.mkv','mux-v44','published',3,3)",
            [],
        )
        .unwrap();
        assert!(
            conn.execute(
                "UPDATE localization_preview_active SET generation_id='gen-v44-next',updated_at_ms=3
                 WHERE item_id='item-v44' AND variant_key=''",
                [],
            )
            .is_err(),
            "an existing pointer must not be redirected to published lineage"
        );
    }

    #[test]
    fn v39_preserves_observations_and_accepts_distinct_slow_state() {
        let conn = Connection::open_in_memory().expect("open");
        for step in MIGRATION_STEPS.iter().filter(|step| step.version <= 38) {
            (step.apply)(&conn).expect("build complete v38 fixture");
            conn.pragma_update(None, "user_version", step.version)
                .expect("advance fixture version");
        }
        conn.execute_batch(
            r#"
DROP INDEX IF EXISTS idx_media_availability_refresh;
DROP TABLE media_availability_observation;
CREATE TABLE media_availability_observation (
  path TEXT PRIMARY KEY,
  state TEXT NOT NULL CHECK(state IN ('present','missing','unreachable')),
  observed_at_ms INTEGER NOT NULL,
  source TEXT NOT NULL,
  duration_ms INTEGER NOT NULL,
  next_refresh_at_ms INTEGER NOT NULL,
  invalidated_at_ms INTEGER
);
CREATE INDEX idx_media_availability_refresh
  ON media_availability_observation(next_refresh_at_ms, invalidated_at_ms);
"#,
        )
        .expect("v38 observation shape");
        conn.execute(
            "INSERT INTO media_availability_observation(path,state,observed_at_ms,source,duration_ms,next_refresh_at_ms) VALUES('historical','present',1,'fixture',2,3)",
            [],
        )
        .expect("historical observation");
        conn.pragma_update(None, "user_version", 38)
            .expect("v38 marker");

        migrate(&conn).expect("v39 migration");

        assert_eq!(
            conn.query_row(
                "SELECT state FROM media_availability_observation WHERE path='historical'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("preserved state"),
            "present"
        );
        conn.execute(
            "INSERT INTO media_availability_observation(path,state,observed_at_ms,source,duration_ms,next_refresh_at_ms) VALUES('timed-out','slow',4,'bounded_timeout',1500,5)",
            [],
        )
        .expect("slow observation accepted");
        assert_eq!(
            schema_user_version(&conn).expect("version"),
            CURRENT_SCHEMA_VERSION,
            "the v39 observation invariant must survive every later migration"
        );
    }

    #[test]
    fn v48_provider_authority_rejects_illegal_sql_and_allows_governed_reinstall() {
        let conn = Connection::open_in_memory().expect("open");
        migrate(&conn).expect("migrate");
        let nonce = "a".repeat(64);
        let generation = "b".repeat(64);
        let token_digest = "c".repeat(64);
        let node_root = "d".repeat(64);
        let provider_root = "e".repeat(64);
        let node_id = "windows:0000000000000001:00000000000000000000000000000001";
        let provider_id = "windows:0000000000000002:00000000000000000000000000000002";
        assert!(conn
            .execute(
                "INSERT INTO provider_install_lineage(
                   attempt_id,stage_root,phase,updated_at_ms,ownership_token_digest,commit_nonce,install_generation
                 ) VALUES('forged-committed','stage','committed',1,?2,?1,?3)",
                rusqlite::params![nonce, token_digest, generation],
            )
            .is_err());
        conn.execute(
            "INSERT INTO provider_install_lineage(
               attempt_id,stage_root,phase,updated_at_ms,ownership_token_digest,commit_nonce,install_generation
             ) VALUES('attempt-v48','stage','prepared',1,?2,?1,?3)",
            rusqlite::params![nonce, token_digest, generation],
        )
        .expect("prepared lineage");
        conn.execute(
            "INSERT INTO provider_install_owner(
               singleton,attempt_id,acquired_at_ms,updated_at_ms,owner_pid,owner_process_identity,commit_nonce
             ) VALUES(1,'attempt-v48',1,1,1,'process',?1)",
            [&nonce],
        )
        .expect("exact owner nonce");
        conn.execute(
            "UPDATE provider_install_lineage SET
               node_directory_identity=?1,provider_directory_identity=?2,
               node_tree_sha256=?3,provider_tree_sha256=?4
              WHERE attempt_id='attempt-v48'",
            rusqlite::params![node_id, provider_id, node_root, provider_root],
        )
        .expect("seal prepared lineage");
        assert!(conn
            .execute(
                "UPDATE provider_install_lineage SET phase='provider_published' WHERE attempt_id='attempt-v48'",
                [],
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE provider_install_lineage SET node_tree_sha256='retargeted' WHERE attempt_id='attempt-v48'",
                [],
            )
            .is_err());
        for phase in [
            "node_publish_intent",
            "node_published",
            "provider_publish_intent",
            "provider_published",
            "committed",
        ] {
            conn.execute(
                "UPDATE provider_install_lineage SET phase=?1 WHERE attempt_id='attempt-v48'",
                [phase],
            )
            .expect("legal phase transition");
        }
        let blank_identity_error = conn
            .execute(
                "INSERT INTO provider_installed_identity(
                   singleton,lineage_attempt_id,commit_nonce,install_generation,
                   node_directory_identity,provider_directory_identity,node_tree_sha256,
                   provider_tree_sha256,committed_at_ms
                 ) VALUES(1,'attempt-v48',?1,'',?2,?3,?4,?5,1)",
                rusqlite::params![nonce, node_id, provider_id, node_root, provider_root],
            )
            .expect_err("blank installed generation must fail closed");
        assert!(
            blank_identity_error
                .to_string()
                .contains("provider installed identity requires exact committed lineage nonce"),
            "unexpected trigger: {blank_identity_error}"
        );
        conn.execute(
            "INSERT INTO provider_installed_identity(
               singleton,lineage_attempt_id,commit_nonce,install_generation,
               node_directory_identity,provider_directory_identity,node_tree_sha256,
               provider_tree_sha256,committed_at_ms
             ) VALUES(1,'attempt-v48',?1,?2,?3,?4,?5,?6,1)",
            rusqlite::params![nonce, generation, node_id, provider_id, node_root, provider_root],
        )
        .expect("exact committed identity");
        let blank_update_error = conn
            .execute(
                "UPDATE provider_installed_identity SET commit_nonce='' WHERE singleton=1",
                [],
            )
            .expect_err("blank installed nonce update must fail closed");
        assert!(
            blank_update_error
                .to_string()
                .contains("provider installed identity update requires exact committed lineage nonce"),
            "unexpected trigger: {blank_update_error}"
        );
        assert!(
            conn.execute(
                "UPDATE provider_installed_identity SET node_tree_sha256='retargeted' WHERE singleton=1",
                [],
            )
            .is_err(),
            "installed identity cannot be retargeted away from exact committed lineage"
        );
        assert!(conn
            .execute("DELETE FROM provider_installed_identity WHERE singleton=1", [])
            .is_err());
        assert!(conn
            .execute(
                "DELETE FROM provider_install_lineage WHERE attempt_id='attempt-v48'",
                [],
            )
            .is_err());
        conn.execute(
            "INSERT INTO provider_installed_identity_mutation_guard(
               singleton,lineage_attempt_id,commit_nonce,operation,created_at_ms
             ) VALUES(1,'attempt-v48',?1,'uninstall',2)",
            [&nonce],
        )
        .expect("governed uninstall guard");
        conn.execute("DELETE FROM provider_installed_identity WHERE singleton=1", [])
            .expect("governed identity deletion");
        conn.execute(
            "DELETE FROM provider_install_lineage WHERE attempt_id='attempt-v48'",
            [],
        )
        .expect("unreferenced lineage deletion");
        conn.execute(
            "DELETE FROM provider_installed_identity_mutation_guard WHERE singleton=1",
            [],
        )
        .expect("clear guard");

        let next_nonce = "f".repeat(64);
        conn.execute(
            "INSERT INTO provider_install_lineage(
               attempt_id,stage_root,phase,updated_at_ms,ownership_token_digest,commit_nonce,install_generation
             ) VALUES('attempt-v48-next','stage-next','prepared',3,?2,?1,?3)",
            rusqlite::params![next_nonce, token_digest, generation],
        )
        .expect("reinstall prepared lineage");
    }

    #[test]
    fn v48_blank_or_malformed_sealed_authority_cannot_leave_prepared() {
        let conn = Connection::open_in_memory().expect("open");
        migrate(&conn).expect("migrate");
        let nonce = "1".repeat(64);
        conn.execute(
            "INSERT INTO provider_install_lineage(
               attempt_id,stage_root,phase,updated_at_ms,ownership_token_digest,commit_nonce,install_generation
             ) VALUES('blank-authority','stage','prepared',1,'',?1,'')",
            [&nonce],
        )
        .expect("prepared permits an incomplete row only while it is non-published");
        conn.execute(
            "INSERT INTO provider_install_owner(
               singleton,attempt_id,acquired_at_ms,updated_at_ms,owner_pid,owner_process_identity,commit_nonce
             ) VALUES(1,'blank-authority',1,1,1,'process',?1)",
            [&nonce],
        )
        .expect("owner binds exact nonce");
        assert!(
            conn.execute(
                "UPDATE provider_install_lineage SET phase='node_publish_intent'
                 WHERE attempt_id='blank-authority'",
                [],
            )
            .is_err(),
            "blank generation, token digest, object IDs, and complete roots must fail closed"
        );
        conn.execute(
            "UPDATE provider_install_lineage SET
               node_directory_identity='windows:0000000000000001:00000000000000000000000000000001',
               provider_directory_identity='windows:0000000000000002:00000000000000000000000000000002',
               node_tree_sha256=?1,provider_tree_sha256=?2
             WHERE attempt_id='blank-authority'",
            rusqlite::params!["g".repeat(64), "2".repeat(63)],
        )
        .expect("prepared row may attempt to seal fields");
        assert!(conn
            .execute(
                "UPDATE provider_install_lineage SET phase='node_publish_intent'
                 WHERE attempt_id='blank-authority'",
                [],
            )
            .is_err());
    }
}

use crate::paths::AppPaths;
use crate::Result;
use rusqlite::{Connection, OpenFlags};
use std::time::Duration;

const CURRENT_SCHEMA_VERSION: u32 = 31;
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
        version: CURRENT_SCHEMA_VERSION,
        apply: apply_schema_v31,
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
    paths.ensure_dirs()?;

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
}

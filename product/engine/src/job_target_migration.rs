//! WP-0321 S6: collapses `download_direct_url` job history from one row per attempt to one
//! durable row per video (`job.target_key`) with a bounded (`JOB_ATTEMPT_HISTORY_LIMIT`) archived
//! attempt history in `job_attempt`. Schema DDL and the pre-migration backup hook live in
//! `db.rs`'s `apply_schema_v58` (consistent with every other versioned migration); this module
//! holds the key derivation and the dedupe/repoint/archive logic that both the migration and,
//! eventually, the group-B enqueue path (`jobs.rs`) share.
//!
//! Design: `governance/workflow/work_packets/WP-0321_SIMPLIFICATION_PASS_v1_REFINEMENT.md`
//! topic `s6-design`.

use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

/// `download_direct_url` job-target identity key: `"download_direct_url:<service>:<media_id>"`,
/// 1:1 with `media_source_identity(service, media_id)`. Prefers `canonical_source_url` (the
/// stable provider page/asset identity) over `url` (which may be an expiring signed CDN URL);
/// falls back to `url` when no canonical URL is recorded. Returns `None` when neither string
/// resolves to a canonical media identity (e.g. an unrecognized host or unparseable URL) — such
/// rows keep a `NULL` target_key and are left out of the one-row-per-video collapse.
pub fn download_target_key(url: &str, canonical_source_url: Option<&str>) -> Option<String> {
    let source = canonical_source_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(url);
    let canonical = crate::library::canonical_media_source(source)?;
    Some(format!(
        "download_direct_url:{}:{}",
        canonical.service, canonical.media_id
    ))
}

/// Minimal local mirror of `jobs.rs::DownloadDirectUrlParams` — only the two fields the target
/// key derivation needs. `jobs.rs` is owned by S6 group B for this work packet, so this struct is
/// deliberately independent rather than importing a `pub(crate)` type from it; unknown fields in
/// the real params JSON are ignored by default (no `deny_unknown_fields`).
#[derive(Debug, Clone, serde::Deserialize)]
struct DownloadDirectUrlParamsKeyFields {
    url: String,
    #[serde(default)]
    canonical_source_url: Option<String>,
}

/// Writes `<db_dir>/backups/pre_s6_v58_<yyyymmdd_hhmmss>.sqlite` via `VACUUM INTO` when the `job`
/// table exists and has at least one row. Called by `db::migrate` immediately before it opens the
/// v58 step's transaction (never inside one — `VACUUM INTO` manages its own internal transaction).
/// At startup migration time there are no other writers, so this runs directly on the live
/// migration connection (a normal read-write connection, not the `query_only` read context S2's
/// `create_pre_purge_backup` has to work around).
///
/// No-ops (returns `Ok(())`) when: the `job` table does not exist yet (fresh install), it has zero
/// rows (nothing to protect), or the connection has no backing file (in-memory test connections).
pub fn backup_before_v58_if_needed(conn: &Connection) -> Result<()> {
    let job_table_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='job')",
        [],
        |row| row.get(0),
    )?;
    if !job_table_exists {
        return Ok(());
    }
    let job_count: i64 = conn.query_row("SELECT COUNT(*) FROM job", [], |row| row.get(0))?;
    if job_count == 0 {
        return Ok(());
    }
    let Some(db_path) = conn.path() else {
        return Ok(());
    };
    let db_path = std::path::PathBuf::from(db_path);
    let Some(db_dir) = db_path.parent() else {
        return Ok(());
    };
    let backups_dir = db_dir.join("backups");
    std::fs::create_dir_all(&backups_dir)?;
    let stamp: String = conn.query_row("SELECT strftime('%Y%m%d_%H%M%S','now')", [], |row| {
        row.get(0)
    })?;
    let backup_path = backups_dir.join(format!("pre_s6_v58_{stamp}.sqlite"));
    if backup_path.exists() {
        std::fs::remove_file(&backup_path)?;
    }
    let backup_path_str = backup_path.to_string_lossy().to_string();
    conn.execute("VACUUM INTO ?1", params![backup_path_str])?;
    Ok(())
}

struct CandidateRow {
    id: String,
    status: String,
    created_at_ms: i64,
    finished_at_ms: Option<i64>,
}

/// Collapses every `download_direct_url` job row with a `NULL target_key` into one durable row
/// per `(service, media_id)`, archiving superseded attempts (bounded to
/// [`JOB_ATTEMPT_HISTORY_LIMIT`]) into `job_attempt`. Runs inside the caller's transaction (the
/// per-step transaction `db::migrate` already opens for the v58 step).
///
/// Idempotent by construction: only rows with `target_key IS NULL` are considered, and every row
/// this function touches ends the call either stamped with a `target_key` (the survivor) or
/// deleted (superseded); a rerun therefore finds no `NULL target_key` `download_direct_url` rows
/// left to process and does nothing.
///
/// `job.target_key`/`job.attempt_no` and the `job_attempt` table must already exist (created by
/// `db.rs::apply_schema_v58` before calling this). The unique partial index on `job.target_key`
/// must be created by the caller *after* this returns, once every duplicate has been resolved.
pub fn collapse_download_direct_url_targets(conn: &Connection) -> Result<()> {
    let candidates: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, params_json FROM job \
             WHERE type = 'download_direct_url' AND target_key IS NULL",
        )?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    if candidates.is_empty() {
        return Ok(());
    }

    // Group parseable candidates by target key; unparseable params/URLs keep a NULL key and are
    // left completely untouched (no status change, no dedupe, no archival).
    let mut groups: HashMap<(String, String, String), Vec<String>> = HashMap::new();
    for (id, params_json) in &candidates {
        let Ok(params) = serde_json::from_str::<DownloadDirectUrlParamsKeyFields>(params_json)
        else {
            continue;
        };
        let Some(canonical) =
            crate::library::canonical_media_source(
                params
                    .canonical_source_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(&params.url),
            )
        else {
            continue;
        };
        let key = format!(
            "download_direct_url:{}:{}",
            canonical.service, canonical.media_id
        );
        groups
            .entry((key, canonical.service, canonical.media_id))
            .or_default()
            .push(id.clone());
    }

    for ((key, service, media_id), job_ids) in groups {
        collapse_one_target(conn, &key, &service, &media_id, &job_ids)?;
    }

    conn.execute(
        "UPDATE derived_projection_state SET dirty = 1, updated_at_ms = updated_at_ms + 1 \
         WHERE projection = 'subscription_activity'",
        [],
    )?;
    Ok(())
}

fn collapse_one_target(
    conn: &Connection,
    key: &str,
    service: &str,
    media_id: &str,
    job_ids: &[String],
) -> Result<()> {
    let placeholders = vec!["?"; job_ids.len()].join(",");
    let rows: Vec<CandidateRow> = {
        let sql = format!(
            "SELECT id, status, created_at_ms, finished_at_ms FROM job \
             WHERE id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mapped = stmt
            .query_map(rusqlite::params_from_iter(job_ids.iter()), |row| {
                Ok(CandidateRow {
                    id: row.get(0)?,
                    status: row.get(1)?,
                    created_at_ms: row.get(2)?,
                    finished_at_ms: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        mapped
    };
    if rows.is_empty() {
        return Ok(());
    }
    if rows.len() == 1 {
        // No duplicates for this video: just stamp the key, attempt_no stays at its default 1.
        conn.execute(
            "UPDATE job SET target_key = ?1 WHERE id = ?2",
            params![key, rows[0].id],
        )?;
        return Ok(());
    }

    let active_job_id: Option<String> = conn
        .query_row(
            "SELECT active_job_id FROM media_source_identity WHERE service = ?1 AND media_id = ?2",
            params![service, media_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();

    let survivor_id = pick_survivor(&rows, active_job_id.as_deref()).to_string();

    // Extra queued/running rows that are not the survivor are canceled before being treated as
    // superseded, so they read as intentionally superseded rather than as abandoned work.
    for row in &rows {
        if row.id == survivor_id {
            continue;
        }
        if row.status == "queued" || row.status == "running" {
            conn.execute(
                "UPDATE job SET status = 'canceled', error = ?1 WHERE id = ?2",
                params![
                    format!("superseded by {survivor_id} (S6 migration)"),
                    row.id
                ],
            )?;
        }
    }

    let mut superseded: Vec<&CandidateRow> =
        rows.iter().filter(|row| row.id != survivor_id).collect();
    // Newest-first so the top JOB_ATTEMPT_HISTORY_LIMIT are the most recent superseded attempts;
    // archived attempt_no is then assigned ascending by time (oldest archived = 1).
    superseded.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
    let archived_count = superseded.len().min(crate::db::JOB_ATTEMPT_HISTORY_LIMIT as usize);
    let mut to_archive: Vec<&CandidateRow> = superseded[..archived_count].to_vec();
    to_archive.sort_by(|a, b| a.created_at_ms.cmp(&b.created_at_ms));

    for (index, row) in to_archive.iter().enumerate() {
        let attempt_no = (index as i64) + 1;
        conn.execute(
            "INSERT INTO job_attempt(job_id, attempt_no, legacy_job_id, batch_id, track, status, error, created_at_ms, started_at_ms, finished_at_ms, logs_path) \
             SELECT ?1, ?2, id, batch_id, NULL, status, error, created_at_ms, started_at_ms, finished_at_ms, logs_path FROM job WHERE id = ?3",
            params![survivor_id, attempt_no, row.id],
        )?;
    }

    // Ids referenced by localization evidence tables are excluded from deletion (foreign keys
    // there are ON DELETE RESTRICT, so leaving them is also required for the transaction to
    // commit, not just a preservation nicety).
    let localization_referenced: std::collections::HashSet<String> = {
        let mut ids = std::collections::HashSet::new();
        let mut stmt = conn.prepare(
            "SELECT source_job_id FROM localization_preview_publication \
             UNION SELECT qc_job_id FROM localization_preview_publication WHERE qc_job_id IS NOT NULL \
             UNION SELECT export_job_id FROM localization_preview_publication WHERE export_job_id IS NOT NULL \
             UNION SELECT source_job_id FROM localization_preview_active",
        )?;
        let mut query_rows = stmt.query([])?;
        while let Some(row) = query_rows.next()? {
            ids.insert(row.get::<_, String>(0)?);
        }
        ids
    };

    for row in &rows {
        if row.id == survivor_id {
            continue;
        }
        if localization_referenced.contains(&row.id) {
            continue;
        }
        repoint_references(conn, &row.id, &survivor_id)?;
        conn.execute("DELETE FROM job WHERE id = ?1", params![row.id])?;
    }

    // `superseded_count + 1` per design, where `superseded_count` counts every non-survivor row
    // in the group regardless of whether it was archived into `job_attempt`, canceled, deleted,
    // or (localization-referenced) left in place.
    let survivor_attempt_no = rows.len() as i64;
    conn.execute(
        "UPDATE job SET target_key = ?1, attempt_no = ?2 WHERE id = ?3",
        params![key, survivor_attempt_no, survivor_id],
    )?;
    Ok(())
}

/// Survivor precedence: the identity's recorded active job if it is still queued/running, else
/// any running row, else the oldest queued row, else the newest terminal row (by
/// `COALESCE(finished_at_ms, created_at_ms)`).
fn pick_survivor<'a>(rows: &'a [CandidateRow], active_job_id: Option<&str>) -> &'a str {
    if let Some(active_id) = active_job_id {
        if let Some(row) = rows
            .iter()
            .find(|row| row.id == active_id && (row.status == "queued" || row.status == "running"))
        {
            return &row.id;
        }
    }
    if let Some(row) = rows.iter().find(|row| row.status == "running") {
        return &row.id;
    }
    if let Some(row) = rows
        .iter()
        .filter(|row| row.status == "queued")
        .min_by_key(|row| row.created_at_ms)
    {
        return &row.id;
    }
    rows.iter()
        .max_by_key(|row| row.finished_at_ms.unwrap_or(row.created_at_ms))
        .map(|row| row.id.as_str())
        .unwrap_or(rows[0].id.as_str())
}

/// Re-points every non-localization foreign reference to a job id from `superseded_id` to
/// `survivor_id`. `UPDATE OR IGNORE` per the design (defensive against a constraint this call
/// site is not aware of); none of these columns are expected to conflict since each is either
/// unconstrained or already unique per (service, media_id)/item.
fn repoint_references(conn: &Connection, superseded_id: &str, survivor_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE OR IGNORE media_source_identity SET active_job_id = ?1 WHERE active_job_id = ?2",
        params![survivor_id, superseded_id],
    )?;
    conn.execute(
        "UPDATE OR IGNORE provider_subscription_item SET job_id = ?1 WHERE job_id = ?2",
        params![survivor_id, superseded_id],
    )?;
    conn.execute(
        "UPDATE OR IGNORE media_source_association SET source_job_id = ?1 WHERE source_job_id = ?2",
        params![survivor_id, superseded_id],
    )?;
    conn.execute(
        "UPDATE OR IGNORE library_item SET file_redownload_authorized_job_id = ?1 WHERE file_redownload_authorized_job_id = ?2",
        params![survivor_id, superseded_id],
    )?;
    conn.execute(
        "UPDATE OR IGNORE library_download_lineage SET source_job_id = ?1 WHERE source_job_id = ?2",
        params![survivor_id, superseded_id],
    )?;
    conn.execute(
        "UPDATE OR IGNORE media_provider_metadata_repair_change SET job_id = ?1 WHERE job_id = ?2",
        params![survivor_id, superseded_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, MIGRATION_STEPS};
    use crate::paths::AppPaths;

    fn migrate_to(conn: &Connection, target_version: u32) {
        for step in MIGRATION_STEPS
            .iter()
            .take_while(|step| step.version <= target_version)
        {
            let tx = conn.unchecked_transaction().expect("tx");
            (step.apply)(&tx).expect("apply step");
            tx.pragma_update(None, "user_version", step.version)
                .expect("bump user_version");
            tx.commit().expect("commit");
        }
    }

    fn insert_job(
        conn: &Connection,
        id: &str,
        status: &str,
        created_at_ms: i64,
        finished_at_ms: Option<i64>,
        params_json: &str,
    ) {
        conn.execute(
            "INSERT INTO job(id, item_id, batch_id, type, status, progress, error, params_json, created_at_ms, started_at_ms, finished_at_ms, logs_path) \
             VALUES (?1, NULL, 'batch-1', 'download_direct_url', ?2, 0.0, NULL, ?3, ?4, NULL, ?5, ?6)",
            params![
                id,
                status,
                params_json,
                created_at_ms,
                finished_at_ms,
                format!("{id}.jsonl")
            ],
        )
        .expect("insert job");
    }

    fn direct_url_params(video_id: &str) -> String {
        format!(r#"{{"url":"https://www.youtube.com/watch?v={video_id}","provider":"youtube"}}"#)
    }

    #[test]
    fn download_target_key_matches_media_source_identity() {
        assert_eq!(
            download_target_key("https://www.youtube.com/watch?v=abc123DEF45", None),
            Some("download_direct_url:youtube:abc123DEF45".to_string())
        );
        assert_eq!(
            download_target_key(
                "https://rr1---sn-abc.googlevideo.com/videoplayback?expire=1",
                Some("https://www.youtube.com/watch?v=abc123DEF45"),
            ),
            Some("download_direct_url:youtube:abc123DEF45".to_string())
        );
        assert_eq!(
            download_target_key("not a url at all", None),
            None,
            "unparseable input must yield no key"
        );
    }

    #[test]
    fn v58_collapses_duplicates_keeps_active_else_newest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = db::open(&paths).expect("open");
        migrate_to(&conn, 57);

        // Video A: two terminal rows, no identity active_job_id -> survivor is the newest by
        // finished_at_ms.
        insert_job(&conn, "a-old", "failed", 1_000, Some(1_500), &direct_url_params("videoAAAAAA"));
        insert_job(&conn, "a-new", "succeeded", 2_000, Some(2_500), &direct_url_params("videoAAAAAA"));

        // Video B: identity active_job_id points at the queued row -> survivor is that row even
        // though a newer terminal row exists.
        insert_job(&conn, "b-old-failed", "failed", 1_000, Some(1_200), &direct_url_params("videoBBBBBB"));
        insert_job(&conn, "b-queued", "queued", 3_000, None, &direct_url_params("videoBBBBBB"));
        conn.execute(
            "INSERT INTO media_source_identity(service, media_id, canonical_url, active_job_id, created_at_ms, updated_at_ms) \
             VALUES('youtube','videoBBBBBB','https://www.youtube.com/watch?v=videoBBBBBB','b-queued',1,1)",
            [],
        )
        .expect("insert identity");

        let tx = conn.unchecked_transaction().expect("tx");
        crate::db::apply_schema_v58(&tx).expect("apply v58");
        tx.pragma_update(None, "user_version", 58).expect("bump");
        tx.commit().expect("commit");

        let a_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM job WHERE target_key = 'download_direct_url:youtube:videoAAAAAA'",
                [],
                |row| row.get(0),
            )
            .expect("count a");
        assert_eq!(a_count, 1);
        let a_survivor: String = conn
            .query_row(
                "SELECT id FROM job WHERE target_key = 'download_direct_url:youtube:videoAAAAAA'",
                [],
                |row| row.get(0),
            )
            .expect("survivor a");
        assert_eq!(a_survivor, "a-new");

        let b_survivor: (String, String) = conn
            .query_row(
                "SELECT id, status FROM job WHERE target_key = 'download_direct_url:youtube:videoBBBBBB'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("survivor b");
        assert_eq!(b_survivor.0, "b-queued");
        assert_eq!(b_survivor.1, "queued");

        let b_old_canceled: String = conn
            .query_row(
                "SELECT status FROM job_attempt WHERE legacy_job_id = 'b-old-failed'",
                [],
                |row| row.get(0),
            )
            .expect("archived b-old-failed");
        assert_eq!(b_old_canceled, "failed");
    }

    #[test]
    fn v58_repoints_identity_subscription_item_redownload_auth_and_repair_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = db::open(&paths).expect("open");
        migrate_to(&conn, 57);

        insert_job(&conn, "c-old", "failed", 1_000, Some(1_100), &direct_url_params("videoCCCCCC"));
        insert_job(&conn, "c-new", "succeeded", 2_000, Some(2_200), &direct_url_params("videoCCCCCC"));

        conn.execute(
            "INSERT INTO media_source_identity(service, media_id, canonical_url, active_job_id, created_at_ms, updated_at_ms) \
             VALUES('youtube','videoCCCCCC','https://www.youtube.com/watch?v=videoCCCCCC','c-old',1,1)",
            [],
        )
        .expect("insert identity");
        conn.execute(
            "INSERT INTO provider_subscription_item(service, subscription_id, media_id, source_url, job_id, discovered_at_ms, updated_at_ms) \
             VALUES('youtube','sub-1','videoCCCCCC','https://www.youtube.com/watch?v=videoCCCCCC','c-old',1,1)",
            [],
        )
        .expect("insert subscription item");
        conn.execute(
            "INSERT INTO library_item(id, created_at_ms, source_type, source_uri, title, media_path, file_redownload_authorized_job_id) \
             VALUES('item-c',1,'youtube','https://www.youtube.com/watch?v=videoCCCCCC','Title','path.mkv','c-old')",
            [],
        )
        .expect("insert library item");
        conn.execute(
            "INSERT INTO media_provider_metadata_repair_change(change_id, job_id, service, media_id, classification, after_title, title_provenance, changed_at_ms) \
             VALUES('change-1','c-old','youtube','videoCCCCCC','retitled','New Title','provider',1)",
            [],
        )
        .expect("insert repair change");

        let tx = conn.unchecked_transaction().expect("tx");
        crate::db::apply_schema_v58(&tx).expect("apply v58");
        tx.pragma_update(None, "user_version", 58).expect("bump");
        tx.commit().expect("commit");

        let identity_active: String = conn
            .query_row(
                "SELECT active_job_id FROM media_source_identity WHERE service='youtube' AND media_id='videoCCCCCC'",
                [],
                |row| row.get(0),
            )
            .expect("identity active");
        assert_eq!(identity_active, "c-new");

        let subscription_job: String = conn
            .query_row(
                "SELECT job_id FROM provider_subscription_item WHERE service='youtube' AND subscription_id='sub-1' AND media_id='videoCCCCCC'",
                [],
                |row| row.get(0),
            )
            .expect("subscription job");
        assert_eq!(subscription_job, "c-new");

        let redownload_auth: String = conn
            .query_row(
                "SELECT file_redownload_authorized_job_id FROM library_item WHERE id='item-c'",
                [],
                |row| row.get(0),
            )
            .expect("redownload auth");
        assert_eq!(redownload_auth, "c-new");

        let repair_job: String = conn
            .query_row(
                "SELECT job_id FROM media_provider_metadata_repair_change WHERE change_id='change-1'",
                [],
                |row| row.get(0),
            )
            .expect("repair job");
        assert_eq!(repair_job, "c-new");

        let old_row_exists: i64 = conn
            .query_row("SELECT COUNT(*) FROM job WHERE id='c-old'", [], |row| {
                row.get(0)
            })
            .expect("old row count");
        assert_eq!(old_row_exists, 0);
    }

    #[test]
    fn v58_leaves_localization_and_unparseable_rows_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = db::open(&paths).expect("open");
        migrate_to(&conn, 57);

        // Unparseable params_json: left with a NULL key, untouched.
        insert_job(&conn, "d-bad", "failed", 1_000, Some(1_100), "not json");
        // Unparseable/unrecognized URL: left with a NULL key, untouched.
        insert_job(
            &conn,
            "d-unrecognized",
            "failed",
            1_000,
            Some(1_100),
            r#"{"url":"not a real url","provider":"other"}"#,
        );

        // A row referenced by localization evidence: must survive deletion even though it would
        // otherwise be superseded.
        insert_job(&conn, "e-old", "succeeded", 1_000, Some(1_100), &direct_url_params("videoEEEEEE"));
        insert_job(&conn, "e-new", "succeeded", 2_000, Some(2_200), &direct_url_params("videoEEEEEE"));
        conn.execute(
            "INSERT INTO library_item(id, created_at_ms, source_type, source_uri, title, media_path) \
             VALUES('item-e',1,'youtube','https://www.youtube.com/watch?v=videoEEEEEE','Title','path.mkv')",
            [],
        )
        .expect("insert library item");
        conn.execute(
            "INSERT INTO localization_preview_publication(generation_id,item_id,variant_key,input_fingerprint_sha256,input_fingerprint_json,artifact_path,artifact_bytes,artifact_sha256,staging_path,source_job_id,phase,qc_intent_json,created_at_ms,updated_at_ms) \
             VALUES('gen-e','item-e','','fp','{}','artifact-e.mkv',10,'hash-e','staging-e.mkv','e-old','published','{}',1,1)",
            [],
        )
        .expect("insert localization publication");

        let tx = conn.unchecked_transaction().expect("tx");
        crate::db::apply_schema_v58(&tx).expect("apply v58");
        tx.pragma_update(None, "user_version", 58).expect("bump");
        tx.commit().expect("commit");

        let bad_key: Option<String> = conn
            .query_row("SELECT target_key FROM job WHERE id='d-bad'", [], |row| {
                row.get(0)
            })
            .expect("bad row");
        assert_eq!(bad_key, None);
        let unrecognized_key: Option<String> = conn
            .query_row(
                "SELECT target_key FROM job WHERE id='d-unrecognized'",
                [],
                |row| row.get(0),
            )
            .expect("unrecognized row");
        assert_eq!(unrecognized_key, None);

        let e_old_exists: i64 = conn
            .query_row("SELECT COUNT(*) FROM job WHERE id='e-old'", [], |row| {
                row.get(0)
            })
            .expect("e-old count");
        assert_eq!(
            e_old_exists, 1,
            "localization-referenced row must not be deleted"
        );
    }

    #[test]
    fn v58_idempotent_on_rerun() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = db::open(&paths).expect("open");
        migrate_to(&conn, 57);

        insert_job(&conn, "f-old", "failed", 1_000, Some(1_100), &direct_url_params("videoFFFFFF"));
        insert_job(&conn, "f-new", "succeeded", 2_000, Some(2_200), &direct_url_params("videoFFFFFF"));

        let tx = conn.unchecked_transaction().expect("tx");
        crate::db::apply_schema_v58(&tx).expect("apply v58 first run");
        tx.commit().expect("commit first run");

        let after_first: i64 = conn
            .query_row("SELECT COUNT(*) FROM job", [], |row| row.get(0))
            .expect("count after first run");

        let tx2 = conn.unchecked_transaction().expect("tx2");
        crate::db::apply_schema_v58(&tx2).expect("apply v58 second run");
        tx2.commit().expect("commit second run");

        let after_second: i64 = conn
            .query_row("SELECT COUNT(*) FROM job", [], |row| row.get(0))
            .expect("count after second run");
        assert_eq!(after_first, after_second);

        let survivor_attempt_no: i64 = conn
            .query_row(
                "SELECT attempt_no FROM job WHERE id='f-new'",
                [],
                |row| row.get(0),
            )
            .expect("survivor attempt_no");
        assert_eq!(survivor_attempt_no, 2);
    }

    #[test]
    fn v58_caps_archived_attempts_at_five() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        let conn = db::open(&paths).expect("open");
        migrate_to(&conn, 57);

        for index in 0..7 {
            insert_job(
                &conn,
                &format!("g-{index}"),
                "failed",
                1_000 + index as i64 * 100,
                Some(1_050 + index as i64 * 100),
                &direct_url_params("videoGGGGGG"),
            );
        }
        insert_job(&conn, "g-survivor", "succeeded", 5_000, Some(5_100), &direct_url_params("videoGGGGGG"));

        let tx = conn.unchecked_transaction().expect("tx");
        crate::db::apply_schema_v58(&tx).expect("apply v58");
        tx.pragma_update(None, "user_version", 58).expect("bump");
        tx.commit().expect("commit");

        let archived_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM job_attempt WHERE job_id = 'g-survivor'",
                [],
                |row| row.get(0),
            )
            .expect("archived count");
        assert_eq!(archived_count, crate::db::JOB_ATTEMPT_HISTORY_LIMIT as i64);

        let remaining_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM job WHERE target_key = 'download_direct_url:youtube:videoGGGGGG'",
                [],
                |row| row.get(0),
            )
            .expect("remaining rows");
        assert_eq!(remaining_rows, 1, "only the survivor row remains in job");
    }
}

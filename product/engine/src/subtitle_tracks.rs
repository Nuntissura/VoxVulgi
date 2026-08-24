use crate::paths::AppPaths;
use crate::subtitles::{SubtitleDocument, SUBTITLE_JSON_SCHEMA_VERSION};
use crate::{db, EngineError, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

// Version selection and three-file publication are one logical operation, but filesystem I/O
// must not run under AppDatabase writer admission. Serialize this narrow publication surface so
// concurrent editor saves cannot choose the same version while keeping DB permits short.
static SUBTITLE_VERSION_PUBLICATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleTrackRow {
    pub id: String,
    pub item_id: String,
    pub kind: String,
    pub lang: String,
    pub format: String,
    pub path: String,
    pub created_by: String,
    pub version: i64,
}

pub fn list_tracks(paths: &AppPaths, item_id: &str) -> Result<Vec<SubtitleTrackRow>> {
    // WP-0226: read-only connection bypasses job-runner write queue.
    let conn = db::open_readonly(paths)?;

    let mut stmt = conn.prepare(
        r#"
SELECT
  id,
  item_id,
  kind,
  lang,
  format,
  path,
  created_by,
  version
FROM subtitle_track
WHERE item_id=?1
ORDER BY kind ASC, lang ASC, version DESC
"#,
    )?;

    let rows = stmt
        .query_map(params![item_id], |row| {
            Ok(SubtitleTrackRow {
                id: row.get(0)?,
                item_id: row.get(1)?,
                kind: row.get(2)?,
                lang: row.get(3)?,
                format: row.get(4)?,
                path: row.get(5)?,
                created_by: row.get(6)?,
                version: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows)
}

pub fn get_track(paths: &AppPaths, track_id: &str) -> Result<SubtitleTrackRow> {
    // WP-0226: read-only connection bypasses job-runner write queue.
    let conn = db::open_readonly(paths)?;

    conn.query_row(
        r#"
SELECT
  id,
  item_id,
  kind,
  lang,
  format,
  path,
  created_by,
  version
FROM subtitle_track
WHERE id=?1
"#,
        params![track_id],
        |row| {
            Ok(SubtitleTrackRow {
                id: row.get(0)?,
                item_id: row.get(1)?,
                kind: row.get(2)?,
                lang: row.get(3)?,
                format: row.get(4)?,
                path: row.get(5)?,
                created_by: row.get(6)?,
                version: row.get(7)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            EngineError::InstallFailed(format!("subtitle track not found: {track_id}"))
        }
        other => EngineError::Database(other),
    })
}

pub fn load_document(paths: &AppPaths, track_id: &str) -> Result<SubtitleDocument> {
    let track = get_track(paths, track_id)?;
    let doc = load_document_from_path(Path::new(&track.path))?;
    Ok(doc)
}

pub fn load_document_from_path(path: &Path) -> Result<SubtitleDocument> {
    let bytes = std::fs::read(path)?;
    let doc: SubtitleDocument = serde_json::from_slice(&bytes)?;
    if doc.schema_version != SUBTITLE_JSON_SCHEMA_VERSION {
        return Err(EngineError::InstallFailed(format!(
            "unsupported subtitle schema_version: {}",
            doc.schema_version
        )));
    }
    Ok(doc)
}

pub fn save_new_version(
    paths: &AppPaths,
    base_track_id: &str,
    doc: SubtitleDocument,
) -> Result<SubtitleTrackRow> {
    save_new_version_with_writer(paths, base_track_id, doc, |doc, json, srt, vtt| {
        crate::subtitles::write_artifacts(doc, json, srt, vtt)
    })
}

fn save_new_version_with_writer<ArtifactWriter>(
    paths: &AppPaths,
    base_track_id: &str,
    mut doc: SubtitleDocument,
    artifact_writer: ArtifactWriter,
) -> Result<SubtitleTrackRow>
where
    ArtifactWriter: FnOnce(&SubtitleDocument, &Path, &Path, &Path) -> Result<()>,
{
    let base = get_track(paths, base_track_id)?;
    if doc.schema_version != SUBTITLE_JSON_SCHEMA_VERSION {
        return Err(EngineError::InstallFailed(format!(
            "unsupported subtitle schema_version: {}",
            doc.schema_version
        )));
    }

    // Ensure doc kind/lang align with the track metadata.
    doc.kind = base.kind.clone();
    if doc.lang.trim().is_empty() {
        doc.lang = base.lang.clone();
    }

    let publication_guard = SUBTITLE_VERSION_PUBLICATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let max_version: Option<i64> = {
        let conn = db::open_readonly(paths)?;
        conn.query_row(
            r#"
SELECT MAX(version)
FROM subtitle_track
WHERE item_id=?1 AND kind=?2 AND lang=?3 AND format=?4
"#,
            params![&base.item_id, &base.kind, &base.lang, &base.format],
            |row| row.get(0),
        )?
    };
    let next_version = max_version.unwrap_or(0).max(base.version) + 1;

    let base_path = Path::new(&base.path);
    let parent = base_path.parent().ok_or_else(|| {
        EngineError::InstallFailed("subtitle track path has no parent directory".to_string())
    })?;
    let stem = versionless_stem(base_path).unwrap_or_else(|| "track".to_string());

    let json_path = parent.join(format!("{stem}.v{next_version}.json"));
    let srt_path = parent.join(format!("{stem}.v{next_version}.srt"));
    let vtt_path = parent.join(format!("{stem}.v{next_version}.vtt"));

    // The publication lock preserves version selection while this deliberately slow filesystem
    // phase runs. No AppDatabase read or writer admission is alive across the callback.
    artifact_writer(&doc, &json_path, &srt_path, &vtt_path)?;

    let id = Uuid::new_v4().to_string();
    let conn = db::write_context(paths)?;
    conn.execute(
        r#"
INSERT INTO subtitle_track (
  id,
  item_id,
  kind,
  lang,
  format,
  path,
  created_by,
  version
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
"#,
        params![
            &id,
            &base.item_id,
            &base.kind,
            &base.lang,
            &base.format,
            json_path.to_string_lossy().to_string(),
            "user",
            next_version
        ],
    )?;
    drop(conn);
    drop(publication_guard);

    Ok(SubtitleTrackRow {
        id,
        item_id: base.item_id,
        kind: base.kind,
        lang: base.lang,
        format: base.format,
        path: json_path.to_string_lossy().to_string(),
        created_by: "user".to_string(),
        version: next_version,
    })
}

pub fn export_document_srt(doc: &SubtitleDocument, out_path: &Path) -> Result<()> {
    let text = crate::subtitles::render_srt(doc)?;
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out_path, text)?;
    Ok(())
}

pub fn export_document_vtt(doc: &SubtitleDocument, out_path: &Path) -> Result<()> {
    let text = crate::subtitles::render_vtt(doc)?;
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out_path, text)?;
    Ok(())
}

fn versionless_stem(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy().to_string();
    if let Some(pos) = stem.rfind(".v") {
        let suffix = &stem[(pos + 2)..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            return Some(stem[..pos].to_string());
        }
    }
    Some(stem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    use crate::subtitles::{SubtitleDocument, SubtitleSegment};
    use std::sync::mpsc;
    use std::time::Duration;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn versionless_stem_strips_trailing_version_suffix() {
        assert_eq!(
            versionless_stem(Path::new("source.v2.json")).as_deref(),
            Some("source")
        );
        assert_eq!(
            versionless_stem(Path::new("source.json")).as_deref(),
            Some("source")
        );
    }

    #[test]
    fn save_new_version_creates_new_file_and_row() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().to_path_buf());
        db::ensure_schema(&paths).expect("schema");

        // Seed a library item row.
        let item_id = "item-1";
        let conn = db::open(&paths).expect("open fixture");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            r#"
INSERT INTO library_item (
  id,
  created_at_ms,
  source_type,
  source_uri,
  title,
  media_path
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
"#,
            params![
                item_id,
                now_ms_test(),
                "local_file",
                "file:///tmp",
                "Test",
                "media/test.mp4"
            ],
        )
        .expect("insert item");

        // Seed a base subtitle track + file.
        let base_dir = paths.derived_item_dir(item_id).join("asr");
        std::fs::create_dir_all(&base_dir).expect("mkdir");
        let base_json_path = base_dir.join("source.json");

        let base_doc = SubtitleDocument {
            schema_version: SUBTITLE_JSON_SCHEMA_VERSION,
            kind: "source".to_string(),
            lang: "ja".to_string(),
            segments: vec![SubtitleSegment {
                index: 0,
                start_ms: 0,
                end_ms: 1000,
                text: "hello".to_string(),
                speaker: None,
            }],
        };
        crate::subtitles::write_artifacts(
            &base_doc,
            &base_json_path,
            &base_dir.join("source.srt"),
            &base_dir.join("source.vtt"),
        )
        .expect("write artifacts");

        let base_track_id = "track-1";
        conn.execute(
            r#"
INSERT INTO subtitle_track (
  id,
  item_id,
  kind,
  lang,
  format,
  path,
  created_by,
  version
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
"#,
            params![
                base_track_id,
                item_id,
                "source",
                "ja",
                "ytfetch_subtitle_json_v1",
                base_json_path.to_string_lossy().to_string(),
                "asr:test",
                1_i64
            ],
        )
        .expect("insert track");

        let mut edited = base_doc.clone();
        edited.segments[0].text = "edited".to_string();

        let saved = save_new_version(&paths, base_track_id, edited).expect("save");
        assert_eq!(saved.version, 2);
        assert!(Path::new(&saved.path).exists());
        assert!(base_json_path.exists());

        let all = list_tracks(&paths, item_id).expect("list");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn blocked_subtitle_artifact_write_does_not_hold_database_writer_admission() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        let (base_track_id, base_doc) = seed_save_fixture(&paths, "permit");
        let save_paths = paths.clone();
        let (filesystem_entered_tx, filesystem_entered_rx) = mpsc::sync_channel(1);
        let (filesystem_release_tx, filesystem_release_rx) = mpsc::sync_channel(1);
        let save = std::thread::spawn(move || {
            save_new_version_with_writer(
                &save_paths,
                &base_track_id,
                base_doc,
                move |doc, json, srt, vtt| {
                    filesystem_entered_tx
                        .send(())
                        .expect("announce blocked I/O");
                    filesystem_release_rx.recv().expect("release blocked I/O");
                    crate::subtitles::write_artifacts(doc, json, srt, vtt)
                },
            )
        });

        filesystem_entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("save reached filesystem phase");
        let writer_paths = paths.clone();
        let (writer_done_tx, writer_done_rx) = mpsc::sync_channel(1);
        let writer = std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                let conn = db::write_context(&writer_paths)?;
                conn.execute(
                    "INSERT OR REPLACE INTO meta(key,value) VALUES('subtitle_io_scope_probe','admitted')",
                    [],
                )?;
                Ok(())
            })();
            writer_done_tx.send(result).expect("writer result");
        });

        let writer_result = writer_done_rx.recv_timeout(Duration::from_secs(2));
        filesystem_release_tx.send(()).expect("release save");
        let save_result = save.join().expect("save thread");
        writer.join().expect("writer thread");
        writer_result
            .expect("unrelated writer must be admitted while filesystem work is blocked")
            .expect("unrelated writer succeeds");
        save_result.expect("save succeeds");
    }

    #[test]
    fn concurrent_subtitle_saves_publish_distinct_monotonic_versions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path().join("app_state"));
        let (base_track_id, base_doc) = seed_save_fixture(&paths, "versions");
        let mut workers = Vec::new();
        for text in ["first edit", "second edit"] {
            let paths = paths.clone();
            let track_id = base_track_id.clone();
            let mut doc = base_doc.clone();
            doc.segments[0].text = text.to_string();
            workers.push(std::thread::spawn(move || {
                save_new_version(&paths, &track_id, doc)
            }));
        }
        let mut saved = workers
            .into_iter()
            .map(|worker| worker.join().expect("save thread").expect("save"))
            .collect::<Vec<_>>();
        saved.sort_by_key(|row| row.version);
        assert_eq!(
            saved.iter().map(|row| row.version).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_ne!(saved[0].path, saved[1].path);
        let texts = saved
            .iter()
            .map(|row| {
                load_document_from_path(Path::new(&row.path))
                    .expect("published document")
                    .segments[0]
                    .text
                    .clone()
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            texts,
            std::collections::HashSet::from(["first edit".to_string(), "second edit".to_string()])
        );
    }

    fn seed_save_fixture(paths: &AppPaths, suffix: &str) -> (String, SubtitleDocument) {
        db::ensure_schema(paths).expect("schema");
        let item_id = format!("item-{suffix}");
        let conn = db::open(paths).expect("open fixture");
        db::migrate(&conn).expect("migrate");
        conn.execute(
            "INSERT INTO library_item(id,created_at_ms,source_type,source_uri,title,media_path) VALUES(?1,?2,'local_file','file:///tmp','Test','media/test.mkv')",
            params![&item_id, now_ms_test()],
        )
        .expect("insert item");
        let base_dir = paths.derived_item_dir(&item_id).join("asr");
        std::fs::create_dir_all(&base_dir).expect("mkdir");
        let base_json_path = base_dir.join("source.json");
        let base_doc = SubtitleDocument {
            schema_version: SUBTITLE_JSON_SCHEMA_VERSION,
            kind: "source".to_string(),
            lang: "ja".to_string(),
            segments: vec![SubtitleSegment {
                index: 0,
                start_ms: 0,
                end_ms: 1000,
                text: "hello".to_string(),
                speaker: None,
            }],
        };
        crate::subtitles::write_artifacts(
            &base_doc,
            &base_json_path,
            &base_dir.join("source.srt"),
            &base_dir.join("source.vtt"),
        )
        .expect("base artifacts");
        let track_id = format!("track-{suffix}");
        conn.execute(
            "INSERT INTO subtitle_track(id,item_id,kind,lang,format,path,created_by,version) VALUES(?1,?2,'source','ja','ytfetch_subtitle_json_v1',?3,'asr:test',1)",
            params![&track_id, &item_id, base_json_path.to_string_lossy().to_string()],
        )
        .expect("insert track");
        drop(conn);
        (track_id, base_doc)
    }

    fn now_ms_test() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }
}

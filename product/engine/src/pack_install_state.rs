//! WP-0234: per-pack install state journal.
//!
//! Records the most recent install attempt for each Python pack so that:
//!
//! - A crashed install (process killed while pip was running) is detected on the next
//!   attempt and the install is promoted to `--force-reinstall`, overwriting the
//!   half-broken venv state instead of layering on top.
//! - A previously-failed install (warmup error, dependency resolution error) is
//!   similarly promoted to `--force-reinstall` so retry has a real chance to recover.
//! - A previously-successful install with the same lockfile hash can short-circuit
//!   to a fast `--upgrade` (idempotent) rather than re-downloading wheels.
//! - WP-0236's Repair surface reads the same state file so the UI and the engine
//!   agree about what happened.
//!
//! Storage: one JSON file per pack at
//! `<APPDATA>/com.voxvulgi.voxvulgi/tools/python/install_state/<pack>.json`.
//! Atomic update via write-temp-then-rename.

use crate::paths::AppPaths;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LastOutcome {
    /// No prior install attempt recorded for this pack.
    Unknown,
    /// `mark_started` was written but neither `mark_completed` nor `mark_failed`
    /// was written. The previous install attempt crashed before it could finish.
    /// Treated identically to `Failed` for the purpose of recovery.
    InProgress,
    /// Install + warmup completed successfully.
    Completed,
    /// Install or warmup returned an error.
    Failed,
}

impl LastOutcome {
    /// Returns true when the next install attempt should force-reinstall instead of
    /// layering pip install --upgrade on top of the existing state.
    pub fn requires_force_reinstall(&self) -> bool {
        matches!(self, LastOutcome::InProgress | LastOutcome::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackInstallState {
    /// Pack name, e.g. `tts_neural_local_v1`.
    pub pack: String,
    /// Hex sha256 of the rendered hashed-requirements body that was installed.
    /// Empty when the install used the legacy pinned-list path (no lockfile).
    #[serde(default)]
    pub lockfile_sha: String,
    /// Unix milliseconds. Updated by `mark_started`.
    #[serde(default)]
    pub started_at_ms: u64,
    /// Unix milliseconds. Updated by `mark_completed` / `mark_failed`. Zero while in_progress.
    #[serde(default)]
    pub finished_at_ms: u64,
    pub last_outcome: LastOutcome,
    /// Free-text error message captured on failure. Truncated to ~1 KB for log size sanity.
    #[serde(default)]
    pub last_error: String,
}

impl PackInstallState {
    pub fn unknown(pack: &str) -> Self {
        Self {
            pack: pack.to_string(),
            lockfile_sha: String::new(),
            started_at_ms: 0,
            finished_at_ms: 0,
            last_outcome: LastOutcome::Unknown,
            last_error: String::new(),
        }
    }

    /// Returns true when the *current* install plan (same pack, same lockfile sha) can
    /// be skipped because the previous attempt already completed successfully.
    /// Note: callers may still choose to install anyway (e.g. operator clicked Repair);
    /// this is informational, not enforced.
    pub fn is_completed_with_lockfile(&self, lockfile_sha: &str) -> bool {
        self.last_outcome == LastOutcome::Completed
            && !self.lockfile_sha.is_empty()
            && self.lockfile_sha == lockfile_sha
    }
}

fn state_path(paths: &AppPaths, pack: &str) -> PathBuf {
    paths
        .python_install_state_dir()
        .join(format!("{pack}.json"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Compute a stable sha256 of the rendered requirements body (or any install plan
/// string) so the state file records exactly what was installed. Pure helper; no IO.
pub fn lockfile_sha_of(rendered: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(rendered.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Load the install state for a pack. Returns `unknown(pack)` if the file is missing
/// or corrupt — corruption is treated as "unknown" so we err toward force-reinstall
/// rather than refusing to install at all.
pub fn load(paths: &AppPaths, pack: &str) -> PackInstallState {
    let path = state_path(paths, pack);
    if !path.exists() {
        return PackInstallState::unknown(pack);
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
            // Corrupt journal: treat as unknown.
            PackInstallState::unknown(pack)
        }),
        Err(_) => PackInstallState::unknown(pack),
    }
}

/// Atomically write the state file. Writes to `<path>.tmp` first then renames over the
/// destination; on Windows this maps to a single `MoveFileEx(MOVEFILE_REPLACE_EXISTING)`.
fn save(paths: &AppPaths, state: &PackInstallState) -> std::io::Result<()> {
    let dir = paths.python_install_state_dir();
    std::fs::create_dir_all(&dir)?;
    let final_path = dir.join(format!("{}.json", state.pack));
    let tmp_path = dir.join(format!("{}.json.tmp", state.pack));
    {
        let mut f = std::fs::File::create(&tmp_path)?;
        f.write_all(
            serde_json::to_string_pretty(state)
                .unwrap_or_else(|_| "{}".to_string())
                .as_bytes(),
        )?;
        f.sync_all()?;
    }
    // Replace existing file (Windows: MoveFileEx REPLACE_EXISTING).
    std::fs::rename(&tmp_path, &final_path)
}

/// Mark an install attempt as started. Records the lockfile sha being installed so
/// that the next call to `load` can compare against this and detect a crash.
pub fn mark_started(paths: &AppPaths, pack: &str, lockfile_sha: &str) -> std::io::Result<()> {
    let state = PackInstallState {
        pack: pack.to_string(),
        lockfile_sha: lockfile_sha.to_string(),
        started_at_ms: now_ms(),
        finished_at_ms: 0,
        last_outcome: LastOutcome::InProgress,
        last_error: String::new(),
    };
    save(paths, &state)
}

/// Mark an install attempt as completed successfully.
pub fn mark_completed(paths: &AppPaths, pack: &str, lockfile_sha: &str) -> std::io::Result<()> {
    let mut state = load(paths, pack);
    state.lockfile_sha = lockfile_sha.to_string();
    if state.started_at_ms == 0 {
        state.started_at_ms = now_ms();
    }
    state.finished_at_ms = now_ms();
    state.last_outcome = LastOutcome::Completed;
    state.last_error.clear();
    save(paths, &state)
}

/// Mark an install attempt as failed with a captured error.
pub fn mark_failed(paths: &AppPaths, pack: &str, error_message: &str) -> std::io::Result<()> {
    let mut state = load(paths, pack);
    state.finished_at_ms = now_ms();
    state.last_outcome = LastOutcome::Failed;
    // Truncate the captured error to keep journal files small.
    let truncated = if error_message.len() > 1024 {
        format!("{}…[truncated]", &error_message[..1024])
    } else {
        error_message.to_string()
    };
    state.last_error = truncated;
    save(paths, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fresh_paths(tmp: &Path) -> AppPaths {
        // Build a minimal AppPaths rooted at the given temp dir so the install_state
        // journal lives entirely under tmp/.
        AppPaths::new(tmp.to_path_buf())
    }

    #[test]
    fn lockfile_sha_is_stable_for_same_input() {
        let a = lockfile_sha_of("foo==1.0 --hash=sha256:abc\n");
        let b = lockfile_sha_of("foo==1.0 --hash=sha256:abc\n");
        assert_eq!(a, b);
        assert_ne!(a, lockfile_sha_of("foo==1.0 --hash=sha256:abd\n"));
        assert_eq!(a.len(), 64); // hex sha256
    }

    #[test]
    fn load_returns_unknown_when_file_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = fresh_paths(tmp.path());
        let state = load(&paths, "tts_neural_local_v1");
        assert_eq!(state.last_outcome, LastOutcome::Unknown);
        assert!(state.lockfile_sha.is_empty());
    }

    #[test]
    fn mark_started_then_completed_round_trips() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = fresh_paths(tmp.path());
        let sha = "a".repeat(64);
        mark_started(&paths, "tts_neural_local_v1", &sha).expect("mark_started");
        let mid = load(&paths, "tts_neural_local_v1");
        assert_eq!(mid.last_outcome, LastOutcome::InProgress);
        assert_eq!(mid.lockfile_sha, sha);
        assert!(mid.started_at_ms > 0);
        assert_eq!(mid.finished_at_ms, 0);

        mark_completed(&paths, "tts_neural_local_v1", &sha).expect("mark_completed");
        let end = load(&paths, "tts_neural_local_v1");
        assert_eq!(end.last_outcome, LastOutcome::Completed);
        assert_eq!(end.lockfile_sha, sha);
        assert!(end.finished_at_ms >= end.started_at_ms);
        assert!(end.last_error.is_empty());
    }

    #[test]
    fn mark_failed_records_error_and_truncates_long_messages() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = fresh_paths(tmp.path());
        mark_started(&paths, "demucs", &"b".repeat(64)).expect("mark_started");
        let long = "boom!".repeat(500); // 2500 chars
        mark_failed(&paths, "demucs", &long).expect("mark_failed");
        let state = load(&paths, "demucs");
        assert_eq!(state.last_outcome, LastOutcome::Failed);
        assert!(state.last_error.len() <= 1024 + "…[truncated]".len() + 8);
        assert!(state.last_error.starts_with("boom!boom!"));
        assert!(state.last_error.ends_with("…[truncated]"));
    }

    #[test]
    fn in_progress_state_requires_force_reinstall_on_next_attempt() {
        // Simulate a crashed install: mark_started was called but the process died
        // before mark_completed / mark_failed could run. The next attempt's load()
        // sees InProgress and should be told to force-reinstall.
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = fresh_paths(tmp.path());
        mark_started(&paths, "diarization", &"c".repeat(64)).expect("mark_started");
        let state = load(&paths, "diarization");
        assert!(state.last_outcome.requires_force_reinstall());
    }

    #[test]
    fn failed_state_requires_force_reinstall_on_next_attempt() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = fresh_paths(tmp.path());
        mark_started(&paths, "tts_preview", &"d".repeat(64)).expect("mark_started");
        mark_failed(&paths, "tts_preview", "pip exit 1").expect("mark_failed");
        let state = load(&paths, "tts_preview");
        assert!(state.last_outcome.requires_force_reinstall());
    }

    #[test]
    fn completed_state_does_not_require_force_reinstall() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = fresh_paths(tmp.path());
        let sha = "e".repeat(64);
        mark_started(&paths, "demucs", &sha).expect("mark_started");
        mark_completed(&paths, "demucs", &sha).expect("mark_completed");
        let state = load(&paths, "demucs");
        assert!(!state.last_outcome.requires_force_reinstall());
        assert!(state.is_completed_with_lockfile(&sha));
        assert!(!state.is_completed_with_lockfile(&"f".repeat(64)));
    }

    #[test]
    fn corrupt_journal_is_treated_as_unknown() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = fresh_paths(tmp.path());
        // Write garbage to the journal file.
        let dir = paths.python_install_state_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tts_neural_local_v1.json"), b"this is not json").unwrap();
        let state = load(&paths, "tts_neural_local_v1");
        assert_eq!(state.last_outcome, LastOutcome::Unknown);
    }
}

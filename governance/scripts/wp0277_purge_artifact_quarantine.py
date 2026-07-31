#!/usr/bin/env python3
"""Permanently purge WP-0277 quarantined download artifacts."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import ntpath
import os
from pathlib import Path
import sqlite3
import stat as statlib
import sys
import time
from typing import Any


RESULT_SCHEMA = "voxvulgi.wp0277.cleanup_artifact_hash_result.v1"
AUDIT_SCHEMA = "voxvulgi.wp0277.final_cleanup_audit.v1"
RECEIPT_SCHEMA = "voxvulgi.wp0277.artifact_quarantine_purge.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-result", required=True)
    parser.add_argument("--expected-artifact-result-sha256", required=True)
    parser.add_argument("--final-audit", required=True)
    parser.add_argument("--expected-final-audit-sha256", required=True)
    parser.add_argument("--quarantine-root", required=True)
    parser.add_argument("--database", required=True)
    parser.add_argument("--quarantine-ledger", required=True)
    parser.add_argument("--purge-ledger", required=True)
    parser.add_argument("--receipt", required=True)
    parser.add_argument("--apply", action="store_true")
    return parser.parse_args()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(8 * 1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


def verify_pinned_json(
    path: Path,
    expected_sha256: str,
    schema: str,
) -> tuple[dict[str, Any], str]:
    actual = sha256_file(path)
    if actual != expected_sha256.strip().upper():
        raise RuntimeError(f"pinned hash mismatch for {path}: {actual}")
    sidecar = path.with_suffix(path.suffix + ".sha256")
    if not sidecar.is_file():
        raise RuntimeError(f"missing hash sidecar: {sidecar}")
    recorded = sidecar.read_text(encoding="utf-8").split()[0].upper()
    if recorded != actual:
        raise RuntimeError(f"hash sidecar mismatch for {path}")
    value = json.loads(path.read_text(encoding="utf-8"))
    if value.get("schema") != schema:
        raise RuntimeError(f"unexpected schema in {path}: {value.get('schema')!r}")
    return value, actual


def normalize_windows_path(value: str) -> str:
    value = value.strip().replace("/", "\\")
    lowered = value.casefold()
    if lowered.startswith("\\\\?\\unc\\"):
        value = "\\\\" + value[8:]
    elif lowered.startswith("\\\\?\\"):
        value = value[4:]
    return ntpath.normpath(value).rstrip("\\").casefold()


def require_descendant(path_value: str, root_value: str) -> None:
    path = normalize_windows_path(path_value)
    root = normalize_windows_path(root_value)
    if path == root or not path.startswith(root + "\\"):
        raise RuntimeError(f"purge path escapes quarantine root: {path_value}")


def regular_file(path: Path) -> tuple[os.stat_result | None, str | None]:
    try:
        observed = path.stat()
    except OSError as error:
        return None, f"{type(error).__name__}:{error}"
    if not statlib.S_ISREG(observed.st_mode):
        return None, "not_regular_file"
    return observed, None


def ledger_status(path: Path) -> dict[str, int]:
    conn = sqlite3.connect(path)
    try:
        return {
            status: count
            for status, count in conn.execute(
                "SELECT status, COUNT(*) FROM action GROUP BY status"
            )
        }
    finally:
        conn.close()


def open_purge_ledger(path: Path) -> sqlite3.Connection:
    path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(path)
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA synchronous=FULL")
    conn.execute(
        """
CREATE TABLE IF NOT EXISTS action (
  action_id TEXT PRIMARY KEY,
  quarantine_path TEXT NOT NULL UNIQUE,
  expected_size_bytes INTEGER NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('pending', 'deleting', 'deleted', 'error')),
  error TEXT,
  updated_at_ms INTEGER NOT NULL
)
"""
    )
    conn.commit()
    return conn


def assert_idle_queue(conn: sqlite3.Connection) -> None:
    paused = conn.execute(
        "SELECT value FROM meta WHERE key='jobs_queue_paused'"
    ).fetchone()
    if paused is None or paused[0] != "1":
        raise RuntimeError("queue is not paused")
    running = conn.execute(
        "SELECT COUNT(*) FROM job WHERE status='running'"
    ).fetchone()[0]
    if running != 0:
        raise RuntimeError(f"running jobs exist: {running}")


def write_receipt(path: Path, value: dict[str, Any]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    encoded = (
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    ).encode("utf-8")
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_bytes(encoded)
    os.replace(temporary, path)
    digest = hashlib.sha256(encoded).hexdigest().upper()
    path.with_suffix(path.suffix + ".sha256").write_text(
        f"{digest}  {path.name}\n",
        encoding="utf-8",
    )
    return digest


def main() -> int:
    args = parse_args()
    result_path = Path(args.artifact_result)
    audit_path = Path(args.final_audit)
    database_path = Path(args.database)
    quarantine_ledger_path = Path(args.quarantine_ledger)
    purge_ledger_path = Path(args.purge_ledger)
    receipt_path = Path(args.receipt)
    result, result_sha = verify_pinned_json(
        result_path,
        args.expected_artifact_result_sha256,
        RESULT_SCHEMA,
    )
    audit, audit_sha = verify_pinned_json(
        audit_path,
        args.expected_final_audit_sha256,
        AUDIT_SCHEMA,
    )
    actions = result.get("actions", [])
    if len(actions) != 435:
        raise RuntimeError(f"unexpected artifact action count: {len(actions)}")
    if sum(int(action["observed_size_bytes"]) for action in actions) != 123181662082:
        raise RuntimeError("unexpected artifact byte total")
    if audit.get("summary", {}).get("assertions_passed") is not True:
        raise RuntimeError("final audit did not pass")
    if audit.get("summary", {}).get("artifact_quarantine_actions") != len(actions):
        raise RuntimeError("final audit artifact count does not match result")
    for assertion in (
        "all_artifact_actions_applied",
        "all_artifact_destinations_present_size_and_hash_matched",
        "all_artifact_evidence_quarantined",
        "all_artifact_sources_absent",
        "database_quick_check_ok",
        "queue_remains_paused",
        "zero_foreign_key_violations",
        "zero_running_direct_jobs",
    ):
        if audit.get("assertions", {}).get(assertion) is not True:
            raise RuntimeError(f"required final-audit assertion is not true: {assertion}")
    if ledger_status(quarantine_ledger_path) != {"applied": len(actions)}:
        raise RuntimeError("artifact quarantine ledger is not fully applied")

    action_ids: set[str] = set()
    quarantine_paths: set[str] = set()
    expected_bytes = 0
    for action in actions:
        action_id = action["action_id"]
        if action_id in action_ids:
            raise RuntimeError(f"duplicate action id: {action_id}")
        action_ids.add(action_id)
        require_descendant(action["quarantine_path"], args.quarantine_root)
        normalized = normalize_windows_path(action["quarantine_path"])
        if normalized in quarantine_paths:
            raise RuntimeError(f"duplicate quarantine path: {action['quarantine_path']}")
        quarantine_paths.add(normalized)
        expected_bytes += int(action["observed_size_bytes"])

    db = sqlite3.connect(database_path)
    try:
        db.execute("PRAGMA foreign_keys=ON")
        if db.execute("PRAGMA quick_check").fetchone()[0] != "ok":
            raise RuntimeError("database quick_check failed")
        if db.execute("PRAGMA foreign_key_check").fetchall():
            raise RuntimeError("database foreign-key violations exist")
        assert_idle_queue(db)
        evidence = {
            source_key: (evidence_id, match_state, json.loads(details_json))
            for evidence_id, source_key, match_state, details_json in db.execute(
                """
SELECT id, source_record_key, match_state, details_json
FROM media_import_evidence
WHERE evidence_kind='wp0277_cleanup_artifact'
"""
            )
        }
        if len(evidence) != len(actions):
            raise RuntimeError(f"artifact evidence coverage mismatch: {len(evidence)}")
        for action in actions:
            row = evidence.get(action["normalized_source_path"])
            if row is None:
                raise RuntimeError(f"artifact evidence missing: {action['action_id']}")
            _, state, details = row
            if state not in {"quarantined", "purged"}:
                raise RuntimeError(
                    f"artifact evidence state drifted: {action['action_id']}={state}"
                )
            for key in (
                "artifact_reason",
                "evidence_size_bytes",
                "evidence_modified_ns",
                "observed_size_bytes",
                "observed_modified_ns",
            ):
                if details.get(key) != action[key]:
                    raise RuntimeError(
                        f"artifact evidence preimage drifted: {action['action_id']}:{key}"
                    )
            if normalize_windows_path(details.get("quarantine_path", "")) != (
                normalize_windows_path(action["quarantine_path"])
            ):
                raise RuntimeError(
                    f"artifact evidence quarantine path drifted: {action['action_id']}"
                )
    finally:
        db.close()

    purge = open_purge_ledger(purge_ledger_path)
    try:
        now = int(time.time() * 1000)
        for action in actions:
            purge.execute(
                """
INSERT INTO action (
  action_id, quarantine_path, expected_size_bytes, status, error, updated_at_ms
) VALUES (?, ?, ?, 'pending', NULL, ?)
ON CONFLICT(action_id) DO UPDATE SET
  quarantine_path=excluded.quarantine_path,
  expected_size_bytes=excluded.expected_size_bytes
""",
                (
                    action["action_id"],
                    action["quarantine_path"],
                    int(action["observed_size_bytes"]),
                    now,
                ),
            )
        purge.commit()
        if {
            row[0] for row in purge.execute("SELECT action_id FROM action")
        } != action_ids:
            raise RuntimeError("purge ledger action set does not match result")

        present = 0
        present_bytes = 0
        for index, action in enumerate(actions, start=1):
            state = purge.execute(
                "SELECT status FROM action WHERE action_id=?",
                (action["action_id"],),
            ).fetchone()[0]
            destination = Path(action["quarantine_path"])
            if state == "deleted":
                if destination.exists():
                    raise RuntimeError(
                        f"ledger says deleted but path exists: {destination}"
                    )
                continue
            if state == "deleting" and not destination.exists():
                purge.execute(
                    "UPDATE action SET status='deleted', error=NULL, updated_at_ms=? "
                    "WHERE action_id=?",
                    (int(time.time() * 1000), action["action_id"]),
                )
                purge.commit()
                continue
            observed, error = regular_file(destination)
            if error or observed is None:
                raise RuntimeError(
                    f"artifact quarantine file unavailable: {destination}: {error}"
                )
            if observed.st_size != int(action["observed_size_bytes"]):
                raise RuntimeError(f"artifact quarantine size changed: {destination}")
            if observed.st_mtime_ns != int(action["observed_modified_ns"]):
                raise RuntimeError(
                    f"artifact quarantine timestamp changed: {destination}"
                )
            if Path(action["source_path"]).exists():
                raise RuntimeError(
                    f"artifact source unexpectedly exists: {action['source_path']}"
                )
            linked_keeper = action.get("current_linked_keeper")
            if linked_keeper is not None:
                keeper, keeper_error = regular_file(Path(linked_keeper["media_path"]))
                if keeper_error or keeper is None or keeper.st_size == 0:
                    raise RuntimeError(
                        f"linked keeper unavailable: {linked_keeper['media_path']}"
                    )
            present += 1
            present_bytes += observed.st_size
            if index % 50 == 0:
                print(
                    f"artifact_preflight_progress={index}/{len(actions)}",
                    flush=True,
                )

        if not args.apply:
            print(
                json.dumps(
                    {
                        "apply": False,
                        "actions": len(actions),
                        "bytes": expected_bytes,
                        "present_to_purge": present,
                        "present_bytes": present_bytes,
                    },
                    sort_keys=True,
                )
            )
            return 0

        started_at = dt.datetime.now().astimezone().isoformat()
        for index, action in enumerate(actions, start=1):
            state = purge.execute(
                "SELECT status FROM action WHERE action_id=?",
                (action["action_id"],),
            ).fetchone()[0]
            if state == "deleted":
                continue
            destination = Path(action["quarantine_path"])
            purge.execute(
                "UPDATE action SET status='deleting', error=NULL, updated_at_ms=? "
                "WHERE action_id=?",
                (int(time.time() * 1000), action["action_id"]),
            )
            purge.commit()
            try:
                destination.unlink()
            except OSError as error:
                purge.execute(
                    "UPDATE action SET status='error', error=?, updated_at_ms=? "
                    "WHERE action_id=?",
                    (
                        f"{type(error).__name__}:{error}",
                        int(time.time() * 1000),
                        action["action_id"],
                    ),
                )
                purge.commit()
                raise
            if destination.exists():
                raise RuntimeError(f"purged path still exists: {destination}")
            purge.execute(
                "UPDATE action SET status='deleted', error=NULL, updated_at_ms=? "
                "WHERE action_id=?",
                (int(time.time() * 1000), action["action_id"]),
            )
            purge.commit()
            if index % 25 == 0:
                print(f"artifact_purge_progress={index}/{len(actions)}", flush=True)

        statuses = {
            status: count
            for status, count in purge.execute(
                "SELECT status, COUNT(*) FROM action GROUP BY status"
            )
        }
        if statuses != {"deleted": len(actions)}:
            raise RuntimeError(f"artifact purge did not finish cleanly: {statuses}")
    finally:
        purge.close()

    purged_at_ms = int(time.time() * 1000)
    db = sqlite3.connect(database_path)
    try:
        db.execute("PRAGMA foreign_keys=ON")
        db.execute("BEGIN IMMEDIATE")
        assert_idle_queue(db)
        for action in actions:
            row = db.execute(
                """
SELECT id, match_state, details_json
FROM media_import_evidence
WHERE evidence_kind='wp0277_cleanup_artifact' AND source_record_key=?
""",
                (action["normalized_source_path"],),
            ).fetchone()
            if row is None:
                raise RuntimeError(f"artifact evidence disappeared: {action['action_id']}")
            evidence_id, state, details_json = row
            details = json.loads(details_json)
            details.update(
                {
                    "purge_state": "purged",
                    "purged_at_ms": purged_at_ms,
                    "artifact_hash_result_sha256": result_sha,
                    "final_audit_sha256": audit_sha,
                }
            )
            changed = db.execute(
                "UPDATE media_import_evidence SET match_state='purged', "
                "details_json=?, updated_at_ms=? WHERE id=? AND match_state=?",
                (
                    json.dumps(details, sort_keys=True),
                    purged_at_ms,
                    evidence_id,
                    state,
                ),
            ).rowcount
            if changed != 1:
                raise RuntimeError(f"artifact evidence changed: {action['action_id']}")
        if db.execute("PRAGMA foreign_key_check").fetchall():
            raise RuntimeError("foreign-key violations after evidence update")
        db.commit()
    except Exception:
        db.rollback()
        raise
    finally:
        db.close()

    db = sqlite3.connect(f"file:{database_path.as_posix()}?mode=ro", uri=True)
    try:
        assert_idle_queue(db)
        quick_check = db.execute("PRAGMA quick_check").fetchone()[0]
        foreign_keys = len(db.execute("PRAGMA foreign_key_check").fetchall())
        states = {
            state: count
            for state, count in db.execute(
                "SELECT match_state, COUNT(*) FROM media_import_evidence "
                "WHERE evidence_kind='wp0277_cleanup_artifact' GROUP BY match_state"
            )
        }
    finally:
        db.close()
    if quick_check != "ok" or foreign_keys != 0:
        raise RuntimeError("database validation failed after artifact purge")
    if states != {"purged": len(actions)}:
        raise RuntimeError(f"artifact evidence did not reach purged: {states}")
    remaining = [
        action["quarantine_path"]
        for action in actions
        if Path(action["quarantine_path"]).exists()
    ]
    if remaining:
        raise RuntimeError(f"purged artifact paths remain: {len(remaining)}")

    receipt = {
        "schema": RECEIPT_SCHEMA,
        "work_packet": "WP-0277",
        "started_at": started_at,
        "completed_at": dt.datetime.now().astimezone().isoformat(),
        "artifact_result": {"path": str(result_path), "sha256": result_sha},
        "final_audit": {"path": str(audit_path), "sha256": audit_sha},
        "quarantine_root": args.quarantine_root,
        "purge_ledger": str(purge_ledger_path),
        "deleted_files": len(actions),
        "deleted_bytes": expected_bytes,
        "remaining_manifest_paths": 0,
        "artifact_evidence_states": states,
        "queue_paused": True,
        "running_jobs": 0,
        "database_quick_check": quick_check,
        "foreign_key_violations": foreign_keys,
        "status": "complete",
    }
    receipt_sha = write_receipt(receipt_path, receipt)
    print(
        json.dumps(
            {
                "status": "complete",
                "deleted_files": len(actions),
                "deleted_bytes": expected_bytes,
                "receipt": str(receipt_path),
                "receipt_sha256": receipt_sha,
            },
            sort_keys=True,
        ),
        flush=True,
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"{type(error).__name__}: {error}", file=sys.stderr, flush=True)
        raise

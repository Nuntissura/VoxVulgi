#!/usr/bin/env python3
"""Apply or roll back the fully-hashed WP-0277 artifact quarantine."""

from __future__ import annotations

import argparse
import datetime as dt
import errno
import hashlib
import json
import ntpath
import os
from pathlib import Path
import shutil
import sqlite3
import sys
import time
from typing import Any


RESULT_SCHEMA = "voxvulgi.wp0277.cleanup_artifact_hash_result.v1"
RECEIPT_SCHEMA = "voxvulgi.wp0277.cleanup_artifact_quarantine_apply.v1"
PLAN_SCHEMA = "voxvulgi.cleanup_artifact_quarantine_plan.v1"
BACKUP_TABLES = (
    "meta",
    "video_library",
    "job",
    "library_item",
    "media_source_identity",
    "media_source_alias",
    "media_import_evidence",
    "ingest_provenance",
    "library_download_lineage",
    "media_source_membership",
    "media_source_association",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--result", required=True)
    parser.add_argument("--database", required=True)
    parser.add_argument("--backup", required=True)
    parser.add_argument("--ledger", required=True)
    parser.add_argument("--receipt", required=True)
    parser.add_argument("--expected-result-sha256", required=True)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--apply", action="store_true")
    mode.add_argument("--rollback", action="store_true")
    return parser.parse_args()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(8 * 1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


def normalize_path(value: str) -> str:
    value = value.strip().replace("/", "\\")
    lowered = value.casefold()
    if lowered.startswith("\\\\?\\unc\\"):
        value = "\\\\" + value[8:]
    elif lowered.startswith("\\\\?\\"):
        value = value[4:]
    return value.rstrip("\\").casefold()


def acquire_run_lock(path: Path):
    path.parent.mkdir(parents=True, exist_ok=True)
    handle = path.open("a+b")
    if handle.seek(0, os.SEEK_END) == 0:
        handle.write(b"0")
        handle.flush()
    handle.seek(0)
    try:
        import msvcrt

        msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
    except ImportError:
        import fcntl

        try:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as error:
            handle.close()
            raise RuntimeError(f"another artifact apply owns {path}") from error
    except OSError as error:
        handle.close()
        raise RuntimeError(f"another artifact apply owns {path}") from error
    return handle


def canonical_row_digests(conn: sqlite3.Connection) -> dict[str, str]:
    digests: dict[str, str] = {}
    for table in BACKUP_TABLES:
        columns = [row[1] for row in conn.execute(f'PRAGMA table_info("{table}")')]
        if not columns:
            raise RuntimeError(f"backup digest table missing: {table}")
        quoted = ", ".join(f'"{column}"' for column in columns)
        digest = hashlib.sha256()
        for row in conn.execute(f'SELECT {quoted} FROM "{table}" ORDER BY {quoted}'):
            stable = [
                {"bytes_hex": value.hex()} if isinstance(value, bytes) else value
                for value in row
            ]
            digest.update(
                (json.dumps(stable, ensure_ascii=False, separators=(",", ":")) + "\n")
                .encode("utf-8")
            )
        digests[table] = digest.hexdigest().upper()
    return digests


def db_state(conn: sqlite3.Connection) -> dict[str, Any]:
    paused = conn.execute(
        "SELECT value FROM meta WHERE key='jobs_queue_paused'"
    ).fetchone()
    return {
        "quick_check": conn.execute("PRAGMA quick_check").fetchone()[0],
        "queue_paused": bool(paused and paused[0] == "1"),
        "running_direct_jobs": conn.execute(
            "SELECT COUNT(*) FROM job "
            "WHERE type='download_direct_url' AND status='running'"
        ).fetchone()[0],
        "queued_direct_jobs": conn.execute(
            "SELECT COUNT(*) FROM job "
            "WHERE type='download_direct_url' AND status='queued'"
        ).fetchone()[0],
        "library_items": conn.execute("SELECT COUNT(*) FROM library_item").fetchone()[0],
        "source_identities": conn.execute(
            "SELECT COUNT(*) FROM media_source_identity"
        ).fetchone()[0],
        "import_evidence": conn.execute(
            "SELECT COUNT(*) FROM media_import_evidence"
        ).fetchone()[0],
        "foreign_key_violations": len(
            conn.execute("PRAGMA foreign_key_check").fetchall()
        ),
    }


def assert_idle_queue(conn: sqlite3.Connection) -> None:
    state = db_state(conn)
    if not state["queue_paused"] or state["running_direct_jobs"] != 0:
        raise RuntimeError("artifact quarantine requires paused idle queue")


def verify_path(path: Path, size_bytes: int, digest: str) -> None:
    stat = path.stat()
    if stat.st_size != size_bytes:
        raise RuntimeError(
            f"size mismatch for {path}: {stat.st_size} != {size_bytes}"
        )
    actual = sha256_file(path)
    if actual != digest.upper():
        raise RuntimeError(f"hash mismatch for {path}: {actual} != {digest}")


def move_verified(source: Path, destination: Path, size_bytes: int, digest: str) -> None:
    if destination.exists():
        raise RuntimeError(f"destination already exists: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    source_preimage = source.stat()
    try:
        source.rename(destination)
    except OSError as error:
        if error.errno != errno.EXDEV and getattr(error, "winerror", None) != 17:
            raise
        partial = destination.with_suffix(destination.suffix + ".partial")
        if partial.exists():
            try:
                verify_path(partial, size_bytes, digest)
            except Exception:
                current_source = source.stat()
                if (
                    current_source.st_dev != source_preimage.st_dev
                    or current_source.st_ino != source_preimage.st_ino
                    or current_source.st_size != source_preimage.st_size
                    or current_source.st_mtime_ns != source_preimage.st_mtime_ns
                ):
                    raise RuntimeError(
                        f"source identity changed before partial recovery: {source}"
                    )
                verify_path(source, size_bytes, digest)
                partial.unlink()
        if not partial.exists():
            with source.open("rb") as input_handle, partial.open("xb") as output_handle:
                shutil.copyfileobj(input_handle, output_handle, 8 * 1024 * 1024)
                output_handle.flush()
                os.fsync(output_handle.fileno())
        verify_path(partial, size_bytes, digest)
        current_source = source.stat()
        if (
            current_source.st_dev != source_preimage.st_dev
            or current_source.st_ino != source_preimage.st_ino
            or current_source.st_size != source_preimage.st_size
            or current_source.st_mtime_ns != source_preimage.st_mtime_ns
        ):
            raise RuntimeError(f"source identity changed during copy fallback: {source}")
        verify_path(source, size_bytes, digest)
        partial.rename(destination)
        os.utime(
            destination,
            ns=(source_preimage.st_atime_ns, source_preimage.st_mtime_ns),
        )
        verify_path(destination, size_bytes, digest)
        current_source = source.stat()
        if (
            current_source.st_dev != source_preimage.st_dev
            or current_source.st_ino != source_preimage.st_ino
            or current_source.st_size != source_preimage.st_size
            or current_source.st_mtime_ns != source_preimage.st_mtime_ns
        ):
            raise RuntimeError(f"source identity changed before unlink: {source}")
        verify_path(source, size_bytes, digest)
        source.unlink()
    try:
        verify_path(destination, size_bytes, digest)
    except Exception:
        if destination.exists() and not source.exists():
            destination.rename(source)
        raise


def validate_result(
    result: dict[str, Any],
    result_path: Path,
    expected_sha256: str,
) -> tuple[str, dict[str, Any]]:
    result_sha = sha256_file(result_path)
    if result_sha != expected_sha256.strip().upper():
        raise RuntimeError(
            f"artifact result hash mismatch: {result_sha} != "
            f"{expected_sha256.strip().upper()}"
        )
    sidecar = result_path.with_suffix(result_path.suffix + ".sha256")
    if not sidecar.is_file() or sidecar.read_text(encoding="utf-8").split()[0].upper() != result_sha:
        raise RuntimeError("artifact result sidecar hash mismatch")
    if result.get("schema") != RESULT_SCHEMA:
        raise RuntimeError(f"unexpected result schema: {result.get('schema')!r}")
    if result.get("summary") != {
        "actions": 435,
        "hashed_files": 435,
        "hash_errors": 0,
        "hashed_bytes": 123181662082,
    }:
        raise RuntimeError(f"unexpected artifact hash summary: {result.get('summary')}")
    source_plan = result.get("source_plan") or {}
    plan_path = Path(source_plan.get("path", ""))
    if not plan_path.is_file():
        raise RuntimeError(f"artifact source plan missing: {plan_path}")
    plan_sha = sha256_file(plan_path)
    if plan_sha != str(source_plan.get("sha256", "")).upper():
        raise RuntimeError("artifact source plan hash mismatch")
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    if plan.get("schema") != PLAN_SCHEMA:
        raise RuntimeError(f"unexpected artifact plan schema: {plan.get('schema')!r}")
    plan_actions = plan.get("actions")
    actions = result.get("actions")
    if not isinstance(actions, list) or not isinstance(plan_actions, list):
        raise RuntimeError("artifact actions are missing")
    if len(actions) != 435 or len(plan_actions) != 435:
        raise RuntimeError("artifact action coverage mismatch")
    plan_by_id = {row["action_id"]: row for row in plan_actions}
    if len(plan_by_id) != len(plan_actions):
        raise RuntimeError("duplicate artifact plan action ids")
    ids: set[str] = set()
    sources: set[str] = set()
    destinations: set[str] = set()
    quarantine_root = normalize_path(ntpath.normpath(plan["quarantine_root"]))
    for action in actions:
        action_id = action["action_id"]
        source = normalize_path(action["source_path"])
        destination = normalize_path(action["quarantine_path"])
        canonical_source = normalize_path(ntpath.normpath(action["source_path"]))
        canonical_destination = normalize_path(
            ntpath.normpath(action["quarantine_path"])
        )
        if action_id in ids or source in sources or destination in destinations:
            raise RuntimeError(f"duplicate artifact result key: {action_id}")
        ids.add(action_id)
        sources.add(source)
        destinations.add(destination)
        if action.get("normalized_source_path") != source:
            raise RuntimeError(f"artifact normalized path mismatch: {action_id}")
        if source != canonical_source or destination != canonical_destination:
            raise RuntimeError(f"artifact path traversal or alias: {action_id}")
        if destination == source or not destination.startswith(quarantine_root + "\\"):
            raise RuntimeError(f"artifact destination is unsafe: {action_id}")
        if source.startswith(quarantine_root + "\\"):
            raise RuntimeError(f"artifact source is already quarantined: {action_id}")
        original = plan_by_id.get(action_id)
        if original is None:
            raise RuntimeError(f"artifact action absent from source plan: {action_id}")
        for key, value in original.items():
            if key in {"full_sha256", "state"}:
                continue
            if action.get(key) != value:
                raise RuntimeError(f"artifact result drifted from plan: {action_id}/{key}")
        digest = str(action.get("full_sha256", ""))
        if (
            action.get("state") != "full_hash_verified"
            or action.get("hash_status") != "ok"
            or len(digest) != 64
            or any(ch not in "0123456789ABCDEF" for ch in digest)
        ):
            raise RuntimeError(f"artifact action is not hash-ready: {action_id}")
    if ids != set(plan_by_id):
        raise RuntimeError("artifact result does not cover the complete plan")
    return result_sha, plan


def validate_backup(
    backup: Path,
    live_conn: sqlite3.Connection,
    before: dict[str, Any],
) -> tuple[str, dict[str, str]]:
    if not backup.is_file():
        raise RuntimeError(f"backup does not exist: {backup}")
    backup_sha = sha256_file(backup)
    conn = sqlite3.connect(f"file:{backup.as_posix()}?mode=ro", uri=True)
    try:
        state = db_state(conn)
        backup_digests = canonical_row_digests(conn)
    finally:
        conn.close()
    if state != before:
        raise RuntimeError(f"backup preimage mismatch: {state} != {before}")
    live_digests = canonical_row_digests(live_conn)
    if backup_digests != live_digests:
        mismatch = sorted(
            table
            for table in BACKUP_TABLES
            if backup_digests.get(table) != live_digests.get(table)
        )
        raise RuntimeError(f"backup row-digest preimage mismatch: {mismatch}")
    return backup_sha, backup_digests


def validate_resume_backup(
    backup: Path,
    live_conn: sqlite3.Connection,
    mutable_tables: set[str],
) -> tuple[str, dict[str, str]]:
    if not backup.is_file():
        raise RuntimeError(f"backup does not exist: {backup}")
    backup_sha = sha256_file(backup)
    conn = sqlite3.connect(f"file:{backup.as_posix()}?mode=ro", uri=True)
    try:
        if conn.execute("PRAGMA quick_check").fetchone()[0] != "ok":
            raise RuntimeError("resume backup integrity failed")
        if conn.execute("PRAGMA foreign_key_check").fetchall():
            raise RuntimeError("resume backup has foreign-key violations")
        backup_digests = canonical_row_digests(conn)
    finally:
        conn.close()
    live_digests = canonical_row_digests(live_conn)
    mismatches = sorted(
        table
        for table in BACKUP_TABLES
        if table not in mutable_tables
        and backup_digests.get(table) != live_digests.get(table)
    )
    if mismatches:
        raise RuntimeError(f"resume changed an unaffected table: {mismatches}")
    return backup_sha, backup_digests


def initialize_ledger(
    ledger_path: Path,
    actions: list[dict[str, Any]],
    result_sha: str,
    backup_sha: str,
    backup_row_digests: dict[str, str] | None,
) -> sqlite3.Connection:
    ledger_path.parent.mkdir(parents=True, exist_ok=True)
    new = not ledger_path.exists()
    ledger = sqlite3.connect(ledger_path)
    ledger.execute("PRAGMA journal_mode=WAL")
    ledger.execute("PRAGMA synchronous=FULL")
    ledger.executescript(
        """
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS action (
  action_id TEXT PRIMARY KEY,
  ordinal INTEGER NOT NULL,
  status TEXT NOT NULL,
  error TEXT,
  updated_at_ms INTEGER NOT NULL
);
"""
    )
    if new:
        ledger.executemany(
            "INSERT INTO meta (key, value) VALUES (?, ?)",
            [
                ("schema", RECEIPT_SCHEMA),
                ("result_sha256", result_sha),
                ("backup_sha256", backup_sha),
                (
                    "backup_row_digests",
                    json.dumps(backup_row_digests or {}, sort_keys=True),
                ),
            ],
        )
        now_ms = int(time.time() * 1000)
        ledger.executemany(
            "INSERT INTO action VALUES (?, ?, 'planned', NULL, ?)",
            [
                (row["action_id"], ordinal, now_ms)
                for ordinal, row in enumerate(actions, 1)
            ],
        )
        ledger.commit()
    else:
        meta = dict(ledger.execute("SELECT key, value FROM meta"))
        if meta.get("result_sha256") != result_sha:
            raise RuntimeError("artifact ledger belongs to another result")
        if meta.get("backup_sha256") != backup_sha:
            raise RuntimeError("artifact ledger backup hash changed")
        ids = {row[0] for row in ledger.execute("SELECT action_id FROM action")}
        if ids != {row["action_id"] for row in actions}:
            raise RuntimeError("artifact ledger action coverage mismatch")
    return ledger


def mark(
    ledger: sqlite3.Connection, action_id: str, status: str, error: str | None = None
) -> None:
    ledger.execute(
        "UPDATE action SET status=?, error=?, updated_at_ms=? WHERE action_id=?",
        (status, error, int(time.time() * 1000), action_id),
    )
    ledger.commit()


def update_evidence(
    conn: sqlite3.Connection,
    action: dict[str, Any],
    result_sha: str,
    target_state: str,
) -> None:
    conn.execute("BEGIN IMMEDIATE")
    try:
        assert_idle_queue(conn)
        rows = conn.execute(
            """
SELECT id, match_state, details_json
FROM media_import_evidence
WHERE evidence_kind='wp0277_cleanup_artifact' AND source_record_key=?
""",
            (action["normalized_source_path"],),
        ).fetchall()
        if len(rows) != 1:
            raise RuntimeError(
                f"artifact evidence coverage mismatch for {action['action_id']}: {len(rows)}"
            )
        evidence_id, current_state, details_json = rows[0]
        allowed = (
            {"quarantine_pending", "quarantined"}
            if target_state == "quarantined"
            else {"quarantined", "quarantine_pending"}
        )
        if current_state not in allowed:
            raise RuntimeError(
                f"artifact evidence state drifted: {action['action_id']}={current_state}"
            )
        details = json.loads(details_json)
        evidence_preimage = {
            "artifact_reason": action["artifact_reason"],
            "evidence_size_bytes": int(action["evidence_size_bytes"]),
            "evidence_modified_ns": int(action["evidence_modified_ns"]),
            "observed_size_bytes": int(action["observed_size_bytes"]),
            "observed_modified_ns": int(action["observed_modified_ns"]),
        }
        mismatches = {
            key: {"database": details.get(key), "result": value}
            for key, value in evidence_preimage.items()
            if details.get(key) != value
        }
        if mismatches:
            raise RuntimeError(
                f"artifact evidence preimage drifted: "
                f"{action['action_id']}={mismatches}"
            )
        details.update(
            {
                "artifact_hash_result_sha256": result_sha,
                "full_sha256": action["full_sha256"],
                "quarantine_path": action["quarantine_path"],
                "quarantine_state": target_state,
            }
        )
        changed = conn.execute(
            "UPDATE media_import_evidence SET match_state=?, details_json=?, "
            "updated_at_ms=? WHERE id=? AND match_state=?",
            (
                target_state,
                json.dumps(details, sort_keys=True),
                int(time.time() * 1000),
                evidence_id,
                current_state,
            ),
        ).rowcount
        if changed != 1:
            raise RuntimeError(f"artifact evidence changed: {action['action_id']}")
        conn.commit()
    except Exception:
        conn.rollback()
        raise


def process_action(
    conn: sqlite3.Connection,
    ledger: sqlite3.Connection,
    action: dict[str, Any],
    result_sha: str,
    rollback: bool,
    prior_status: str,
) -> None:
    source = Path(action["source_path"])
    destination = Path(action["quarantine_path"])
    expected_size = int(action["observed_size_bytes"])
    digest = action["full_sha256"]
    source_exists = source.exists()
    destination_exists = destination.exists()
    if source_exists and destination_exists:
        allowed = (
            {"rolling_back", "attention", "failed"}
            if rollback
            else {"moving", "attention", "failed"}
        )
        if prior_status not in allowed:
            raise RuntimeError(f"both source and quarantine exist: {source}")
        verify_path(source, expected_size, digest)
        verify_path(destination, expected_size, digest)
        if rollback:
            mark(ledger, action["action_id"], "rolling_back")
            destination.unlink()
            destination_exists = False
        else:
            stat = source.stat()
            if stat.st_mtime_ns != int(action["observed_modified_ns"]):
                raise RuntimeError(
                    f"artifact source mtime changed in resume state: {source}"
                )
            mark(ledger, action["action_id"], "moving")
            source.unlink()
            source_exists = False
    if not source_exists and not destination_exists:
        raise RuntimeError(f"neither source nor quarantine exists: {source}")
    moved_now = False
    if rollback:
        if destination_exists:
            verify_path(destination, expected_size, digest)
            mark(ledger, action["action_id"], "rolling_back")
            move_verified(destination, source, expected_size, digest)
            moved_now = True
        else:
            verify_path(source, expected_size, digest)
        try:
            update_evidence(
                conn, action, result_sha, target_state="quarantine_pending"
            )
        except Exception:
            if moved_now and source.exists() and not destination.exists():
                move_verified(source, destination, expected_size, digest)
            raise
        mark(ledger, action["action_id"], "rolled_back")
    else:
        if source_exists:
            stat = source.stat()
            if stat.st_mtime_ns != int(action["observed_modified_ns"]):
                raise RuntimeError(f"artifact source mtime changed: {source}")
            verify_path(source, expected_size, digest)
            mark(ledger, action["action_id"], "moving")
            move_verified(source, destination, expected_size, digest)
            moved_now = True
        else:
            verify_path(destination, expected_size, digest)
        try:
            update_evidence(conn, action, result_sha, target_state="quarantined")
        except Exception:
            if moved_now and destination.exists() and not source.exists():
                move_verified(destination, source, expected_size, digest)
            raise
        mark(ledger, action["action_id"], "applied")


def main() -> int:
    args = parse_args()
    result_path = Path(args.result)
    database_path = Path(args.database)
    backup_path = Path(args.backup)
    ledger_path = Path(args.ledger)
    receipt_path = Path(args.receipt)
    run_lock = acquire_run_lock(
        database_path.with_name("wp0277_mutation.run.lock")
    )
    result = json.loads(result_path.read_text(encoding="utf-8"))
    result_sha, _source_plan = validate_result(
        result,
        result_path,
        args.expected_result_sha256,
    )
    conn = sqlite3.connect(database_path)
    conn.execute("PRAGMA foreign_keys=ON")
    conn.execute("PRAGMA busy_timeout=30000")
    before = db_state(conn)
    if before["quick_check"] != "ok" or before["foreign_key_violations"] != 0:
        raise RuntimeError(f"database integrity precondition failed: {before}")
    assert_idle_queue(conn)
    if ledger_path.exists():
        probe_ledger = sqlite3.connect(ledger_path)
        try:
            existing_statuses = {
                status: count
                for status, count in probe_ledger.execute(
                    "SELECT status, COUNT(*) FROM action GROUP BY status"
                )
            }
        finally:
            probe_ledger.close()
        if set(existing_statuses).issubset({"planned"}):
            backup_sha, backup_row_digests = validate_backup(
                backup_path, conn, before
            )
        else:
            backup_sha, backup_row_digests = validate_resume_backup(
                backup_path,
                conn,
                {"media_import_evidence"},
            )
    else:
        backup_sha, backup_row_digests = validate_backup(
            backup_path, conn, before
        )
    ledger = initialize_ledger(
        ledger_path,
        result["actions"],
        result_sha,
        backup_sha,
        backup_row_digests,
    )
    actions = list(reversed(result["actions"])) if args.rollback else result["actions"]
    processed = 0
    failed = 0
    if args.apply or args.rollback:
        for action in actions:
            status = ledger.execute(
                "SELECT status FROM action WHERE action_id=?",
                (action["action_id"],),
            ).fetchone()[0]
            if args.apply and status == "applied":
                continue
            if args.rollback and status == "rolled_back":
                continue
            try:
                process_action(
                    conn,
                    ledger,
                    action,
                    result_sha,
                    rollback=args.rollback,
                    prior_status=status,
                )
                processed += 1
            except Exception as error:
                error_detail = f"{type(error).__name__}: {error}"
                mark(ledger, action["action_id"], "attention", error_detail)
                failed += 1
                break
    after = db_state(conn)
    statuses = {
        status: count
        for status, count in ledger.execute(
            "SELECT status, COUNT(*) FROM action GROUP BY status"
        )
    }
    conn.close()
    ledger.close()
    receipt = {
        "schema": RECEIPT_SCHEMA,
        "work_packet": "WP-0277",
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "mode": (
            "apply" if args.apply else "rollback" if args.rollback else "dry_run"
        ),
        "result": {"path": str(result_path), "sha256": result_sha},
        "database": str(database_path),
        "backup": {"path": str(backup_path), "sha256": backup_sha},
        "ledger": str(ledger_path),
        "processed_this_run": processed,
        "failed_this_run": failed,
        "action_statuses": statuses,
        "before": before,
        "after": after,
    }
    receipt_path.parent.mkdir(parents=True, exist_ok=True)
    receipt_path.write_text(
        json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    receipt_path.with_suffix(receipt_path.suffix + ".sha256").write_text(
        f"{sha256_file(receipt_path)}  {receipt_path.name}\n", encoding="utf-8"
    )
    print(json.dumps(receipt, sort_keys=True))
    run_lock.close()
    return 0 if failed == 0 else 2


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"ERROR: {type(error).__name__}: {error}", file=sys.stderr)
        raise

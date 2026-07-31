#!/usr/bin/env python3
"""Permanently purge WP-0277 exact-duplicate quarantine files."""

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


MANIFEST_SCHEMA = "voxvulgi.wp0277.duplicate_quarantine_manifest.v1"
AUDIT_SCHEMA = "voxvulgi.wp0277.final_cleanup_audit.v1"
RECEIPT_SCHEMA = "voxvulgi.wp0277.duplicate_quarantine_purge.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--expected-manifest-sha256", required=True)
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


def is_regular_file(path: Path) -> tuple[os.stat_result | None, str | None]:
    try:
        observed = path.stat()
    except OSError as error:
        return None, f"{type(error).__name__}:{error}"
    if not statlib.S_ISREG(observed.st_mode):
        return None, "not_regular_file"
    return observed, None


def read_quarantine_ledger(path: Path) -> dict[str, int]:
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


def write_receipt(path: Path, value: dict[str, Any]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    encoded = (
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    ).encode("utf-8")
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_bytes(encoded)
    os.replace(temporary, path)
    digest = hashlib.sha256(encoded).hexdigest().upper()
    sidecar = path.with_suffix(path.suffix + ".sha256")
    sidecar.write_text(f"{digest}  {path.name}\n", encoding="utf-8")
    return digest


def main() -> int:
    args = parse_args()
    manifest_path = Path(args.manifest)
    audit_path = Path(args.final_audit)
    database_path = Path(args.database)
    quarantine_ledger_path = Path(args.quarantine_ledger)
    purge_ledger_path = Path(args.purge_ledger)
    receipt_path = Path(args.receipt)
    quarantine_root = args.quarantine_root
    manifest, manifest_sha = verify_pinned_json(
        manifest_path,
        args.expected_manifest_sha256,
        MANIFEST_SCHEMA,
    )
    audit, audit_sha = verify_pinned_json(
        audit_path,
        args.expected_final_audit_sha256,
        AUDIT_SCHEMA,
    )
    actions = manifest.get("actions", [])
    if len(actions) != 1826:
        raise RuntimeError(f"unexpected duplicate action count: {len(actions)}")
    if not audit.get("summary", {}).get("assertions_passed"):
        raise RuntimeError("final audit did not pass")
    if audit.get("summary", {}).get("duplicate_quarantine_actions") != len(actions):
        raise RuntimeError("final audit duplicate count does not match manifest")
    required_audit_assertions = (
        "all_duplicate_actions_applied",
        "all_duplicate_destinations_present_size_and_hash_matched",
        "all_duplicate_keepers_present_size_and_hash_matched",
        "all_duplicate_sources_absent",
        "database_quick_check_ok",
        "queue_remains_paused",
        "zero_foreign_key_violations",
        "zero_running_direct_jobs",
    )
    for name in required_audit_assertions:
        if audit.get("assertions", {}).get(name) is not True:
            raise RuntimeError(f"required final-audit assertion is not true: {name}")
    quarantine_status = read_quarantine_ledger(quarantine_ledger_path)
    if quarantine_status != {"applied": len(actions)}:
        raise RuntimeError(
            f"quarantine ledger is not fully applied: {quarantine_status}"
        )

    action_ids: set[str] = set()
    quarantine_paths: set[str] = set()
    keeper_expectations: dict[str, dict[str, Any]] = {}
    expected_bytes = 0
    for action in actions:
        action_id = action["action_id"]
        if action_id in action_ids:
            raise RuntimeError(f"duplicate action id: {action_id}")
        action_ids.add(action_id)
        quarantine_path = action["quarantine_path"]
        require_descendant(quarantine_path, quarantine_root)
        quarantine_key = normalize_windows_path(quarantine_path)
        if quarantine_key in quarantine_paths:
            raise RuntimeError(f"duplicate quarantine path: {quarantine_path}")
        quarantine_paths.add(quarantine_key)
        expected_bytes += int(action["size_bytes"])
        keeper_key = normalize_windows_path(action["keeper_path"])
        expectation = {
            "path": action["keeper_path"],
            "library_item_id": action["keeper_library_item_id"],
            "size_bytes": int(action["size_bytes"]),
            "modified_ns": int(action["keeper_modified_ns"]),
            "full_sha256": action["full_sha256"].upper(),
        }
        existing = keeper_expectations.setdefault(keeper_key, expectation)
        if existing != expectation:
            raise RuntimeError(
                f"conflicting keeper expectations: {action['keeper_path']}"
            )

    db = sqlite3.connect(f"file:{database_path.as_posix()}?mode=ro", uri=True)
    try:
        if db.execute("PRAGMA quick_check").fetchone()[0] != "ok":
            raise RuntimeError("database quick_check failed")
        if db.execute("PRAGMA foreign_key_check").fetchall():
            raise RuntimeError("database foreign-key violations exist")
        paused = db.execute(
            "SELECT value FROM meta WHERE key='jobs_queue_paused'"
        ).fetchone()
        if paused is None or paused[0] != "1":
            raise RuntimeError("queue is not paused")
        running = db.execute(
            "SELECT COUNT(*) FROM job WHERE status='running'"
        ).fetchone()[0]
        if running != 0:
            raise RuntimeError(f"running jobs exist: {running}")
        library_paths = {
            item_id: normalize_windows_path(media_path)
            for item_id, media_path in db.execute(
                "SELECT id, media_path FROM library_item"
            )
        }
    finally:
        db.close()

    for index, expectation in enumerate(keeper_expectations.values(), start=1):
        stored = library_paths.get(expectation["library_item_id"])
        if stored != normalize_windows_path(expectation["path"]):
            raise RuntimeError(
                "keeper library row missing or changed: "
                f"{expectation['library_item_id']}"
            )
        observed, error = is_regular_file(Path(expectation["path"]))
        if error:
            raise RuntimeError(f"keeper unavailable: {expectation['path']}: {error}")
        assert observed is not None
        if observed.st_size != expectation["size_bytes"]:
            raise RuntimeError(f"keeper size changed: {expectation['path']}")
        if observed.st_mtime_ns != expectation["modified_ns"]:
            raise RuntimeError(f"keeper timestamp changed: {expectation['path']}")
        if index % 100 == 0:
            print(
                f"keeper_preflight_progress={index}/{len(keeper_expectations)}",
                flush=True,
            )

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
                    int(action["size_bytes"]),
                    now,
                ),
            )
        purge.commit()
        ledger_ids = {
            row[0] for row in purge.execute("SELECT action_id FROM action")
        }
        if ledger_ids != action_ids:
            raise RuntimeError("purge ledger action set does not match manifest")

        preflight_present = 0
        preflight_bytes = 0
        for index, action in enumerate(actions, start=1):
            state = purge.execute(
                "SELECT status FROM action WHERE action_id=?",
                (action["action_id"],),
            ).fetchone()[0]
            path = Path(action["quarantine_path"])
            if state == "deleted":
                if path.exists():
                    raise RuntimeError(
                        f"ledger says deleted but path exists: {path}"
                    )
                continue
            if state == "deleting" and not path.exists():
                purge.execute(
                    "UPDATE action SET status='deleted', error=NULL, "
                    "updated_at_ms=? WHERE action_id=?",
                    (int(time.time() * 1000), action["action_id"]),
                )
                purge.commit()
                continue
            observed, error = is_regular_file(path)
            if error:
                raise RuntimeError(f"quarantine file unavailable: {path}: {error}")
            assert observed is not None
            if observed.st_size != int(action["size_bytes"]):
                raise RuntimeError(f"quarantine size changed: {path}")
            if observed.st_mtime_ns != int(action["source_modified_ns"]):
                raise RuntimeError(f"quarantine timestamp changed: {path}")
            if Path(action["source_path"]).exists():
                raise RuntimeError(
                    f"original duplicate source unexpectedly exists: "
                    f"{action['source_path']}"
                )
            preflight_present += 1
            preflight_bytes += observed.st_size
            if index % 100 == 0:
                print(
                    f"quarantine_preflight_progress={index}/{len(actions)}",
                    flush=True,
                )

        if not args.apply:
            print(
                json.dumps(
                    {
                        "apply": False,
                        "actions": len(actions),
                        "bytes": expected_bytes,
                        "keepers": len(keeper_expectations),
                        "present_to_purge": preflight_present,
                        "present_bytes": preflight_bytes,
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
            path = Path(action["quarantine_path"])
            if state == "deleted":
                continue
            keeper, keeper_error = is_regular_file(Path(action["keeper_path"]))
            if keeper_error or keeper is None:
                raise RuntimeError(
                    f"keeper unavailable at deletion boundary: "
                    f"{action['keeper_path']}: {keeper_error}"
                )
            if keeper.st_size != int(action["size_bytes"]):
                raise RuntimeError(
                    f"keeper size changed at deletion boundary: "
                    f"{action['keeper_path']}"
                )
            purge.execute(
                "UPDATE action SET status='deleting', error=NULL, updated_at_ms=? "
                "WHERE action_id=?",
                (int(time.time() * 1000), action["action_id"]),
            )
            purge.commit()
            try:
                path.unlink()
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
            if path.exists():
                raise RuntimeError(f"purged path still exists: {path}")
            purge.execute(
                "UPDATE action SET status='deleted', error=NULL, updated_at_ms=? "
                "WHERE action_id=?",
                (int(time.time() * 1000), action["action_id"]),
            )
            purge.commit()
            if index % 50 == 0:
                print(f"purge_progress={index}/{len(actions)}", flush=True)

        statuses = {
            status: count
            for status, count in purge.execute(
                "SELECT status, COUNT(*) FROM action GROUP BY status"
            )
        }
        if statuses != {"deleted": len(actions)}:
            raise RuntimeError(f"purge did not finish cleanly: {statuses}")
    finally:
        purge.close()

    db = sqlite3.connect(f"file:{database_path.as_posix()}?mode=ro", uri=True)
    try:
        final_paused = db.execute(
            "SELECT value FROM meta WHERE key='jobs_queue_paused'"
        ).fetchone()[0]
        final_running = db.execute(
            "SELECT COUNT(*) FROM job WHERE status='running'"
        ).fetchone()[0]
        final_quick_check = db.execute("PRAGMA quick_check").fetchone()[0]
        final_foreign_keys = len(db.execute("PRAGMA foreign_key_check").fetchall())
    finally:
        db.close()
    if final_paused != "1" or final_running != 0:
        raise RuntimeError("queue state changed during purge")
    if final_quick_check != "ok" or final_foreign_keys != 0:
        raise RuntimeError("database validation failed after purge")
    remaining = [
        action["quarantine_path"]
        for action in actions
        if Path(action["quarantine_path"]).exists()
    ]
    if remaining:
        raise RuntimeError(f"purged paths remain: {len(remaining)}")

    receipt = {
        "schema": RECEIPT_SCHEMA,
        "work_packet": "WP-0277",
        "started_at": started_at,
        "completed_at": dt.datetime.now().astimezone().isoformat(),
        "manifest": {
            "path": str(manifest_path),
            "sha256": manifest_sha,
        },
        "final_audit": {
            "path": str(audit_path),
            "sha256": audit_sha,
        },
        "quarantine_root": quarantine_root,
        "purge_ledger": str(purge_ledger_path),
        "deleted_files": len(actions),
        "deleted_bytes": expected_bytes,
        "keeper_files_preserved": len(keeper_expectations),
        "remaining_manifest_paths": 0,
        "queue_paused": True,
        "running_jobs": 0,
        "database_quick_check": final_quick_check,
        "foreign_key_violations": final_foreign_keys,
        "status": "complete",
    }
    digest = write_receipt(receipt_path, receipt)
    print(
        json.dumps(
            {
                "status": "complete",
                "deleted_files": len(actions),
                "deleted_bytes": expected_bytes,
                "receipt": str(receipt_path),
                "receipt_sha256": digest,
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

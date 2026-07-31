#!/usr/bin/env python3
"""Apply or roll back a WP-0277 duplicate quarantine manifest recoverably."""

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
import uuid
from typing import Any


MANIFEST_SCHEMA = "voxvulgi.wp0277.duplicate_quarantine_manifest.v1"
RECEIPT_SCHEMA = "voxvulgi.wp0277.duplicate_quarantine_apply.v1"
ID_NAMESPACE = uuid.UUID("2317c683-3228-48ee-a0b6-a22b5c36ff20")
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
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--database", required=True)
    parser.add_argument("--backup", required=True)
    parser.add_argument("--ledger", required=True)
    parser.add_argument("--receipt", required=True)
    parser.add_argument("--expected-manifest-sha256", required=True)
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


def deterministic_id(kind: str, key: str) -> str:
    return str(uuid.uuid5(ID_NAMESPACE, f"{kind}:{key}"))


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
            raise RuntimeError(f"another quarantine apply owns {path}") from error
    except OSError as error:
        handle.close()
        raise RuntimeError(f"another quarantine apply owns {path}") from error
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
        "canceled_direct_jobs": conn.execute(
            "SELECT COUNT(*) FROM job "
            "WHERE type='download_direct_url' AND status='canceled'"
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
        raise RuntimeError("duplicate quarantine requires paused idle queue")


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


def initialize_ledger(
    ledger_path: Path,
    manifest: dict[str, Any],
    manifest_sha: str,
    backup_path: Path,
    backup_sha: str,
    before: dict[str, Any],
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
        now_ms = int(time.time() * 1000)
        metadata = {
            "schema": RECEIPT_SCHEMA,
            "manifest_sha256": manifest_sha,
            "backup_path": str(backup_path),
            "backup_sha256": backup_sha,
            "database_preimage": json.dumps(before, sort_keys=True),
            "backup_row_digests": json.dumps(
                backup_row_digests or {}, sort_keys=True
            ),
        }
        ledger.executemany(
            "INSERT INTO meta (key, value) VALUES (?, ?)", metadata.items()
        )
        ledger.executemany(
            "INSERT INTO action (action_id, ordinal, status, error, updated_at_ms) "
            "VALUES (?, ?, 'planned', NULL, ?)",
            [
                (action["action_id"], ordinal, now_ms)
                for ordinal, action in enumerate(manifest["actions"], 1)
            ],
        )
        ledger.commit()
    else:
        metadata = dict(ledger.execute("SELECT key, value FROM meta"))
        if metadata.get("manifest_sha256") != manifest_sha:
            raise RuntimeError("ledger belongs to a different manifest")
        if metadata.get("backup_sha256") != backup_sha:
            raise RuntimeError("ledger backup hash changed")
        action_ids = {
            row[0] for row in ledger.execute("SELECT action_id FROM action")
        }
        if action_ids != {row["action_id"] for row in manifest["actions"]}:
            raise RuntimeError("ledger action coverage mismatch")
    return ledger


def validate_manifest(
    manifest: dict[str, Any],
    manifest_sha: str,
    expected_sha: str,
    database_path: Path,
) -> None:
    if manifest.get("schema") != MANIFEST_SCHEMA:
        raise RuntimeError(f"unexpected manifest schema: {manifest.get('schema')!r}")
    if manifest_sha != expected_sha.strip().upper():
        raise RuntimeError(
            f"manifest hash mismatch: {manifest_sha} != {expected_sha.strip().upper()}"
        )
    if manifest.get("state") != "planned_not_applied":
        raise RuntimeError(f"manifest is not apply-ready: {manifest.get('state')!r}")
    if normalize_path(manifest.get("database", "")) != normalize_path(
        str(database_path)
    ):
        raise RuntimeError("manifest database does not match requested database")
    actions = manifest.get("actions")
    groups = manifest.get("groups")
    if not isinstance(actions, list) or not isinstance(groups, list):
        raise RuntimeError("manifest groups/actions are missing")
    summary = manifest.get("summary") or {}
    if (
        summary.get("exact_duplicate_groups") != len(groups)
        or summary.get("redundant_files") != len(actions)
        or summary.get("reclaimable_bytes")
        != sum(int(row["size_bytes"]) for row in actions)
    ):
        raise RuntimeError("manifest summary does not match actions/groups")
    action_ids: set[str] = set()
    sources: set[str] = set()
    destinations: set[str] = set()
    quarantine_root = normalize_path(ntpath.normpath(manifest["quarantine_root"]))
    for action in actions:
        action_id = action["action_id"]
        source = normalize_path(action["source_path"])
        keeper = normalize_path(action["keeper_path"])
        destination = normalize_path(action["quarantine_path"])
        canonical_source = normalize_path(ntpath.normpath(action["source_path"]))
        canonical_keeper = normalize_path(ntpath.normpath(action["keeper_path"]))
        canonical_destination = normalize_path(
            ntpath.normpath(action["quarantine_path"])
        )
        if action_id in action_ids or source in sources or destination in destinations:
            raise RuntimeError(f"duplicate manifest action key: {action_id}")
        action_ids.add(action_id)
        sources.add(source)
        destinations.add(destination)
        if action["source_normalized_path"] != source:
            raise RuntimeError(f"source normalized path mismatch: {action_id}")
        if action["keeper_normalized_path"] != keeper:
            raise RuntimeError(f"keeper normalized path mismatch: {action_id}")
        if (
            source != canonical_source
            or keeper != canonical_keeper
            or destination != canonical_destination
        ):
            raise RuntimeError(f"manifest path traversal or alias: {action_id}")
        if source == keeper or source == destination or keeper == destination:
            raise RuntimeError(f"source/keeper/destination alias: {action_id}")
        if not destination.startswith(quarantine_root + "\\"):
            raise RuntimeError(f"destination escapes quarantine root: {action_id}")
        if source.startswith(quarantine_root + "\\"):
            raise RuntimeError(f"source is already under quarantine: {action_id}")
        if int(action["size_bytes"]) <= 0:
            raise RuntimeError(f"non-positive action size: {action_id}")
        digest = action["full_sha256"]
        if len(digest) != 64 or any(ch not in "0123456789ABCDEF" for ch in digest):
            raise RuntimeError(f"invalid action hash: {action_id}")
        preimages = action["source_library_preimages"]
        if not preimages or len({row["library_item_id"] for row in preimages}) != len(
            preimages
        ):
            raise RuntimeError(f"invalid source library preimages: {action_id}")
        if Path(action["source_path"]).exists() and Path(action["keeper_path"]).exists():
            if os.path.samefile(action["source_path"], action["keeper_path"]):
                raise RuntimeError(f"source and keeper are the same file: {action_id}")


def validate_backup(
    backup: Path,
    live_conn: sqlite3.Connection,
    live_before: dict[str, Any],
) -> tuple[str, dict[str, Any], dict[str, str]]:
    if not backup.is_file():
        raise RuntimeError(f"backup does not exist: {backup}")
    backup_sha = sha256_file(backup)
    conn = sqlite3.connect(f"file:{backup.as_posix()}?mode=ro", uri=True)
    try:
        backup_state = db_state(conn)
        backup_digests = canonical_row_digests(conn)
    finally:
        conn.close()
    if backup_state != live_before:
        raise RuntimeError(f"backup preimage mismatch: {backup_state} != {live_before}")
    live_digests = canonical_row_digests(live_conn)
    if backup_digests != live_digests:
        mismatch = sorted(
            table
            for table in BACKUP_TABLES
            if backup_digests.get(table) != live_digests.get(table)
        )
        raise RuntimeError(f"backup row-digest preimage mismatch: {mismatch}")
    return backup_sha, backup_state, backup_digests


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


def mark_ledger(
    ledger: sqlite3.Connection, action_id: str, status: str, error: str | None = None
) -> None:
    ledger.execute(
        "UPDATE action SET status=?, error=?, updated_at_ms=? WHERE action_id=?",
        (status, error, int(time.time() * 1000), action_id),
    )
    ledger.commit()


def apply_metadata(
    conn: sqlite3.Connection,
    action: dict[str, Any],
    manifest_sha: str,
) -> None:
    now_ms = int(time.time() * 1000)
    conn.execute("BEGIN IMMEDIATE")
    try:
        assert_idle_queue(conn)
        keeper_row = conn.execute(
            "SELECT media_path FROM library_item WHERE id=?",
            (action["keeper_library_item_id"],),
        ).fetchone()
        if (
            keeper_row is None
            or normalize_path(keeper_row[0])
            != normalize_path(action["keeper_stored_media_path"])
            or normalize_path(keeper_row[0])
            != normalize_path(action["keeper_path"])
        ):
            raise RuntimeError(
                f"keeper library row drifted: {action['keeper_library_item_id']}"
            )
        for preimage in action["source_library_preimages"]:
            current = conn.execute(
                "SELECT media_path FROM library_item WHERE id=?",
                (preimage["library_item_id"],),
            ).fetchone()
            if current is None:
                raise RuntimeError(
                    f"source library row missing: {preimage['library_item_id']}"
                )
            if normalize_path(current[0]) == normalize_path(preimage["media_path"]):
                changed = conn.execute(
                    "UPDATE library_item SET media_path=? WHERE id=? AND media_path=?",
                    (
                        action["keeper_stored_media_path"],
                        preimage["library_item_id"],
                        current[0],
                    ),
                ).rowcount
                if changed != 1:
                    raise RuntimeError(
                        f"source library row changed: {preimage['library_item_id']}"
                    )
            elif normalize_path(current[0]) != normalize_path(
                action["keeper_stored_media_path"]
            ):
                raise RuntimeError(
                    "source library row drifted outside original/keeper path: "
                    f"{preimage['library_item_id']}"
                )
        for preimage in action["identity_preimages"]:
            current = conn.execute(
                "SELECT library_item_id FROM media_source_identity "
                "WHERE service='youtube' AND media_id=?",
                (preimage["media_id"],),
            ).fetchone()
            if current is None:
                raise RuntimeError(f"source identity missing: {preimage['media_id']}")
            if current[0] == preimage["library_item_id"]:
                changed = conn.execute(
                    """
UPDATE media_source_identity
SET library_item_id=?, repair_state='ready', updated_at_ms=?
WHERE service='youtube' AND media_id=? AND library_item_id=?
""",
                    (
                        action["keeper_library_item_id"],
                        now_ms,
                        preimage["media_id"],
                        preimage["library_item_id"],
                    ),
                ).rowcount
                if changed != 1:
                    raise RuntimeError(f"source identity changed: {preimage['media_id']}")
            elif current[0] != action["keeper_library_item_id"]:
                raise RuntimeError(
                    f"source identity drifted: {preimage['media_id']} -> {current[0]}"
                )
        for preimage in action["source_library_preimages"]:
            evidence_id = deterministic_id(
                "duplicate-quarantine",
                f"{action['action_id']}:{preimage['library_item_id']}",
            )
            existing = conn.execute(
                "SELECT library_item_id, evidence_kind, source_record_key "
                "FROM media_import_evidence WHERE id=?",
                (evidence_id,),
            ).fetchone()
            details_json = json.dumps(
                {
                    "manifest_sha256": manifest_sha,
                    "quarantine_path": action["quarantine_path"],
                    "keeper_path": action["keeper_path"],
                    "full_sha256": action["full_sha256"],
                    "size_bytes": action["size_bytes"],
                },
                sort_keys=True,
            )
            if existing is None:
                conn.execute(
                    """
INSERT INTO media_import_evidence (
  id, library_item_id, service, media_id, evidence_kind,
  source_record_key, source_path_snapshot, source_url_snapshot,
  match_state, details_json, created_at_ms, updated_at_ms
) VALUES (?, ?, 'local', NULL, 'wp0277_duplicate_quarantine',
          ?, ?, NULL, 'quarantined', ?, ?, ?)
""",
                    (
                        evidence_id,
                        preimage["library_item_id"],
                        action["action_id"],
                        preimage["media_path"],
                        details_json,
                        now_ms,
                        now_ms,
                    ),
                )
            else:
                expected = (
                    preimage["library_item_id"],
                    "wp0277_duplicate_quarantine",
                    action["action_id"],
                )
                if existing != expected:
                    raise RuntimeError(f"cleanup evidence drifted: {evidence_id}")
                changed = conn.execute(
                    "UPDATE media_import_evidence SET match_state='quarantined', "
                    "details_json=?, updated_at_ms=? WHERE id=?",
                    (details_json, now_ms, evidence_id),
                ).rowcount
                if changed != 1:
                    raise RuntimeError(f"cleanup evidence changed: {evidence_id}")
        conn.commit()
    except Exception:
        conn.rollback()
        raise


def rollback_metadata(conn: sqlite3.Connection, action: dict[str, Any]) -> None:
    now_ms = int(time.time() * 1000)
    conn.execute("BEGIN IMMEDIATE")
    try:
        assert_idle_queue(conn)
        for preimage in action["source_library_preimages"]:
            current = conn.execute(
                "SELECT media_path FROM library_item WHERE id=?",
                (preimage["library_item_id"],),
            ).fetchone()
            if current is None:
                raise RuntimeError(
                    f"rollback source library row missing: {preimage['library_item_id']}"
                )
            if normalize_path(current[0]) == normalize_path(
                action["keeper_stored_media_path"]
            ):
                changed = conn.execute(
                    "UPDATE library_item SET media_path=? WHERE id=? AND media_path=?",
                    (preimage["media_path"], preimage["library_item_id"], current[0]),
                ).rowcount
                if changed != 1:
                    raise RuntimeError(
                        f"rollback library row changed: {preimage['library_item_id']}"
                    )
            elif normalize_path(current[0]) != normalize_path(preimage["media_path"]):
                raise RuntimeError(
                    f"rollback library row drifted: {preimage['library_item_id']}"
                )
        for preimage in action["identity_preimages"]:
            current = conn.execute(
                "SELECT library_item_id FROM media_source_identity "
                "WHERE service='youtube' AND media_id=?",
                (preimage["media_id"],),
            ).fetchone()
            if current is None:
                raise RuntimeError(
                    f"rollback source identity missing: {preimage['media_id']}"
                )
            if current[0] == action["keeper_library_item_id"]:
                changed = conn.execute(
                    "UPDATE media_source_identity SET library_item_id=?, updated_at_ms=? "
                    "WHERE service='youtube' AND media_id=? AND library_item_id=?",
                    (
                        preimage["library_item_id"],
                        now_ms,
                        preimage["media_id"],
                        action["keeper_library_item_id"],
                    ),
                ).rowcount
                if changed != 1:
                    raise RuntimeError(
                        f"rollback identity changed: {preimage['media_id']}"
                    )
            elif current[0] != preimage["library_item_id"]:
                raise RuntimeError(
                    f"rollback identity drifted: {preimage['media_id']}"
                )
        conn.execute(
            "UPDATE media_import_evidence SET match_state='rolled_back', "
            "updated_at_ms=? WHERE evidence_kind='wp0277_duplicate_quarantine' "
            "AND source_record_key=?",
            (now_ms, action["action_id"]),
        )
        conn.commit()
    except Exception:
        conn.rollback()
        raise


def apply_action(
    conn: sqlite3.Connection,
    ledger: sqlite3.Connection,
    action: dict[str, Any],
    manifest_sha: str,
    prior_status: str,
) -> str:
    source = Path(action["source_path"])
    destination = Path(action["quarantine_path"])
    keeper = Path(action["keeper_path"])
    source_exists = source.exists()
    destination_exists = destination.exists()
    if source_exists and destination_exists:
        if prior_status not in {"moving", "attention", "failed"}:
            raise RuntimeError(f"both source and quarantine exist: {source}")
        stat = source.stat()
        if stat.st_mtime_ns != action["source_modified_ns"]:
            raise RuntimeError(f"source mtime changed in copy-resume state: {source}")
        verify_path(source, action["size_bytes"], action["full_sha256"])
        verify_path(destination, action["size_bytes"], action["full_sha256"])
        source.unlink()
        source_exists = False
    if not source_exists and not destination_exists:
        raise RuntimeError(f"neither source nor quarantine exists: {source}")
    verify_path(keeper, action["size_bytes"], action["full_sha256"])
    moved_now = False
    if source_exists:
        stat = source.stat()
        if stat.st_mtime_ns != action["source_modified_ns"]:
            raise RuntimeError(f"source mtime changed: {source}")
        verify_path(source, action["size_bytes"], action["full_sha256"])
        mark_ledger(ledger, action["action_id"], "moving")
        move_verified(
            source, destination, action["size_bytes"], action["full_sha256"]
        )
        moved_now = True
    else:
        verify_path(destination, action["size_bytes"], action["full_sha256"])
    try:
        apply_metadata(conn, action, manifest_sha)
    except Exception as error:
        if moved_now and destination.exists() and not source.exists():
            move_verified(
                destination, source, action["size_bytes"], action["full_sha256"]
            )
        mark_ledger(ledger, action["action_id"], "failed", str(error))
        raise
    mark_ledger(ledger, action["action_id"], "applied")
    return "applied"


def rollback_action(
    conn: sqlite3.Connection,
    ledger: sqlite3.Connection,
    action: dict[str, Any],
    prior_status: str,
) -> str:
    source = Path(action["source_path"])
    destination = Path(action["quarantine_path"])
    source_exists = source.exists()
    destination_exists = destination.exists()
    if source_exists and destination_exists:
        if prior_status not in {"rolling_back", "attention", "failed"}:
            raise RuntimeError(f"rollback found both source and quarantine: {source}")
        verify_path(source, action["size_bytes"], action["full_sha256"])
        verify_path(destination, action["size_bytes"], action["full_sha256"])
        mark_ledger(ledger, action["action_id"], "rolling_back")
        destination.unlink()
        destination_exists = False
    if not source_exists and not destination_exists:
        raise RuntimeError(f"rollback found neither source nor quarantine: {source}")
    moved_now = False
    if destination_exists:
        verify_path(destination, action["size_bytes"], action["full_sha256"])
        mark_ledger(ledger, action["action_id"], "rolling_back")
        move_verified(
            destination, source, action["size_bytes"], action["full_sha256"]
        )
        moved_now = True
    else:
        verify_path(source, action["size_bytes"], action["full_sha256"])
    try:
        rollback_metadata(conn, action)
    except Exception as error:
        if moved_now and source.exists() and not destination.exists():
            move_verified(
                source, destination, action["size_bytes"], action["full_sha256"]
            )
        mark_ledger(ledger, action["action_id"], "attention", str(error))
        raise
    mark_ledger(ledger, action["action_id"], "rolled_back")
    return "rolled_back"


def main() -> int:
    args = parse_args()
    manifest_path = Path(args.manifest)
    database_path = Path(args.database)
    backup_path = Path(args.backup)
    ledger_path = Path(args.ledger)
    receipt_path = Path(args.receipt)
    run_lock = acquire_run_lock(
        database_path.with_name("wp0277_mutation.run.lock")
    )
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    manifest_sha = sha256_file(manifest_path)
    validate_manifest(
        manifest,
        manifest_sha,
        args.expected_manifest_sha256,
        database_path,
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
            backup_sha, backup_state, backup_row_digests = validate_backup(
                backup_path, conn, before
            )
        else:
            backup_sha, backup_row_digests = validate_resume_backup(
                backup_path,
                conn,
                {
                    "library_item",
                    "media_source_identity",
                    "media_import_evidence",
                },
            )
            backup_state = None
    else:
        backup_sha, backup_state, backup_row_digests = validate_backup(
            backup_path, conn, before
        )
    ledger = initialize_ledger(
        ledger_path,
        manifest,
        manifest_sha,
        backup_path,
        backup_sha,
        before if backup_state is None else backup_state,
        backup_row_digests,
    )
    selected_actions = (
        list(reversed(manifest["actions"])) if args.rollback else manifest["actions"]
    )
    processed = 0
    failed = 0
    if args.apply or args.rollback:
        for action in selected_actions:
            status = ledger.execute(
                "SELECT status FROM action WHERE action_id=?",
                (action["action_id"],),
            ).fetchone()[0]
            if args.apply and status == "applied":
                continue
            if args.rollback and status == "rolled_back":
                continue
            try:
                if args.rollback:
                    rollback_action(conn, ledger, action, status)
                else:
                    apply_action(conn, ledger, action, manifest_sha, status)
                processed += 1
            except Exception as error:
                mark_ledger(ledger, action["action_id"], "attention", str(error))
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
        "manifest": {"path": str(manifest_path), "sha256": manifest_sha},
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

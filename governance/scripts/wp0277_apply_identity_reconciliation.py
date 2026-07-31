#!/usr/bin/env python3
"""Resolve deferred multi-file YouTube identities after WP-0277 full hashing."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import sys
import time
import uuid
from typing import Any


CANDIDATE_SCHEMA = "voxvulgi.wp0277.reconciled_identity_candidate_manifest.v1"
HASH_SCHEMA = "voxvulgi.wp0277.reconciled_identity_hash_result.v1"
PATH_RECEIPT_SCHEMA = "voxvulgi.path_reconcile_apply.v1"
RECEIPT_SCHEMA = "voxvulgi.wp0277.identity_reconcile_apply.v1"
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
    parser.add_argument("--candidates", required=True)
    parser.add_argument("--hash-result", required=True)
    parser.add_argument("--path-reconcile-receipt", required=True)
    parser.add_argument("--expected-candidates-sha256", required=True)
    parser.add_argument("--expected-hash-result-sha256", required=True)
    parser.add_argument("--expected-path-reconcile-receipt-sha256", required=True)
    parser.add_argument("--database", required=True)
    parser.add_argument("--receipt", required=True)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--backup")
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
            raise RuntimeError(f"another WP-0277 mutation owns {path}") from error
    except OSError as error:
        handle.close()
        raise RuntimeError(f"another WP-0277 mutation owns {path}") from error
    return handle


def verify_pinned_file(path: Path, expected_sha256: str) -> str:
    actual = sha256_file(path)
    expected = expected_sha256.strip().upper()
    if actual != expected:
        raise RuntimeError(f"pinned hash mismatch for {path}: {actual} != {expected}")
    sidecar = path.with_suffix(path.suffix + ".sha256")
    if not sidecar.is_file():
        raise RuntimeError(f"hash sidecar missing: {sidecar}")
    sidecar_hash = sidecar.read_text(encoding="utf-8").split()[0].upper()
    if sidecar_hash != actual:
        raise RuntimeError(f"hash sidecar mismatch for {path}")
    return actual


def deterministic_id(kind: str, key: str) -> str:
    return str(uuid.uuid5(ID_NAMESPACE, f"{kind}:{key}"))


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


def load_path_reconcile_handoff(
    receipt_path: Path,
    database: Path,
    before: dict[str, Any],
    expected_sha256: str,
) -> tuple[dict[str, list[dict[str, Any]]], str]:
    if not receipt_path.is_file():
        raise RuntimeError(f"path reconciliation receipt does not exist: {receipt_path}")
    receipt_sha256 = verify_pinned_file(receipt_path, expected_sha256)
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    if receipt.get("schema") != PATH_RECEIPT_SCHEMA or receipt.get("mode") != "apply":
        raise RuntimeError("path reconciliation receipt is not a completed apply receipt")
    if normalize_path(receipt.get("database", "")) != normalize_path(str(database)):
        raise RuntimeError("path reconciliation receipt database mismatch")
    receipt_after = receipt.get("after", {})
    state_map = {
        "quick_check": "quick_check",
        "queue_paused": "queue_paused",
        "running_direct_jobs": "running_direct_jobs",
        "queued_direct_jobs": "queued_direct_jobs",
        "canceled_direct_jobs": "canceled_direct_jobs",
        "source_identities": "source_identities",
        "import_evidence": "import_evidence",
        "memberships": "memberships",
        "associations": "associations",
        "foreign_key_violations": "foreign_key_violations",
    }
    mismatches = {
        current_key: {
            "receipt": receipt_after.get(receipt_key),
            "current": before.get(current_key),
        }
        for receipt_key, current_key in state_map.items()
        if receipt_after.get(receipt_key) != before.get(current_key)
    }
    if mismatches:
        raise RuntimeError(
            "path reconciliation receipt does not match current database state: "
            f"{mismatches}"
        )
    plan = receipt.get("plan", {})
    if (
        plan.get("deferred_association_inserts") != 1
        or plan.get("deferred_membership_inserts") != 1
    ):
        raise RuntimeError("path reconciliation receipt deferred counts mismatch")
    handoff = receipt.get("deferred_source_context")
    if not isinstance(handoff, dict):
        raise RuntimeError("path reconciliation receipt lacks deferred source context")
    associations = handoff.get("associations")
    memberships = handoff.get("memberships")
    if (
        not isinstance(associations, list)
        or not isinstance(memberships, list)
        or len(associations) != 1
        or len(memberships) != 1
    ):
        raise RuntimeError("path reconciliation deferred source context mismatch")
    expected_media_id = "2-hvGJUuCYQ"
    if (
        associations[0].get("media_id") != expected_media_id
        or memberships[0].get("media_id") != expected_media_id
    ):
        raise RuntimeError("deferred source context belongs to an unexpected identity")
    return {
        "associations": associations,
        "memberships": memberships,
    }, receipt_sha256


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
        "source_identities": conn.execute(
            "SELECT COUNT(*) FROM media_source_identity"
        ).fetchone()[0],
        "source_aliases": conn.execute(
            "SELECT COUNT(*) FROM media_source_alias"
        ).fetchone()[0],
        "import_evidence": conn.execute(
            "SELECT COUNT(*) FROM media_import_evidence"
        ).fetchone()[0],
        "memberships": conn.execute(
            "SELECT COUNT(*) FROM media_source_membership"
        ).fetchone()[0],
        "associations": conn.execute(
            "SELECT COUNT(*) FROM media_source_association"
        ).fetchone()[0],
        "foreign_key_violations": len(
            conn.execute("PRAGMA foreign_key_check").fetchall()
        ),
    }


def verify_backup(
    backup: Path,
    live_conn: sqlite3.Connection,
    before: dict[str, Any],
) -> dict[str, Any]:
    if not backup.is_file():
        raise RuntimeError(f"backup does not exist: {backup}")
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
    return {
        "path": str(backup),
        "sha256": sha256_file(backup),
        "state": state,
        "row_digests": backup_digests,
    }


def library_rows_by_path(
    conn: sqlite3.Connection,
) -> dict[str, list[dict[str, Any]]]:
    rows: dict[str, list[dict[str, Any]]] = {}
    query = """
SELECT id, media_path, width, height, duration_ms, origin, source_type, created_at_ms
FROM library_item
"""
    for row in conn.execute(query):
        value = {
            "id": row[0],
            "media_path": row[1],
            "width": row[2],
            "height": row[3],
            "duration_ms": row[4],
            "origin": row[5],
            "source_type": row[6],
            "created_at_ms": row[7],
        }
        rows.setdefault(normalize_path(row[1]), []).append(value)
    return rows


def select_keeper(
    group: dict[str, Any],
    identity: sqlite3.Row | None,
    library_by_path: dict[str, list[dict[str, Any]]],
) -> dict[str, Any]:
    enriched = []
    for candidate in group["candidates"]:
        library_rows = library_by_path.get(candidate["normalized_path"], [])
        if not library_rows:
            raise RuntimeError(
                f"candidate has no reconciled library row: {candidate['path']}"
            )
        for library in library_rows:
            enriched.append({"candidate": candidate, "library": library})
    if identity is not None and identity["library_item_id"]:
        linked = [
            row
            for row in enriched
            if row["library"]["id"] == identity["library_item_id"]
        ]
        if not linked:
            raise RuntimeError(
                f"linked keeper is outside candidate set: youtube:{group['media_id']}"
            )
        return linked[0]

    def score(row: dict[str, Any]) -> tuple[Any, ...]:
        candidate = row["candidate"]
        library = row["library"]
        pixels = int(library["width"] or 0) * int(library["height"] or 0)
        job_output = "voxvulgi_job_output_suffix" in candidate["evidence_sources"]
        return (
            -pixels,
            -int(library["duration_ms"] or 0),
            -int(candidate["size_bytes"]),
            -int(job_output),
            0 if library["origin"] == "voxvulgi_download" else 1,
            candidate["normalized_path"].count("\\"),
            candidate["normalized_path"],
            library["id"],
        )

    return min(enriched, key=score)


def build_plan(
    conn: sqlite3.Connection,
    candidates: dict[str, Any],
    hash_result: dict[str, Any],
    candidate_sha256: str,
    deferred_source_context: dict[str, list[dict[str, Any]]],
    verify_files: bool,
) -> dict[str, Any]:
    if candidates.get("schema") != CANDIDATE_SCHEMA:
        raise RuntimeError(f"unexpected candidate schema: {candidates.get('schema')!r}")
    if hash_result.get("schema") != HASH_SCHEMA:
        raise RuntimeError(f"unexpected hash schema: {hash_result.get('schema')!r}")
    if hash_result["summary"].get("hash_errors") != 0:
        raise RuntimeError("identity hash result contains errors")
    if (
        hash_result.get("candidate_manifest", {}).get("sha256", "").upper()
        != candidate_sha256.upper()
    ):
        raise RuntimeError("identity hash result is not bound to this candidate manifest")
    result_groups = hash_result.get("groups")
    candidate_groups = candidates.get("groups")
    if not isinstance(result_groups, list) or not isinstance(candidate_groups, list):
        raise RuntimeError("identity candidate/hash groups are missing")
    result_ids = {row["group_id"] for row in result_groups}
    candidate_ids = {row["group_id"] for row in candidate_groups}
    if result_ids != candidate_ids:
        raise RuntimeError("identity candidate/hash group coverage mismatch")
    if (
        len(candidate_groups) != 1243
        or len(result_groups) != 1243
        or len(candidate_ids) != 1243
        or len(result_ids) != 1243
    ):
        raise RuntimeError(
            "unexpected identity group coverage: "
            f"{len(candidate_groups)}/{len(result_groups)}/"
            f"{len(candidate_ids)}/{len(result_ids)}"
        )
    summary = hash_result.get("summary", {})
    expected_summary = {
        "identity_groups": 1243,
        "hashed_same_size_groups": 567,
        "hashed_unique_files": 1272,
        "hashed_read_bytes": 226_968_149_694,
        "hash_errors": 0,
    }
    summary_mismatches = {
        key: {"actual": summary.get(key), "expected": expected}
        for key, expected in expected_summary.items()
        if summary.get(key) != expected
    }
    if summary_mismatches:
        raise RuntimeError(f"identity hash summary mismatch: {summary_mismatches}")
    candidates_by_id = {row["group_id"]: row for row in candidate_groups}
    computed_exact_sets = 0
    computed_exact_members = 0
    computed_redundant = 0
    computed_reclaimable = 0
    computed_size_split = 0
    computed_hash_split = 0
    for result_group in result_groups:
        candidate_group = candidates_by_id[result_group["group_id"]]
        if (
            result_group.get("media_id") != candidate_group.get("media_id")
            or result_group.get("identity_state")
            != candidate_group.get("identity_state")
        ):
            raise RuntimeError(
                f"identity hash group metadata mismatch: {result_group['group_id']}"
            )
        candidate_by_path = {
            row["normalized_path"]: row
            for row in candidate_group["candidates"]
        }
        exact_paths: set[str] = set()
        exact_sets = result_group.get("exact_sets")
        if not isinstance(exact_sets, list):
            raise RuntimeError(
                f"identity hash exact sets missing: {result_group['group_id']}"
            )
        for exact_set in exact_sets:
            members = exact_set.get("members")
            if not isinstance(members, list) or len(members) < 2:
                raise RuntimeError(
                    f"identity hash exact set is invalid: {result_group['group_id']}"
                )
            digest = str(exact_set.get("sha256", ""))
            size_bytes = int(exact_set.get("size_bytes", -1))
            if (
                len(digest) != 64
                or any(ch not in "0123456789ABCDEF" for ch in digest)
                or exact_set.get("member_count") != len(members)
                or exact_set.get("reclaimable_bytes")
                != size_bytes * (len(members) - 1)
            ):
                raise RuntimeError(
                    f"identity hash exact set summary mismatch: "
                    f"{result_group['group_id']}"
                )
            for member in members:
                normalized = member.get("normalized_path")
                candidate = candidate_by_path.get(normalized)
                if (
                    candidate is None
                    or normalized in exact_paths
                    or member.get("sha256") != digest
                    or int(member.get("size_bytes", -1)) != size_bytes
                    or member.get("path") != candidate.get("path")
                    or size_bytes != int(candidate.get("size_bytes", -1))
                ):
                    raise RuntimeError(
                        f"identity hash exact member mismatch: "
                        f"{result_group['group_id']}/{normalized}"
                    )
                exact_paths.add(normalized)
        resolution = result_group.get("resolution")
        if (
            (exact_sets and resolution != "exact_duplicate")
            or (not exact_sets and resolution not in {"hash_split", "size_split"})
        ):
            raise RuntimeError(
                f"identity hash resolution mismatch: {result_group['group_id']}"
            )
        computed_exact_sets += len(exact_sets)
        computed_exact_members += sum(len(row["members"]) for row in exact_sets)
        computed_redundant += sum(len(row["members"]) - 1 for row in exact_sets)
        computed_reclaimable += sum(
            int(row["reclaimable_bytes"]) for row in exact_sets
        )
        computed_size_split += int(resolution == "size_split")
        computed_hash_split += int(resolution == "hash_split")
    derived_summary = {
        "size_split_groups": computed_size_split,
        "hash_split_groups": computed_hash_split,
        "exact_duplicate_sets": computed_exact_sets,
        "exact_member_files": computed_exact_members,
        "redundant_files": computed_redundant,
        "reclaimable_bytes": computed_reclaimable,
    }
    derived_mismatches = {
        key: {"actual": summary.get(key), "computed": value}
        for key, value in derived_summary.items()
        if summary.get(key) != value
    }
    if derived_mismatches:
        raise RuntimeError(
            f"identity hash derived summary mismatch: {derived_mismatches}"
        )
    for group in candidates["groups"]:
        for candidate in group["candidates"]:
            if candidate["normalized_path"] != normalize_path(candidate["path"]):
                raise RuntimeError(
                    f"candidate normalized path mismatch: {candidate['path']}"
                )

    conn.row_factory = sqlite3.Row
    library_by_path = library_rows_by_path(conn)
    updates = []
    creates = []
    unchanged = []
    for group in candidates["groups"]:
        identity = conn.execute(
            "SELECT * FROM media_source_identity "
            "WHERE service='youtube' AND media_id=?",
            (group["media_id"],),
        ).fetchone()
        keeper = select_keeper(group, identity, library_by_path)
        candidate = keeper["candidate"]
        library = keeper["library"]
        if verify_files:
            stat = os.stat(candidate["path"])
            if (
                stat.st_size != int(candidate["size_bytes"])
                or stat.st_mtime_ns != int(candidate["modified_ns"])
            ):
                raise RuntimeError(f"selected keeper stat changed: {candidate['path']}")
        state = group["identity_state"]
        if state == "identity_active_claim":
            if (
                identity is None
                or identity["library_item_id"] is not None
                or not identity["active_job_id"]
            ):
                raise RuntimeError(f"active identity preimage changed: {group['media_id']}")
            job = conn.execute(
                "SELECT id, type, status, item_id, progress, error, finished_at_ms "
                "FROM job WHERE id=?",
                (identity["active_job_id"],),
            ).fetchone()
            if (
                job is None
                or job["type"] != "download_direct_url"
                or job["status"] != "queued"
            ):
                raise RuntimeError(f"active job preimage changed: {group['media_id']}")
            updates.append(
                {
                    "media_id": group["media_id"],
                    "canonical_url": identity["canonical_url"],
                    "expected_active_job_id": identity["active_job_id"],
                    "keeper_library_item_id": library["id"],
                    "keeper_path": candidate["path"],
                    "keeper_normalized_path": candidate["normalized_path"],
                    "keeper_size_bytes": candidate["size_bytes"],
                    "keeper_modified_ns": candidate["modified_ns"],
                }
            )
        elif state == "identity_absent":
            if identity is not None:
                raise RuntimeError(f"absent identity now exists: {group['media_id']}")
            if group["media_id"] != "2-hvGJUuCYQ":
                raise RuntimeError(
                    f"unexpected absent identity: {group['media_id']}"
                )
            creates.append(
                {
                    "media_id": group["media_id"],
                    "canonical_url": (
                        f"https://www.youtube.com/watch?v={group['media_id']}"
                    ),
                    "keeper_library_item_id": library["id"],
                    "keeper_path": candidate["path"],
                    "keeper_normalized_path": candidate["normalized_path"],
                    "keeper_size_bytes": candidate["size_bytes"],
                    "keeper_modified_ns": candidate["modified_ns"],
                    "deferred_associations": deferred_source_context[
                        "associations"
                    ],
                    "deferred_memberships": deferred_source_context[
                        "memberships"
                    ],
                }
            )
        elif state == "identity_linked_present_keeper":
            if (
                identity is None
                or identity["library_item_id"] != library["id"]
                or identity["active_job_id"] is not None
            ):
                raise RuntimeError(f"linked identity preimage changed: {group['media_id']}")
            unchanged.append(
                {
                    "media_id": group["media_id"],
                    "keeper_library_item_id": library["id"],
                    "keeper_path": candidate["path"],
                }
            )
        else:
            raise RuntimeError(f"unexpected identity state: {state!r}")
    if (len(updates), len(creates), len(unchanged)) != (555, 1, 687):
        raise RuntimeError(
            "identity plan count mismatch: "
            f"{len(updates)}/{len(creates)}/{len(unchanged)}"
        )
    if len({row["expected_active_job_id"] for row in updates}) != len(updates):
        raise RuntimeError("active identity jobs are not unique")
    if (
        len(creates[0]["deferred_associations"]) != 1
        or len(creates[0]["deferred_memberships"]) != 1
    ):
        raise RuntimeError("deferred absent identity source context mismatch")
    deferred_association = creates[0]["deferred_associations"][0]
    deferred_membership = creates[0]["deferred_memberships"][0]
    subscription_id = deferred_association["source_subscription_id"]
    if (
        deferred_association["media_id"] != creates[0]["media_id"]
        or deferred_membership["media_id"] != creates[0]["media_id"]
        or deferred_association["origin_kind"] != "subscription"
        or deferred_membership["source_kind"] != "playlist"
        or deferred_membership["source_subscription_id"] != subscription_id
        or deferred_association["id"]
        != deterministic_id(
            "association",
            f"{creates[0]['media_id']}:{subscription_id or ''}:"
            f"{deferred_association['source_job_id']}",
        )
    ):
        raise RuntimeError("deferred source context relationship mismatch")
    source_job = conn.execute(
        "SELECT id, params_json FROM job WHERE id=?",
        (deferred_association["source_job_id"],),
    ).fetchone()
    if source_job is None:
        raise RuntimeError("deferred association source job is missing")
    source_params = json.loads(source_job["params_json"] or "{}")
    if source_params.get("subscription_id") != subscription_id:
        raise RuntimeError("deferred association source job subscription drifted")
    subscription = conn.execute(
        "SELECT title, source_url FROM youtube_subscription WHERE id=?",
        (subscription_id,),
    ).fetchone()
    if (
        subscription is None
        or subscription["title"] != deferred_membership["source_title_snapshot"]
        or subscription["source_url"] != deferred_membership["source_url_snapshot"]
    ):
        raise RuntimeError("deferred membership subscription snapshot drifted")
    return {"updates": updates, "creates": creates, "unchanged": unchanged}


def apply_plan(
    conn: sqlite3.Connection,
    plan: dict[str, Any],
    expected_before: dict[str, Any],
    source_hashes: dict[str, str],
) -> dict[str, Any]:
    now_ms = int(time.time() * 1000)
    conn.execute("BEGIN IMMEDIATE")
    try:
        current = db_state(conn)
        if current != expected_before:
            raise RuntimeError(f"identity reconciliation preimage changed: {current}")
        if not current["queue_paused"] or current["running_direct_jobs"] != 0:
            raise RuntimeError("identity reconciliation requires paused idle queue")
        for row in plan["updates"]:
            changed = conn.execute(
                """
UPDATE media_source_identity
SET library_item_id=?, active_job_id=NULL, repair_state='ready',
    last_failed_url=NULL, last_error=NULL, updated_at_ms=?
WHERE service='youtube' AND media_id=? AND library_item_id IS NULL
  AND active_job_id=?
""",
                (
                    row["keeper_library_item_id"],
                    now_ms,
                    row["media_id"],
                    row["expected_active_job_id"],
                ),
            ).rowcount
            if changed != 1:
                raise RuntimeError(f"identity changed: {row['media_id']}")
            changed = conn.execute(
                """
UPDATE job
SET status='canceled', error=?, finished_at_ms=?
WHERE id=? AND type='download_direct_url' AND status='queued'
""",
                (
                    "WP-0277 reconciled existing physical media; canonical "
                    f"identity linked to {row['keeper_library_item_id']}",
                    now_ms,
                    row["expected_active_job_id"],
                ),
            ).rowcount
            if changed != 1:
                raise RuntimeError(f"active job changed: {row['expected_active_job_id']}")
        for row in plan["creates"]:
            conn.execute(
                """
INSERT INTO media_source_identity (
  service, media_id, canonical_url, library_item_id, active_job_id,
  repair_state, created_at_ms, updated_at_ms
) VALUES ('youtube', ?, ?, ?, NULL, 'ready', ?, ?)
""",
                (
                    row["media_id"],
                    row["canonical_url"],
                    row["keeper_library_item_id"],
                    now_ms,
                    now_ms,
                ),
            )
            conn.execute(
                """
INSERT INTO media_source_alias (service, media_id, source_url, created_at_ms)
VALUES ('youtube', ?, ?, ?)
""",
                (row["media_id"], row["canonical_url"], now_ms),
            )
            for association in row["deferred_associations"]:
                conn.execute(
                    """
INSERT INTO media_source_association (
  id, service, media_id, origin_kind, source_subscription_id,
  source_job_id, created_at_ms
) VALUES (?, 'youtube', ?, ?, ?, ?, ?)
""",
                    (
                        association["id"],
                        row["media_id"],
                        association["origin_kind"],
                        association["source_subscription_id"],
                        association["source_job_id"],
                        now_ms,
                    ),
                )
            for membership in row["deferred_memberships"]:
                conn.execute(
                    """
INSERT INTO media_source_membership (
  service, media_id, source_subscription_id, source_kind,
  source_url_snapshot, source_title_snapshot, evidence_kind,
  created_at_ms, updated_at_ms
) VALUES ('youtube', ?, ?, ?, ?, ?, ?, ?, ?)
""",
                    (
                        row["media_id"],
                        membership["source_subscription_id"],
                        membership["source_kind"],
                        membership["source_url_snapshot"],
                        membership["source_title_snapshot"],
                        "wp0277_reconciled_job_output",
                        now_ms,
                        now_ms,
                    ),
                )
        for row in plan["updates"] + plan["creates"]:
            conn.execute(
                """
INSERT INTO media_import_evidence (
  id, library_item_id, service, media_id, evidence_kind,
  source_record_key, source_path_snapshot, source_url_snapshot,
  match_state, details_json, created_at_ms, updated_at_ms
) VALUES (?, ?, 'youtube', ?, 'wp0277_identity_reconciled',
          ?, ?, ?, 'linked', ?, ?, ?)
""",
                (
                    deterministic_id("identity-reconcile", row["media_id"]),
                    row["keeper_library_item_id"],
                    row["media_id"],
                    row["media_id"],
                    row["keeper_path"],
                    row["canonical_url"],
                    json.dumps(
                        {
                            "candidate_manifest_sha256": source_hashes["candidates"],
                            "hash_result_sha256": source_hashes["hash_result"],
                            "path_reconcile_receipt_sha256": source_hashes[
                                "path_reconcile_receipt"
                            ],
                            "keeper_size_bytes": row["keeper_size_bytes"],
                            "keeper_modified_ns": row["keeper_modified_ns"],
                        },
                        sort_keys=True,
                    ),
                    now_ms,
                    now_ms,
                ),
            )
        after = db_state(conn)
        expected_after = {
            **expected_before,
            "queued_direct_jobs": expected_before["queued_direct_jobs"] - 555,
            "canceled_direct_jobs": expected_before["canceled_direct_jobs"] + 555,
            "source_identities": expected_before["source_identities"] + 1,
            "source_aliases": expected_before["source_aliases"] + 1,
            "import_evidence": expected_before["import_evidence"] + 556,
            "memberships": expected_before["memberships"] + 1,
            "associations": expected_before["associations"] + 1,
        }
        if after != expected_after:
            raise RuntimeError(f"identity reconciliation postcondition failed: {after}")
        for row in plan["updates"] + plan["creates"]:
            identity = conn.execute(
                "SELECT library_item_id, active_job_id, repair_state "
                "FROM media_source_identity "
                "WHERE service='youtube' AND media_id=?",
                (row["media_id"],),
            ).fetchone()
            if (
                identity is None
                or identity[0] != row["keeper_library_item_id"]
                or identity[1] is not None
                or identity[2] != "ready"
            ):
                raise RuntimeError(f"identity postcondition failed: {row['media_id']}")
        conn.commit()
        return after
    except Exception:
        conn.rollback()
        raise


def main() -> int:
    args = parse_args()
    candidate_path = Path(args.candidates)
    hash_path = Path(args.hash_result)
    path_receipt_path = Path(args.path_reconcile_receipt)
    database = Path(args.database)
    receipt_path = Path(args.receipt)
    run_lock = acquire_run_lock(
        database.with_name("wp0277_mutation.run.lock")
    )
    candidates = json.loads(candidate_path.read_text(encoding="utf-8"))
    hash_result = json.loads(hash_path.read_text(encoding="utf-8"))
    source_hashes = {
        "candidates": verify_pinned_file(
            candidate_path, args.expected_candidates_sha256
        ),
        "hash_result": verify_pinned_file(
            hash_path, args.expected_hash_result_sha256
        ),
    }
    conn = sqlite3.connect(database)
    conn.execute("PRAGMA foreign_keys=ON")
    conn.execute("PRAGMA busy_timeout=30000")
    before = db_state(conn)
    if before["quick_check"] != "ok" or before["foreign_key_violations"] != 0:
        raise RuntimeError(f"database integrity precondition failed: {before}")
    if not before["queue_paused"] or before["running_direct_jobs"] != 0:
        raise RuntimeError("identity reconciliation requires paused idle queue")
    deferred_source_context, path_receipt_sha256 = load_path_reconcile_handoff(
        path_receipt_path,
        database,
        before,
        args.expected_path_reconcile_receipt_sha256,
    )
    source_hashes["path_reconcile_receipt"] = path_receipt_sha256
    plan = build_plan(
        conn,
        candidates,
        hash_result,
        source_hashes["candidates"],
        deferred_source_context,
        verify_files=args.apply,
    )
    backup = (
        verify_backup(Path(args.backup), conn, before)
        if args.apply and args.backup
        else None
    )
    if args.apply and backup is None:
        raise RuntimeError("--apply requires --backup")
    after = apply_plan(conn, plan, before, source_hashes) if args.apply else before
    conn.close()
    receipt = {
        "schema": RECEIPT_SCHEMA,
        "work_packet": "WP-0277",
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "mode": "applied" if args.apply else "dry_run",
        "database": str(database),
        "sources": {
            "candidate_manifest": {
                "path": str(candidate_path),
                "sha256": source_hashes["candidates"],
            },
            "hash_result": {
                "path": str(hash_path),
                "sha256": source_hashes["hash_result"],
            },
            "path_reconcile_receipt": {
                "path": str(path_receipt_path),
                "sha256": source_hashes["path_reconcile_receipt"],
            },
        },
        "backup": backup,
        "plan": {
            "identity_updates": len(plan["updates"]),
            "identity_creates": len(plan["creates"]),
            "identity_unchanged": len(plan["unchanged"]),
            "job_cancellations": len(plan["updates"]),
            "evidence_inserts": len(plan["updates"]) + len(plan["creates"]),
            "deferred_association_inserts": sum(
                len(row["deferred_associations"]) for row in plan["creates"]
            ),
            "deferred_membership_inserts": sum(
                len(row["deferred_memberships"]) for row in plan["creates"]
            ),
        },
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
    print(json.dumps(receipt["plan"], sort_keys=True))
    run_lock.close()
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"ERROR: {type(error).__name__}: {error}", file=sys.stderr)
        raise

#!/usr/bin/env python3
"""Plan or atomically apply the WP-0277 evidence-driven path reconciliation."""

from __future__ import annotations

import argparse
import collections
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import sys
import time
import uuid
from typing import Any


EVIDENCE_SCHEMA = "voxvulgi.path_reconcile_evidence.v1"
PROBE_SCHEMA = "voxvulgi.path_reconcile_probe.v1"
RECEIPT_SCHEMA = "voxvulgi.path_reconcile_apply.v1"
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
    parser.add_argument("--evidence", required=True)
    parser.add_argument("--probe-receipt", required=True)
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
    return digest.hexdigest()


def path_key(value: str) -> str:
    value = value.strip().replace("/", "\\")
    lowered = value.casefold()
    if lowered.startswith("\\\\?\\unc\\"):
        value = "\\\\" + value[8:]
    elif lowered.startswith("\\\\?\\"):
        value = value[4:]
    return value.rstrip("\\").casefold()


def stored_path(value: str) -> str:
    value = value.replace("/", "\\")
    if value.casefold().startswith("\\\\?\\unc\\"):
        return value
    if value.startswith("\\\\"):
        return "\\\\?\\UNC\\" + value[2:]
    return value


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
            raise RuntimeError(f"another WP-0277 mutation owns {path}") from error
    except OSError as error:
        handle.close()
        raise RuntimeError(f"another WP-0277 mutation owns {path}") from error
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
        digests[table] = digest.hexdigest()
    return digests


def cleanup_artifact_reason(path: str, probe_status: str) -> str | None:
    name = Path(path).name
    if name.casefold().endswith(".temp.mp4"):
        return "temporary_download"
    if re.search(r"\.f\d+\.(?:mp4|m4a|webm)$", name, re.IGNORECASE):
        return "yt_dlp_format_fragment"
    if probe_status != "ok":
        return "invalid_container"
    return None


def membership_kind(source_url: str, media_id: str) -> str:
    lower = source_url.strip().casefold()
    if "/playlist" in lower or "list=" in lower:
        return "playlist"
    if lower.rstrip("/").endswith("/shorts"):
        return "shorts_page"
    if lower.rstrip("/").endswith("/videos"):
        return "videos_page"
    if media_id.casefold() in lower and (
        "watch?v=" in lower or "youtu.be/" in lower or "/shorts/" in lower
    ):
        return "direct_video"
    return "channel_page"


def load_evidence(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        evidence = json.load(handle)
    if evidence.get("schema") != EVIDENCE_SCHEMA:
        raise RuntimeError(f"unexpected evidence schema: {evidence.get('schema')!r}")
    if evidence.get("read_only_evidence") is not True:
        raise RuntimeError("path evidence is not marked read-only")
    counts = evidence["summary"]["counts"]
    required = {
        "physical_only_total": 5971,
        "physical_only_canonical_identity": 5322,
        "physical_only_unmatched": 649,
        "missing_media_total": 1632,
        "missing_collision_mappings": 1630,
        "missing_exceptions": 2,
        "deleted_part_rows": 199,
        "canonical_evidence_conflicts": 0,
    }
    for key, expected in required.items():
        if counts.get(key) != expected:
            raise RuntimeError(
                f"evidence count mismatch for {key}: {counts.get(key)} != {expected}"
            )
    return evidence


def load_probes(path: Path) -> dict[str, dict[str, Any]]:
    latest: dict[str, dict[str, Any]] = {}
    with path.open("r", encoding="utf-8") as handle:
        for raw in handle:
            row = json.loads(raw)
            if row.get("schema") != PROBE_SCHEMA or row.get("event") != "probe":
                continue
            latest[row["normalized_path"]] = row
    return latest


def db_state(conn: sqlite3.Connection) -> dict[str, Any]:
    paused_row = conn.execute(
        "SELECT value FROM meta WHERE key='jobs_queue_paused'"
    ).fetchone()
    return {
        "quick_check": conn.execute("PRAGMA quick_check").fetchone()[0],
        "queue_paused": bool(paused_row and paused_row[0] == "1"),
        "queued_direct_jobs": conn.execute(
            "SELECT COUNT(*) FROM job "
            "WHERE status='queued' AND type='download_direct_url'"
        ).fetchone()[0],
        "running_direct_jobs": conn.execute(
            "SELECT COUNT(*) FROM job "
            "WHERE status='running' AND type='download_direct_url'"
        ).fetchone()[0],
        "library_items": conn.execute("SELECT COUNT(*) FROM library_item").fetchone()[0],
        "source_identities": conn.execute(
            "SELECT COUNT(*) FROM media_source_identity"
        ).fetchone()[0],
        "import_evidence": conn.execute(
            "SELECT COUNT(*) FROM media_import_evidence"
        ).fetchone()[0],
        "ingest_provenance": conn.execute(
            "SELECT COUNT(*) FROM ingest_provenance"
        ).fetchone()[0],
        "download_lineage": conn.execute(
            "SELECT COUNT(*) FROM library_download_lineage"
        ).fetchone()[0],
        "memberships": conn.execute(
            "SELECT COUNT(*) FROM media_source_membership"
        ).fetchone()[0],
        "associations": conn.execute(
            "SELECT COUNT(*) FROM media_source_association"
        ).fetchone()[0],
        "canceled_direct_jobs": conn.execute(
            "SELECT COUNT(*) FROM job "
            "WHERE status='canceled' AND type='download_direct_url'"
        ).fetchone()[0],
        "deleted_part_library_paths": conn.execute(
            "SELECT COUNT(*) FROM library_item "
            "WHERE lower(media_path) LIKE '%.part'"
        ).fetchone()[0],
        "foreign_key_violations": len(
            conn.execute("PRAGMA foreign_key_check").fetchall()
        ),
    }


def verify_backup(
    backup: Path,
    live_conn: sqlite3.Connection,
    live_state: dict[str, Any],
) -> dict[str, Any]:
    if not backup.is_file():
        raise RuntimeError(f"backup does not exist: {backup}")
    uri = f"file:{backup.as_posix()}?mode=ro"
    conn = sqlite3.connect(uri, uri=True)
    try:
        state = db_state(conn)
        backup_digests = canonical_row_digests(conn)
    finally:
        conn.close()
    if state["quick_check"] != "ok" or state["foreign_key_violations"] != 0:
        raise RuntimeError(f"backup integrity failed: {state}")
    for key in (
        "queue_paused",
        "queued_direct_jobs",
        "running_direct_jobs",
        "library_items",
        "source_identities",
        "import_evidence",
        "ingest_provenance",
        "download_lineage",
        "memberships",
        "associations",
        "canceled_direct_jobs",
        "deleted_part_library_paths",
    ):
        if state[key] != live_state[key]:
            raise RuntimeError(
                f"backup preimage mismatch for {key}: {state[key]} != {live_state[key]}"
            )
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


def build_plan(
    evidence: dict[str, Any],
    probes: dict[str, dict[str, Any]],
    conn: sqlite3.Connection,
    verify_live_files: bool,
) -> dict[str, Any]:
    canonical = evidence["physical_only"]["canonical_identity_records"]
    unmatched = evidence["physical_only"]["unmatched_records"]
    physical = canonical + unmatched
    if len(probes) < len(physical):
        raise RuntimeError(
            f"probe receipt incomplete: {len(probes)} rows for {len(physical)} files"
        )

    existing_paths: dict[str, list[tuple[str, str]]] = collections.defaultdict(list)
    for item_id, media_path in conn.execute(
        "SELECT id, media_path FROM library_item"
    ):
        existing_paths[path_key(media_path)].append((item_id, media_path))
    existing_memberships = {
        (row[0], row[1], row[2])
        for row in conn.execute(
            "SELECT service, media_id, source_subscription_id "
            "FROM media_source_membership"
        )
    }
    existing_associations = {
        (row[0], row[1], row[2], row[3])
        for row in conn.execute(
            "SELECT service, media_id, COALESCE(source_subscription_id, ''), "
            "origin_kind FROM media_source_association"
        )
    }
    existing_identity_media_ids = {
        row[0]
        for row in conn.execute(
            "SELECT media_id FROM media_source_identity WHERE service='youtube'"
        )
    }

    physical_items: list[dict[str, Any]] = []
    by_media_id: dict[str, list[dict[str, Any]]] = collections.defaultdict(list)
    for record in physical:
        file_row = record["file"]
        normalized = file_row["normalized_path"]
        probe = probes.get(normalized)
        if not probe or probe.get("status") not in {"ok", "error"}:
            raise RuntimeError(f"missing completed ffprobe receipt: {file_row['path']}")
        artifact_reason = cleanup_artifact_reason(
            file_row["path"], probe.get("status", "error")
        )
        if probe["observed_size_bytes"] != file_row["size_bytes"] and not artifact_reason:
            raise RuntimeError(f"probe size mismatch: {file_row['path']}")
        if probe["observed_modified_ns"] != file_row["modified_ns"] and not artifact_reason:
            raise RuntimeError(f"probe mtime mismatch: {file_row['path']}")
        if verify_live_files:
            stat = os.stat(file_row["path"])
            expected_size = (
                probe.get("observed_size_bytes")
                if artifact_reason
                else file_row["size_bytes"]
            )
            expected_mtime = (
                probe.get("observed_modified_ns")
                if artifact_reason
                else file_row["modified_ns"]
            )
            if (
                stat.st_size != expected_size
                or stat.st_mtime_ns != expected_mtime
            ):
                raise RuntimeError(
                    f"physical file changed after evidence: {file_row['path']}"
                )
        if existing_paths.get(normalized):
            raise RuntimeError(
                f"physical-only path is already indexed: {file_row['path']}"
            )
        media_id = record.get("canonical_media_id")
        sources = record.get("evidence_sources") or []
        from_job = "voxvulgi_job_output_suffix" in sources
        canonical_url = (
            record.get("current_identity") or {}
        ).get("canonical_url") or (
            f"https://www.youtube.com/watch?v={media_id}" if media_id else None
        )
        item_id = deterministic_id("library", normalized)
        probe_data = probe.get("probe") or {}
        container = probe_data.get("container")
        if container:
            container = container.split(",", 1)[0]
        item = {
            "id": item_id,
            "created_at_ms": int(time.time() * 1000),
            "source_type": "url_direct" if from_job else "local_file",
            "source_uri": canonical_url if from_job else stored_path(file_row["path"]),
            "title": Path(file_row["path"]).stem,
            "media_path": stored_path(file_row["path"]),
            "duration_ms": probe_data.get("duration_ms"),
            "width": probe_data.get("width"),
            "height": probe_data.get("height"),
            "container": container,
            "video_codec": probe_data.get("video_codec"),
            "audio_codec": probe_data.get("audio_codec"),
            "thumbnail_path": None,
            "library_id": "default-video-library",
            "origin": "voxvulgi_download" if from_job else (
                "4kvdp_import" if media_id else "local_import"
            ),
            "normalized_path": normalized,
            "classification": (
                "canonical_identity" if media_id else "unmatched_local"
            ),
            "media_id": media_id,
            "canonical_url": canonical_url,
            "identity_state": record.get("identity_state"),
            "content_valid": probe.get("status") == "ok",
            "content_error": probe.get("error"),
            "artifact_reason": artifact_reason,
            "observed_size_bytes": probe.get("observed_size_bytes"),
            "observed_modified_ns": probe.get("observed_modified_ns"),
            "record": record,
        }
        if from_job:
            prefix = ((record.get("parsed_vv_suffix") or {}).get("job_id_prefix") or "")
            jobs = record.get("matching_job_evidence") or []
            job = next(
                (row for row in jobs if row["id"].casefold().startswith(prefix.casefold())),
                jobs[0] if len(jobs) == 1 else None,
            )
            if not job:
                raise RuntimeError(
                    f"job-output evidence has no exact source job: {file_row['path']}"
                )
            params = job.get("params") or {}
            subscription_id = params.get("subscription_id")
            item["lineage"] = {
                "source_job_id": job["id"],
                "source_batch_id": job.get("batch_id"),
                "source_subscription_id": subscription_id,
                "service": "youtube",
                "origin_kind": "subscription" if subscription_id else "single",
                "work_track": (
                    "youtube_recurring" if subscription_id else "youtube_single"
                ),
                "job_status": job.get("status"),
            }
        else:
            item["lineage"] = None
        physical_items.append(item)
        if media_id and artifact_reason is None:
            by_media_id[media_id].append(item)

    identity_links: list[dict[str, Any]] = []
    identity_creates: list[dict[str, Any]] = []
    job_cancellations: list[dict[str, Any]] = []
    deferred_multi_file_ids: list[str] = []
    artifact_media_ids: list[str] = []
    for media_id, items in sorted(by_media_id.items()):
        if len(items) != 1:
            deferred_multi_file_ids.append(media_id)
            continue
        item = items[0]
        record = item["record"]
        state = item["identity_state"]
        if state == "identity_active_claim":
            identity = record["current_identity"]
            job = record["current_claim_job"]
            if not identity or not job or job.get("status") != "queued":
                raise RuntimeError(f"invalid active-claim evidence for {media_id}")
            identity_links.append(
                {
                    "media_id": media_id,
                    "library_item_id": item["id"],
                    "expected_active_job_id": job["id"],
                }
            )
            job_cancellations.append(
                {
                    "job_id": job["id"],
                    "media_id": media_id,
                    "library_item_id": item["id"],
                }
            )
        elif state == "identity_absent":
            identity_creates.append(
                {
                    "media_id": media_id,
                    "canonical_url": item["canonical_url"],
                    "library_item_id": item["id"],
                    "record": record,
                }
            )
        elif state != "identity_linked_present_keeper":
            raise RuntimeError(f"unknown identity state {state!r} for {media_id}")

    collision_relinks = []
    for record in evidence["missing_media"]["collision_mappings"]:
        source = record["library_item"]
        sibling = record["present_sibling_library_rows"][0]
        file_row = record["present_sibling_file"]
        if verify_live_files:
            stat = os.stat(file_row["path"])
            if (
                stat.st_size != file_row["size_bytes"]
                or stat.st_mtime_ns != file_row["modified_ns"]
            ):
                raise RuntimeError(f"collision target changed: {file_row['path']}")
        collision_relinks.append(
            {
                "library_item_id": source["id"],
                "expected_path": source["media_path"],
                "target_path": sibling["media_path"],
                "target_library_item_id": sibling["id"],
                "classification": record["classification"],
            }
        )

    exception_records = []
    for record in evidence["missing_media"]["exceptions"]:
        exception_records.append(
            {
                "library_item_id": record["library_item"]["id"],
                "expected_path": record["library_item"]["media_path"],
                "classification": record["classification"],
                "media_id": record.get("source_uri_media_id"),
            }
        )

    part_relinks = []
    collision_target_by_item = {
        row["library_item_id"]: row["target_path"] for row in collision_relinks
    }
    for record in evidence["deleted_part_rows"]["records"]:
        source = record["library_item"]
        target = record["reduction"]["expected_final_path"]
        overlap_ids = record.get("overlapping_missing_media_item_ids") or []
        transitive_targets = {
            collision_target_by_item[item_id]
            for item_id in overlap_ids
            if item_id in collision_target_by_item
        }
        if len(transitive_targets) > 1:
            raise RuntimeError(
                f"part row has conflicting collision targets: {source['id']}"
            )
        if transitive_targets:
            target = next(iter(transitive_targets))
        elif record["final_library_rows"]:
            target = record["final_library_rows"][0]["media_path"]
        observation = record.get("final_file_observation") or {}
        target_present = bool(
            transitive_targets
            or (
                observation.get("exists") is not False
                and observation.get("size_bytes") is not None
                and observation.get("modified_ns") is not None
            )
        )
        if target_present and not transitive_targets and verify_live_files:
            stat = os.stat(observation["path"])
            if (
                stat.st_size != observation["size_bytes"]
                or stat.st_mtime_ns != observation["modified_ns"]
            ):
                raise RuntimeError(f"part final changed: {observation['path']}")
        part_relinks.append(
            {
                "library_item_id": source["id"],
                "expected_path": source["media_path"],
                "target_path": target,
                "classification": record["classification"],
                "target_present": target_present,
            }
        )

    library_inserts = [
        row for row in physical_items if row["artifact_reason"] is None
    ]
    artifact_physical = [
        row for row in physical_items if row["artifact_reason"] is not None
    ]
    artifact_media_ids = sorted(
        {
            row["media_id"]
            for row in artifact_physical
            if row["media_id"] is not None
        }
    )
    provenance_rows = [
        {
            "library_item_id": row["id"],
            "source_url": row["canonical_url"],
            "attested_at_ms": row["created_at_ms"],
        }
        for row in library_inserts
        if row["media_id"] is not None
    ]
    lineage_rows = [
        {
            "library_item_id": row["id"],
            "item_created_at_ms": row["created_at_ms"],
            **row["lineage"],
            "record": row["record"],
            "media_id": row["media_id"],
        }
        for row in library_inserts
        if row["lineage"] is not None
    ]
    failed_job_links = [
        {
            "job_id": row["source_job_id"],
            "library_item_id": row["library_item_id"],
        }
        for row in lineage_rows
        if row["job_status"] == "failed"
    ]
    membership_inserts: dict[tuple[str, str, str], dict[str, Any]] = {}
    association_inserts: dict[tuple[str, str, str, str], dict[str, Any]] = {}
    for row in lineage_rows:
        media_id = row["media_id"]
        job_id = row["source_job_id"]
        subscription_id = row["source_subscription_id"]
        association_key = (
            "youtube",
            media_id,
            subscription_id or "",
            row["origin_kind"],
        )
        if association_key not in existing_associations:
            association_inserts[association_key] = {
                "id": deterministic_id(
                    "association",
                    f"{media_id}:{subscription_id or ''}:{job_id}",
                ),
                "media_id": media_id,
                "origin_kind": row["origin_kind"],
                "source_subscription_id": subscription_id,
                "source_job_id": job_id,
            }
        if not subscription_id:
            continue
        membership_key = ("youtube", media_id, subscription_id)
        if membership_key in existing_memberships:
            continue
        subscription = next(
            (
                value
                for value in row["record"].get("subscriptions") or []
                if value["id"] == subscription_id
            ),
            None,
        )
        if not subscription:
            raise RuntimeError(
                f"missing subscription snapshot for {media_id}/{subscription_id}"
            )
        membership_inserts[membership_key] = {
            "media_id": media_id,
            "source_subscription_id": subscription_id,
            "source_kind": membership_kind(subscription["source_url"], media_id),
            "source_url_snapshot": subscription["source_url"],
            "source_title_snapshot": subscription["title"],
        }
    planned_identity_media_ids = existing_identity_media_ids | {
        row["media_id"] for row in identity_creates
    }
    deferred_association_inserts = {
        key: row
        for key, row in association_inserts.items()
        if row["media_id"] not in planned_identity_media_ids
    }
    association_inserts = {
        key: row
        for key, row in association_inserts.items()
        if row["media_id"] in planned_identity_media_ids
    }
    deferred_membership_inserts = {
        key: row
        for key, row in membership_inserts.items()
        if row["media_id"] not in planned_identity_media_ids
    }
    membership_inserts = {
        key: row
        for key, row in membership_inserts.items()
        if row["media_id"] in planned_identity_media_ids
    }
    all_association_inserts = association_inserts | deferred_association_inserts
    all_membership_inserts = membership_inserts | deferred_membership_inserts
    identity_create_association_keys = {
        (
            "youtube",
            create["media_id"],
            (job.get("params") or {}).get("subscription_id") or "",
            "subscription",
        )
        for create in identity_creates
        for job in (create["record"].get("matching_job_evidence") or [])
        if (job.get("params") or {}).get("subscription_id")
    }
    identity_create_membership_keys = {
        (service, media_id, subscription_id)
        for service, media_id, subscription_id, _origin_kind
        in identity_create_association_keys
    }
    if not identity_create_association_keys.issubset(all_association_inserts):
        raise RuntimeError(
            "identity-create association evidence is not covered by the centralized plan"
        )
    if not identity_create_membership_keys.issubset(all_membership_inserts):
        raise RuntimeError(
            "identity-create membership evidence is not covered by the centralized plan"
        )
    if (
        len(physical_items) != 5971
        or len(collision_relinks) != 1630
        or len(part_relinks) != 199
    ):
        raise RuntimeError("reconciliation plan count assertion failed")
    if len(identity_links) != len(job_cancellations):
        raise RuntimeError(
            "identity plan assertion failed: "
            f"links={len(identity_links)} creates={len(identity_creates)} "
            f"cancels={len(job_cancellations)} deferred={len(deferred_multi_file_ids)}"
        )
    expected_atomic_counts = {
        "provenance": 4980,
        "lineage": 3574,
        "failed_job_links": 3574,
        "membership_inserts": 64,
        "association_inserts": 65,
        "deferred_membership_inserts": 1,
        "deferred_association_inserts": 1,
    }
    actual_atomic_counts = {
        "provenance": len(provenance_rows),
        "lineage": len(lineage_rows),
        "failed_job_links": len(failed_job_links),
        "membership_inserts": len(membership_inserts),
        "association_inserts": len(association_inserts),
        "deferred_membership_inserts": len(deferred_membership_inserts),
        "deferred_association_inserts": len(deferred_association_inserts),
    }
    if actual_atomic_counts != expected_atomic_counts:
        raise RuntimeError(
            f"lineage/source-context plan mismatch: {actual_atomic_counts} "
            f"!= {expected_atomic_counts}"
        )
    return {
        "library_inserts": library_inserts,
        "artifact_physical": artifact_physical,
        "identity_links": identity_links,
        "identity_creates": identity_creates,
        "job_cancellations": job_cancellations,
        "deferred_multi_file_ids": deferred_multi_file_ids,
        "artifact_media_ids": artifact_media_ids,
        "provenance_rows": provenance_rows,
        "lineage_rows": lineage_rows,
        "failed_job_links": failed_job_links,
        "membership_inserts": list(membership_inserts.values()),
        "association_inserts": list(association_inserts.values()),
        "deferred_membership_inserts": list(
            deferred_membership_inserts.values()
        ),
        "deferred_association_inserts": list(
            deferred_association_inserts.values()
        ),
        "collision_relinks": collision_relinks,
        "missing_exceptions": exception_records,
        "part_relinks": part_relinks,
    }


def insert_import_evidence(
    conn: sqlite3.Connection,
    *,
    library_item_id: str | None,
    service: str,
    media_id: str | None,
    evidence_kind: str,
    source_record_key: str,
    source_path: str,
    source_url: str | None,
    match_state: str,
    details: dict[str, Any],
    now_ms: int,
) -> None:
    evidence_id = deterministic_id(
        "evidence",
        f"{evidence_kind}:{source_record_key}:{library_item_id}:{media_id or ''}",
    )
    conn.execute(
        """
        INSERT INTO media_import_evidence (
          id, library_item_id, service, media_id, evidence_kind,
          source_record_key, source_path_snapshot, source_url_snapshot,
          match_state, details_json, created_at_ms, updated_at_ms
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            evidence_id,
            library_item_id,
            service,
            media_id,
            evidence_kind,
            source_record_key,
            source_path,
            source_url,
            match_state,
            json.dumps(details, ensure_ascii=False, sort_keys=True),
            now_ms,
            now_ms,
        ),
    )


def apply_plan(
    conn: sqlite3.Connection,
    plan: dict[str, Any],
    evidence_sha: str,
    expected_before: dict[str, Any],
) -> dict[str, Any]:
    now_ms = int(time.time() * 1000)
    conn.execute("BEGIN IMMEDIATE")
    try:
        state = db_state(conn)
        for key, expected in expected_before.items():
            if state.get(key) != expected:
                raise RuntimeError(
                    f"apply preimage changed for {key}: {state.get(key)} != {expected}"
                )
        if (
            not state["queue_paused"]
            or state["running_direct_jobs"] != 0
            or state["quick_check"] != "ok"
            or state["foreign_key_violations"] != 0
        ):
            raise RuntimeError(f"live apply gate failed: {state}")
        default_library = conn.execute(
            "SELECT id FROM video_library WHERE kind='default' ORDER BY created_at_ms LIMIT 1"
        ).fetchone()
        if not default_library:
            raise RuntimeError("default video library is missing")

        for item in plan["library_inserts"]:
            if conn.execute(
                "SELECT 1 FROM library_item WHERE id=?", (item["id"],)
            ).fetchone():
                raise RuntimeError(f"planned library ID already exists: {item['id']}")
            conn.execute(
                """
                INSERT INTO library_item (
                  id, created_at_ms, source_type, source_uri, title, media_path,
                  duration_ms, width, height, container, video_codec, audio_codec,
                  thumbnail_path, library_id, origin
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    item["id"],
                    item["created_at_ms"],
                    item["source_type"],
                    item["source_uri"],
                    item["title"],
                    item["media_path"],
                    item["duration_ms"],
                    item["width"],
                    item["height"],
                    item["container"],
                    item["video_codec"],
                    item["audio_codec"],
                    item["thumbnail_path"],
                    default_library[0],
                    item["origin"],
                ),
            )
            insert_import_evidence(
                conn,
                library_item_id=item["id"],
                service="youtube" if item["media_id"] else "local",
                media_id=item["media_id"],
                evidence_kind="wp0277_physical_only",
                source_record_key=item["normalized_path"],
                source_path=item["media_path"],
                source_url=item["canonical_url"],
                match_state=(
                    "indexed" if item["content_valid"] else "invalid_container"
                ),
                details={
                    "evidence_sha256": evidence_sha,
                    "classification": item["classification"],
                    "identity_state": item["identity_state"],
                    "evidence_sources": item["record"].get("evidence_sources", []),
                    "content_valid": item["content_valid"],
                    "content_error": item["content_error"],
                },
                now_ms=now_ms,
            )

        for row in plan["provenance_rows"]:
            conn.execute(
                """
                INSERT INTO ingest_provenance (
                  item_id, provider, source_url, rights_note,
                  attested_at_ms, created_at_ms
                ) VALUES (?, 'youtube_yt_dlp_v1', ?, 'not_collected', ?, ?)
                """,
                (
                    row["library_item_id"],
                    row["source_url"],
                    row["attested_at_ms"],
                    now_ms,
                ),
            )

        for row in plan["lineage_rows"]:
            conn.execute(
                """
                INSERT INTO library_download_lineage (
                  item_id, source_job_id, source_batch_id, source_subscription_id,
                  service, origin_kind, work_track, item_created_at_ms, recorded_at_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    row["library_item_id"],
                    row["source_job_id"],
                    row["source_batch_id"],
                    row["source_subscription_id"],
                    row["service"],
                    row["origin_kind"],
                    row["work_track"],
                    row["item_created_at_ms"],
                    now_ms,
                ),
            )

        for row in plan["failed_job_links"]:
            changed = conn.execute(
                "UPDATE job SET item_id=? WHERE id=? AND status='failed' AND item_id IS NULL",
                (row["library_item_id"], row["job_id"]),
            ).rowcount
            if changed != 1:
                raise RuntimeError(f"failed source job changed: {row['job_id']}")

        for item in plan["artifact_physical"]:
            insert_import_evidence(
                conn,
                library_item_id=None,
                service="youtube" if item["media_id"] else "local",
                media_id=item["media_id"],
                evidence_kind="wp0277_cleanup_artifact",
                source_record_key=item["normalized_path"],
                source_path=item["media_path"],
                source_url=item["canonical_url"],
                match_state="quarantine_pending",
                details={
                    "evidence_sha256": evidence_sha,
                    "classification": item["classification"],
                    "identity_state": item["identity_state"],
                    "evidence_sources": item["record"].get("evidence_sources", []),
                    "content_error": item["content_error"],
                    "artifact_reason": item["artifact_reason"],
                    "evidence_size_bytes": item["record"]["file"]["size_bytes"],
                    "evidence_modified_ns": item["record"]["file"]["modified_ns"],
                    "observed_size_bytes": item["observed_size_bytes"],
                    "observed_modified_ns": item["observed_modified_ns"],
                },
                now_ms=now_ms,
            )

        for link in plan["identity_links"]:
            changed = conn.execute(
                """
                UPDATE media_source_identity
                SET library_item_id=?, active_job_id=NULL, repair_state='ready',
                    last_failed_url=NULL, last_error=NULL, updated_at_ms=?
                WHERE service='youtube' AND media_id=? AND library_item_id IS NULL
                  AND active_job_id=?
                """,
                (
                    link["library_item_id"],
                    now_ms,
                    link["media_id"],
                    link["expected_active_job_id"],
                ),
            ).rowcount
            if changed != 1:
                raise RuntimeError(f"identity claim changed: {link['media_id']}")

        for create in plan["identity_creates"]:
            conn.execute(
                """
                INSERT INTO media_source_identity (
                  service, media_id, canonical_url, library_item_id,
                  active_job_id, repair_state, created_at_ms, updated_at_ms
                ) VALUES ('youtube', ?, ?, ?, NULL, 'ready', ?, ?)
                """,
                (
                    create["media_id"],
                    create["canonical_url"],
                    create["library_item_id"],
                    now_ms,
                    now_ms,
                ),
            )
            conn.execute(
                """
                INSERT OR IGNORE INTO media_source_alias (
                  service, media_id, source_url, created_at_ms
                ) VALUES ('youtube', ?, ?, ?)
                """,
                (
                    create["media_id"],
                    create["canonical_url"],
                    now_ms,
                ),
            )

        for row in plan["association_inserts"]:
            conn.execute(
                """
                INSERT INTO media_source_association (
                  id, service, media_id, origin_kind, source_subscription_id,
                  source_job_id, created_at_ms
                ) VALUES (?, 'youtube', ?, ?, ?, ?, ?)
                """,
                (
                    row["id"],
                    row["media_id"],
                    row["origin_kind"],
                    row["source_subscription_id"],
                    row["source_job_id"],
                    now_ms,
                ),
            )

        for row in plan["membership_inserts"]:
            conn.execute(
                """
                INSERT INTO media_source_membership (
                  service, media_id, source_subscription_id, source_kind,
                  source_url_snapshot, source_title_snapshot, evidence_kind,
                  created_at_ms, updated_at_ms
                ) VALUES (
                  'youtube', ?, ?, ?, ?, ?, 'wp0277_reconciled_job_output', ?, ?
                )
                """,
                (
                    row["media_id"],
                    row["source_subscription_id"],
                    row["source_kind"],
                    row["source_url_snapshot"],
                    row["source_title_snapshot"],
                    now_ms,
                    now_ms,
                ),
            )

        for cancel in plan["job_cancellations"]:
            changed = conn.execute(
                """
                UPDATE job
                SET status='canceled',
                    error='WP-0277 reconciled validated physical output; download suppressed',
                    finished_at_ms=COALESCE(finished_at_ms, ?)
                WHERE id=? AND status='queued' AND type='download_direct_url'
                """,
                (now_ms, cancel["job_id"]),
            ).rowcount
            if changed != 1:
                raise RuntimeError(f"queued job changed: {cancel['job_id']}")

        for relink in plan["collision_relinks"]:
            changed = conn.execute(
                "UPDATE library_item SET media_path=? WHERE id=? AND media_path=?",
                (
                    relink["target_path"],
                    relink["library_item_id"],
                    relink["expected_path"],
                ),
            ).rowcount
            if changed != 1:
                raise RuntimeError(
                    f"collision source changed: {relink['library_item_id']}"
                )
            insert_import_evidence(
                conn,
                library_item_id=relink["library_item_id"],
                service="local",
                media_id=None,
                evidence_kind="wp0277_missing_collision_relink",
                source_record_key=relink["library_item_id"],
                source_path=relink["expected_path"],
                source_url=None,
                match_state="relinked",
                details={
                    "evidence_sha256": evidence_sha,
                    "target_path": relink["target_path"],
                    "target_library_item_id": relink["target_library_item_id"],
                },
                now_ms=now_ms,
            )

        for exception in plan["missing_exceptions"]:
            current = conn.execute(
                "SELECT media_path FROM library_item WHERE id=?",
                (exception["library_item_id"],),
            ).fetchone()
            if not current or current[0] != exception["expected_path"]:
                raise RuntimeError(
                    f"missing exception changed: {exception['library_item_id']}"
                )
            insert_import_evidence(
                conn,
                library_item_id=exception["library_item_id"],
                service="youtube" if exception["media_id"] else "local",
                media_id=exception["media_id"],
                evidence_kind="wp0277_missing_exception",
                source_record_key=exception["library_item_id"],
                source_path=exception["expected_path"],
                source_url=None,
                match_state="unresolved",
                details={
                    "evidence_sha256": evidence_sha,
                    "classification": exception["classification"],
                },
                now_ms=now_ms,
            )
            if exception["media_id"]:
                conn.execute(
                    """
                    INSERT INTO media_source_identity (
                      service, media_id, canonical_url, library_item_id,
                      active_job_id, repair_state, last_error,
                      created_at_ms, updated_at_ms
                    ) VALUES (
                      'youtube', ?, ?, ?, NULL, 'missing',
                      'WP-0277 canonical source exists but media path is missing', ?, ?
                    )
                    """,
                    (
                        exception["media_id"],
                        f"https://www.youtube.com/watch?v={exception['media_id']}",
                        exception["library_item_id"],
                        now_ms,
                        now_ms,
                    ),
                )
                source_url = conn.execute(
                    "SELECT source_uri FROM library_item WHERE id=?",
                    (exception["library_item_id"],),
                ).fetchone()[0]
                for alias in (
                    source_url,
                    f"https://www.youtube.com/watch?v={exception['media_id']}",
                ):
                    conn.execute(
                        """
                        INSERT INTO media_source_alias (
                          service, media_id, source_url, created_at_ms
                        ) VALUES ('youtube', ?, ?, ?)
                        """,
                        (exception["media_id"], alias, now_ms),
                    )

        for relink in plan["part_relinks"]:
            changed = conn.execute(
                "UPDATE library_item SET media_path=? WHERE id=? AND media_path=?",
                (
                    relink["target_path"],
                    relink["library_item_id"],
                    relink["expected_path"],
                ),
            ).rowcount
            if changed != 1:
                raise RuntimeError(f"part source changed: {relink['library_item_id']}")
            insert_import_evidence(
                conn,
                library_item_id=relink["library_item_id"],
                service="local",
                media_id=None,
                evidence_kind="wp0277_deleted_part_reduction",
                source_record_key=relink["library_item_id"],
                source_path=relink["expected_path"],
                source_url=None,
                match_state="relinked" if relink["target_present"] else "missing",
                details={
                    "evidence_sha256": evidence_sha,
                    "classification": relink["classification"],
                    "target_path": relink["target_path"],
                },
                now_ms=now_ms,
            )
        transactional_after = db_state(conn)
        expected_after = {
            "library_items": expected_before["library_items"]
            + len(plan["library_inserts"]),
            "source_identities": expected_before["source_identities"]
            + len(plan["identity_creates"])
            + 1,
            "import_evidence": expected_before["import_evidence"] + 7802,
            "ingest_provenance": expected_before["ingest_provenance"]
            + len(plan["provenance_rows"]),
            "download_lineage": expected_before["download_lineage"]
            + len(plan["lineage_rows"]),
            "queued_direct_jobs": expected_before["queued_direct_jobs"]
            - len(plan["job_cancellations"]),
            "canceled_direct_jobs": expected_before["canceled_direct_jobs"]
            + len(plan["job_cancellations"]),
            "memberships": expected_before["memberships"]
            + len(plan["membership_inserts"]),
            "associations": expected_before["associations"]
            + len(plan["association_inserts"]),
            "deleted_part_library_paths": 0,
            "running_direct_jobs": 0,
            "queue_paused": True,
            "foreign_key_violations": 0,
            "quick_check": "ok",
        }
        for key, expected in expected_after.items():
            if transactional_after.get(key) != expected:
                raise RuntimeError(
                    f"transaction postcondition failed for {key}: "
                    f"{transactional_after.get(key)} != {expected}"
                )
        for row in plan["collision_relinks"] + plan["part_relinks"]:
            current = conn.execute(
                "SELECT media_path FROM library_item WHERE id=?",
                (row["library_item_id"],),
            ).fetchone()
            if not current or current[0] != row["target_path"]:
                raise RuntimeError(
                    f"transaction path postcondition failed: {row['library_item_id']}"
                )
        conn.commit()
        return transactional_after
    except Exception:
        conn.rollback()
        raise


def plan_summary(plan: dict[str, Any]) -> dict[str, Any]:
    return {
        "library_inserts": len(plan["library_inserts"]),
        "cleanup_artifacts_deferred_to_quarantine": len(
            plan["artifact_physical"]
        ),
        "canonical_library_inserts": sum(
            row["classification"] == "canonical_identity"
            for row in plan["library_inserts"]
        ),
        "unmatched_library_inserts": sum(
            row["classification"] == "unmatched_local"
            for row in plan["library_inserts"]
        ),
        "identity_links": len(plan["identity_links"]),
        "identity_creates": len(plan["identity_creates"]),
        "job_cancellations": len(plan["job_cancellations"]),
        "deferred_multi_file_ids": len(plan["deferred_multi_file_ids"]),
        "artifact_media_ids": len(plan["artifact_media_ids"]),
        "ingest_provenance_inserts": len(plan["provenance_rows"]),
        "download_lineage_inserts": len(plan["lineage_rows"]),
        "failed_job_item_links": len(plan["failed_job_links"]),
        "membership_inserts": len(plan["membership_inserts"]),
        "association_inserts": len(plan["association_inserts"]),
        "deferred_membership_inserts": len(
            plan["deferred_membership_inserts"]
        ),
        "deferred_association_inserts": len(
            plan["deferred_association_inserts"]
        ),
        "collision_relinks": len(plan["collision_relinks"]),
        "missing_exceptions_preserved": len(plan["missing_exceptions"]),
        "part_relinks": len(plan["part_relinks"]),
        "records_deleted": 0,
    }


def main() -> int:
    args = parse_args()
    evidence_path = Path(args.evidence)
    probe_path = Path(args.probe_receipt)
    database_path = Path(args.database)
    receipt_path = Path(args.receipt)
    run_lock = acquire_run_lock(
        database_path.with_name("wp0277_mutation.run.lock")
    )
    if args.apply and not args.backup:
        raise RuntimeError("--apply requires --backup")
    evidence = load_evidence(evidence_path)
    probes = load_probes(probe_path)
    conn = sqlite3.connect(database_path, timeout=30)
    conn.execute("PRAGMA foreign_keys=ON")
    try:
        before = db_state(conn)
        if (
            before["quick_check"] != "ok"
            or not before["queue_paused"]
            or before["running_direct_jobs"] != 0
            or before["foreign_key_violations"] != 0
        ):
            raise RuntimeError(f"database preflight failed: {before}")
        backup = (
            verify_backup(Path(args.backup), conn, before) if args.apply else None
        )
        plan = build_plan(evidence, probes, conn, verify_live_files=args.apply)
        summary = plan_summary(plan)
        transactional_after = None
        if args.apply:
            transactional_after = apply_plan(
                conn,
                plan,
                sha256_file(evidence_path),
                before,
            )
        after = db_state(conn)
        if args.apply:
            if after != transactional_after:
                raise RuntimeError(
                    "database state changed between transactional audit and "
                    f"post-commit audit: {after} != {transactional_after}"
                )
            if (
                not after["queue_paused"]
                or after["running_direct_jobs"] != 0
                or after["quick_check"] != "ok"
                or after["foreign_key_violations"] != 0
            ):
                raise RuntimeError(f"post-apply integrity failed: {after}")
        receipt = {
            "schema": RECEIPT_SCHEMA,
            "work_packet": "WP-0277",
            "mode": "apply" if args.apply else "dry_run",
            "created_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
            "evidence": {
                "path": str(evidence_path),
                "sha256": sha256_file(evidence_path),
            },
            "probe_receipt": {
                "path": str(probe_path),
                "sha256": sha256_file(probe_path),
                "successful_paths": sum(
                    row.get("status") == "ok" for row in probes.values()
                ),
            },
            "database": str(database_path),
            "backup": backup,
            "before": before,
            "plan": summary,
            "after": after,
            "deferred_multi_file_ids": plan["deferred_multi_file_ids"],
            "deferred_source_context": {
                "associations": plan["deferred_association_inserts"],
                "memberships": plan["deferred_membership_inserts"],
            },
        }
        receipt_path.parent.mkdir(parents=True, exist_ok=True)
        blob = (
            json.dumps(receipt, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
        ).encode("utf-8")
        receipt_path.write_bytes(blob)
        digest = hashlib.sha256(blob).hexdigest()
        receipt_path.with_suffix(receipt_path.suffix + ".sha256").write_text(
            f"{digest}  {receipt_path.name}\n",
            encoding="ascii",
        )
        print(
            json.dumps(
                {
                    "receipt": str(receipt_path),
                    "sha256": digest,
                    "mode": receipt["mode"],
                    "plan": summary,
                    "before": before,
                    "after": after,
                },
                sort_keys=True,
            )
        )
    finally:
        conn.close()
        run_lock.close()
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"ERROR: {error}", file=sys.stderr)
        raise

#!/usr/bin/env python3
"""Merge all WP-0277 exact-hash lanes into one deterministic quarantine manifest."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import sys
from typing import Any


CONFIRMED_SCHEMA = "voxvulgi.media_cleanup_plan.v1"
REMAINING_SCHEMA = "voxvulgi.wp0277.full_hash_result.v1"
RECONCILED_SCHEMA = "voxvulgi.wp0277.reconciled_identity_hash_result.v1"
OUTPUT_SCHEMA = "voxvulgi.wp0277.duplicate_quarantine_manifest.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--confirmed-plan", required=True)
    parser.add_argument("--remaining-result", required=True)
    parser.add_argument("--reconciled-result", required=True)
    parser.add_argument("--expected-confirmed-sha256", required=True)
    parser.add_argument("--expected-remaining-sha256", required=True)
    parser.add_argument("--expected-reconciled-sha256", required=True)
    parser.add_argument("--database", required=True)
    parser.add_argument("--quarantine-root", required=True)
    parser.add_argument("--output", required=True)
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


def load_json(path: Path, schema: str) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if value.get("schema") != schema:
        raise RuntimeError(f"unexpected schema in {path}: {value.get('schema')!r}")
    return value


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


def add_member(
    buckets: dict[tuple[str, int], dict[str, dict[str, Any]]],
    path_hashes: dict[str, tuple[str, int]],
    *,
    path: str,
    sha256: str,
    size_bytes: int,
    media_id: str | None,
    lane: str,
) -> None:
    normalized = normalize_path(path)
    content_key = (sha256.upper(), int(size_bytes))
    prior = path_hashes.get(normalized)
    if prior is not None and prior != content_key:
        raise RuntimeError(f"conflicting content claims for {path}")
    path_hashes[normalized] = content_key
    member = buckets.setdefault(content_key, {}).setdefault(
        normalized,
        {
            "path": path,
            "normalized_path": normalized,
            "sha256": content_key[0],
            "size_bytes": content_key[1],
            "media_ids": set(),
            "source_lanes": set(),
        },
    )
    if media_id:
        member["media_ids"].add(media_id)
    member["source_lanes"].add(lane)


def collect_members(
    confirmed: dict[str, Any],
    remaining: dict[str, Any],
    reconciled: dict[str, Any],
) -> dict[tuple[str, int], dict[str, dict[str, Any]]]:
    buckets: dict[tuple[str, int], dict[str, dict[str, Any]]] = {}
    path_hashes: dict[str, tuple[str, int]] = {}
    for group in confirmed["groups"]:
        members = [group["keeper"]] + group["redundant_members"]
        for member in members:
            add_member(
                buckets,
                path_hashes,
                path=member["path"],
                sha256=group["sha256"],
                size_bytes=group["size_bytes"],
                media_id=group.get("source_identity"),
                lane="confirmed_57gb",
            )
    for group in remaining["groups"]:
        for exact_set in group["exact_sets"]:
            for member in exact_set["members"]:
                add_member(
                    buckets,
                    path_hashes,
                    path=member["path"],
                    sha256=exact_set["sha256"],
                    size_bytes=exact_set["size_bytes"],
                    media_id=group.get("media_id"),
                    lane="remaining_1092",
                )
    for group in reconciled["groups"]:
        for exact_set in group["exact_sets"]:
            for member in exact_set["members"]:
                add_member(
                    buckets,
                    path_hashes,
                    path=member["path"],
                    sha256=exact_set["sha256"],
                    size_bytes=exact_set["size_bytes"],
                    media_id=group.get("media_id"),
                    lane="reconciled_identity",
                )
    return {
        key: members for key, members in buckets.items() if len(members) > 1
    }


def database_context(
    conn: sqlite3.Connection,
) -> tuple[
    dict[str, list[dict[str, Any]]],
    dict[str, list[str]],
    dict[str, int],
    dict[str, int],
]:
    by_path: dict[str, list[dict[str, Any]]] = {}
    for row in conn.execute(
        """
SELECT id, media_path, origin, source_type, source_uri, created_at_ms,
       width, height, duration_ms
FROM library_item
"""
    ):
        value = {
            "id": row[0],
            "media_path": row[1],
            "origin": row[2],
            "source_type": row[3],
            "source_uri": row[4],
            "created_at_ms": row[5],
            "width": row[6],
            "height": row[7],
            "duration_ms": row[8],
        }
        by_path.setdefault(normalize_path(row[1]), []).append(value)
    identities: dict[str, list[str]] = {}
    for item_id, media_id in conn.execute(
        "SELECT library_item_id, media_id FROM media_source_identity "
        "WHERE service='youtube' AND library_item_id IS NOT NULL"
    ):
        identities.setdefault(item_id, []).append(media_id)
    evidence = {
        item_id: count
        for item_id, count in conn.execute(
            "SELECT library_item_id, COUNT(*) FROM media_import_evidence "
            "WHERE library_item_id IS NOT NULL GROUP BY library_item_id"
        )
    }
    provenance = {
        item_id: count
        for item_id, count in conn.execute(
            "SELECT item_id, COUNT(*) FROM ingest_provenance "
            "GROUP BY item_id"
        )
    }
    return by_path, identities, evidence, provenance


def enrich_member(
    member: dict[str, Any],
    by_path: dict[str, list[dict[str, Any]]],
    identities: dict[str, list[str]],
    evidence: dict[str, int],
    provenance: dict[str, int],
) -> dict[str, Any]:
    stat = os.stat(member["path"])
    if stat.st_size != member["size_bytes"] or stat.st_size <= 0:
        raise RuntimeError(f"member size changed: {member['path']}")
    library_rows = by_path.get(member["normalized_path"], [])
    if not library_rows:
        raise RuntimeError(f"member has no VV library metadata: {member['path']}")
    enriched_rows = []
    for row in library_rows:
        item_id = row["id"]
        enriched_rows.append(
            {
                **row,
                "identities": sorted(identities.get(item_id, [])),
                "import_evidence_count": evidence.get(item_id, 0),
                "ingest_provenance_count": provenance.get(item_id, 0),
            }
        )
    return {
        **member,
        "media_ids": sorted(member["media_ids"]),
        "source_lanes": sorted(member["source_lanes"]),
        "observed_size_bytes": stat.st_size,
        "observed_modified_ns": stat.st_mtime_ns,
        "library_rows": enriched_rows,
    }


def library_score(row: dict[str, Any]) -> tuple[Any, ...]:
    return (
        -len(row["identities"]),
        -int(row["import_evidence_count"] > 0),
        -int(row["ingest_provenance_count"] > 0),
        0 if row["origin"] == "voxvulgi_download" else 1,
        0 if row["source_type"] == "url_direct" else 1,
        int(row["created_at_ms"]),
        row["id"],
    )


def member_score(member: dict[str, Any]) -> tuple[Any, ...]:
    best_library = min(member["library_rows"], key=library_score)
    identity_count = sum(len(row["identities"]) for row in member["library_rows"])
    evidence_count = sum(
        row["import_evidence_count"] for row in member["library_rows"]
    )
    provenance_count = sum(
        row["ingest_provenance_count"] for row in member["library_rows"]
    )
    return (
        -identity_count,
        -evidence_count,
        -provenance_count,
        library_score(best_library),
        member["normalized_path"].count("\\"),
        member["normalized_path"],
    )


def main() -> int:
    args = parse_args()
    confirmed_path = Path(args.confirmed_plan)
    remaining_path = Path(args.remaining_result)
    reconciled_path = Path(args.reconciled_result)
    database_path = Path(args.database)
    output_path = Path(args.output)
    source_hashes = {
        "confirmed_plan": verify_pinned_file(
            confirmed_path, args.expected_confirmed_sha256
        ),
        "remaining_result": verify_pinned_file(
            remaining_path, args.expected_remaining_sha256
        ),
        "reconciled_result": verify_pinned_file(
            reconciled_path, args.expected_reconciled_sha256
        ),
    }
    confirmed = load_json(confirmed_path, CONFIRMED_SCHEMA)
    remaining = load_json(remaining_path, REMAINING_SCHEMA)
    reconciled = load_json(reconciled_path, RECONCILED_SCHEMA)
    if remaining["summary"].get("hash_errors") != 0:
        raise RuntimeError("remaining-candidate hash result contains errors")
    if reconciled["summary"].get("hash_errors") != 0:
        raise RuntimeError("reconciled-identity hash result contains errors")
    buckets = collect_members(confirmed, remaining, reconciled)

    conn = sqlite3.connect(database_path)
    quick_check = conn.execute("PRAGMA quick_check").fetchone()[0]
    foreign_keys = len(conn.execute("PRAGMA foreign_key_check").fetchall())
    paused = conn.execute(
        "SELECT value FROM meta WHERE key='jobs_queue_paused'"
    ).fetchone()
    running = conn.execute(
        "SELECT COUNT(*) FROM job "
        "WHERE type='download_direct_url' AND status='running'"
    ).fetchone()[0]
    if quick_check != "ok" or foreign_keys != 0:
        raise RuntimeError("database integrity precondition failed")
    if not paused or paused[0] != "1" or running != 0:
        raise RuntimeError("duplicate manifest requires paused idle queue")
    by_path, identities, evidence, provenance = database_context(conn)
    conn.close()

    quarantine_root = Path(args.quarantine_root)
    groups = []
    actions = []
    destination_keys: set[str] = set()
    for index, ((digest, size), raw_members) in enumerate(
        sorted(buckets.items()), 1
    ):
        members = [
            enrich_member(
                member, by_path, identities, evidence, provenance
            )
            for member in raw_members.values()
        ]
        members.sort(key=member_score)
        keeper = members[0]
        keeper_library = min(keeper["library_rows"], key=library_score)
        group_id = f"WP0277-DUP-{index:04d}-{digest[:16]}"
        group_actions = []
        for member_index, member in enumerate(members[1:], 1):
            action_id = f"{group_id}-{member_index:03d}"
            extension = Path(member["path"]).suffix.casefold()
            source_key = hashlib.sha256(
                member["normalized_path"].encode("utf-8")
            ).hexdigest().upper()[:20]
            destination = quarantine_root / group_id / (
                f"{digest[:16]}_{source_key}{extension}"
            )
            destination_key = normalize_path(str(destination))
            if destination_key in destination_keys:
                raise RuntimeError(f"duplicate quarantine destination: {destination}")
            destination_keys.add(destination_key)
            if destination.exists():
                raise RuntimeError(f"quarantine destination already exists: {destination}")
            source_library_preimages = [
                {"library_item_id": row["id"], "media_path": row["media_path"]}
                for row in member["library_rows"]
            ]
            identity_preimages = [
                {"media_id": media_id, "library_item_id": row["id"]}
                for row in member["library_rows"]
                for media_id in row["identities"]
            ]
            action = {
                "action_id": action_id,
                "group_id": group_id,
                "source_path": member["path"],
                "source_normalized_path": member["normalized_path"],
                "quarantine_path": str(destination),
                "keeper_path": keeper["path"],
                "keeper_normalized_path": keeper["normalized_path"],
                "keeper_library_item_id": keeper_library["id"],
                "keeper_stored_media_path": keeper_library["media_path"],
                "source_library_preimages": source_library_preimages,
                "identity_preimages": identity_preimages,
                "size_bytes": size,
                "full_sha256": digest,
                "source_modified_ns": member["observed_modified_ns"],
                "keeper_modified_ns": keeper["observed_modified_ns"],
                "state": "planned",
            }
            actions.append(action)
            group_actions.append(action_id)
        groups.append(
            {
                "group_id": group_id,
                "full_sha256": digest,
                "size_bytes": size,
                "member_count": len(members),
                "reclaimable_bytes": size * (len(members) - 1),
                "keeper": keeper,
                "keeper_library_item_id": keeper_library["id"],
                "redundant_members": members[1:],
                "action_ids": group_actions,
                "keeper_rule": (
                    "identity-linked metadata; then import evidence; then ingest "
                    "provenance; then existing VV download metadata; then shallow path"
                ),
            }
        )
    manifest = {
        "schema": OUTPUT_SCHEMA,
        "work_packet": "WP-0277",
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "state": "planned_not_applied",
        "database": str(database_path),
        "quarantine_root": str(quarantine_root),
        "sources": {
            "confirmed_plan": {
                "path": str(confirmed_path),
                "sha256": source_hashes["confirmed_plan"],
            },
            "remaining_result": {
                "path": str(remaining_path),
                "sha256": source_hashes["remaining_result"],
            },
            "reconciled_result": {
                "path": str(reconciled_path),
                "sha256": source_hashes["reconciled_result"],
            },
        },
        "preconditions": {
            "queue_paused": True,
            "running_direct_jobs": 0,
            "database_quick_check": quick_check,
            "foreign_key_violations": foreign_keys,
            "all_sources_present_nonzero_size_matched": True,
            "all_destinations_unique_and_absent": True,
        },
        "summary": {
            "exact_duplicate_groups": len(groups),
            "unique_member_files": sum(group["member_count"] for group in groups),
            "redundant_files": len(actions),
            "reclaimable_bytes": sum(
                group["reclaimable_bytes"] for group in groups
            ),
        },
        "groups": groups,
        "actions": actions,
    }
    if len({action["source_normalized_path"] for action in actions}) != len(actions):
        raise RuntimeError("duplicate source actions after global content merge")
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    output_path.with_suffix(output_path.suffix + ".sha256").write_text(
        f"{sha256_file(output_path)}  {output_path.name}\n", encoding="utf-8"
    )
    print(json.dumps(manifest["summary"], sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"ERROR: {type(error).__name__}: {error}", file=sys.stderr)
        raise

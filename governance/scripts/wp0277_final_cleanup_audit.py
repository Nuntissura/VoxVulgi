#!/usr/bin/env python3
"""Final read-only NAS, quarantine, queue, and VV metadata audit for WP-0277."""

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
from typing import Any


DUPLICATE_SCHEMA = "voxvulgi.wp0277.duplicate_quarantine_manifest.v1"
ARTIFACT_SCHEMA = "voxvulgi.wp0277.cleanup_artifact_hash_result.v1"
PATH_RECONCILE_SCHEMA = "voxvulgi.path_reconcile_evidence.v1"
OUTPUT_SCHEMA = "voxvulgi.wp0277.final_cleanup_audit.v1"
MEDIA_EXTENSIONS = {
    ".mp4",
    ".mkv",
    ".webm",
    ".mov",
    ".avi",
    ".m4v",
    ".ts",
    ".mts",
    ".m2ts",
    ".mpg",
    ".mpeg",
    ".wmv",
    ".flv",
    ".m4a",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True)
    parser.add_argument("--database", required=True)
    parser.add_argument("--duplicate-manifest", required=True)
    parser.add_argument("--expected-duplicate-manifest-sha256", required=True)
    parser.add_argument("--duplicate-ledger", required=True)
    parser.add_argument("--artifact-result", required=True)
    parser.add_argument("--expected-artifact-result-sha256", required=True)
    parser.add_argument("--artifact-ledger", required=True)
    parser.add_argument("--path-reconcile-evidence", required=True)
    parser.add_argument("--expected-path-reconcile-evidence-sha256", required=True)
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
    return ntpath.normpath(value).rstrip("\\").casefold()


def is_under_root(path_value: str, normalized_root: str) -> bool:
    normalized = normalize_path(path_value)
    return normalized == normalized_root or normalized.startswith(normalized_root + "\\")


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


def verify_media_file(
    path: Path,
    expected_size: int,
    expected_sha256: str,
) -> str | None:
    try:
        observed = path.stat()
        if not statlib.S_ISREG(observed.st_mode):
            return "not_regular_file"
        if observed.st_size != expected_size:
            return f"size:{observed.st_size}!={expected_size}"
        actual_sha256 = sha256_file(path)
        if actual_sha256 != expected_sha256.upper():
            return f"sha256:{actual_sha256}!={expected_sha256.upper()}"
    except OSError as error:
        return f"{type(error).__name__}:{error}"
    return None


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


def main() -> int:
    args = parse_args()
    root = Path(args.root)
    database = Path(args.database)
    duplicate_path = Path(args.duplicate_manifest)
    duplicate_ledger_path = Path(args.duplicate_ledger)
    artifact_path = Path(args.artifact_result)
    artifact_ledger_path = Path(args.artifact_ledger)
    path_reconcile_path = Path(args.path_reconcile_evidence)
    output_path = Path(args.output)
    duplicate_sha = verify_pinned_file(
        duplicate_path, args.expected_duplicate_manifest_sha256
    )
    artifact_sha = verify_pinned_file(
        artifact_path, args.expected_artifact_result_sha256
    )
    path_reconcile_sha = verify_pinned_file(
        path_reconcile_path, args.expected_path_reconcile_evidence_sha256
    )
    duplicates = load_json(duplicate_path, DUPLICATE_SCHEMA)
    artifacts = load_json(artifact_path, ARTIFACT_SCHEMA)
    path_reconcile = load_json(path_reconcile_path, PATH_RECONCILE_SCHEMA)
    duplicate_status = ledger_status(duplicate_ledger_path)
    artifact_status = ledger_status(artifact_ledger_path)

    physical: dict[str, dict[str, Any]] = {}
    part_files = []
    zero_mp4 = []
    zero_media = []
    non_regular_media = []
    scan_errors = []

    def record_walk_error(error: OSError) -> None:
        scan_errors.append(
            {"path": error.filename or str(root), "error": str(error)}
        )

    for directory, _dirs, names in os.walk(root, onerror=record_walk_error):
        for name in names:
            path = Path(directory) / name
            try:
                stat = path.stat()
            except OSError as error:
                scan_errors.append({"path": str(path), "error": str(error)})
                continue
            lowered = name.casefold()
            is_media = path.suffix.casefold() in MEDIA_EXTENSIONS
            is_regular = statlib.S_ISREG(stat.st_mode)
            if lowered.endswith(".part"):
                part_files.append({"path": str(path), "size_bytes": stat.st_size})
            if lowered.endswith(".mp4") and stat.st_size == 0:
                zero_mp4.append(str(path))
            if is_media and stat.st_size == 0:
                zero_media.append(str(path))
            if is_media and not is_regular:
                non_regular_media.append(str(path))
            if is_media and is_regular and stat.st_size > 0:
                physical[normalize_path(str(path))] = {
                    "path": str(path),
                    "size_bytes": stat.st_size,
                }

    conn = sqlite3.connect(f"file:{database.as_posix()}?mode=ro", uri=True)
    quick_check = conn.execute("PRAGMA quick_check").fetchone()[0]
    foreign_keys = len(conn.execute("PRAGMA foreign_key_check").fetchall())
    paused = conn.execute(
        "SELECT value FROM meta WHERE key='jobs_queue_paused'"
    ).fetchone()
    queued = conn.execute(
        "SELECT COUNT(*) FROM job "
        "WHERE type='download_direct_url' AND status='queued'"
    ).fetchone()[0]
    running = conn.execute(
        "SELECT COUNT(*) FROM job "
        "WHERE type='download_direct_url' AND status='running'"
    ).fetchone()[0]
    queued_claims = conn.execute(
        """
SELECT COUNT(*)
FROM media_source_identity identity
JOIN job ON job.id=identity.active_job_id
WHERE identity.service='youtube'
  AND job.type='download_direct_url'
  AND job.status='queued'
"""
    ).fetchone()[0]
    orphan_queued = conn.execute(
        """
SELECT COUNT(*)
FROM job
WHERE type='download_direct_url' AND status='queued'
  AND NOT EXISTS (
    SELECT 1 FROM media_source_identity identity
    WHERE identity.service='youtube' AND identity.active_job_id=job.id
  )
"""
    ).fetchone()[0]
    stale_claims = conn.execute(
        """
SELECT COUNT(*)
FROM media_source_identity identity
LEFT JOIN job ON job.id=identity.active_job_id
WHERE identity.service='youtube' AND identity.active_job_id IS NOT NULL
  AND (job.id IS NULL OR job.type<>'download_direct_url' OR job.status<>'queued')
"""
    ).fetchone()[0]
    library_rows = conn.execute(
        "SELECT id, media_path FROM library_item"
    ).fetchall()
    library_paths = {normalize_path(row[1]) for row in library_rows}
    missing_library_rows = []
    zero_library_rows = []
    zero_media_library_rows = []
    non_regular_media_library_rows = []
    part_library_rows = []
    for item_id, path_value in library_rows:
        path = Path(path_value)
        if path_value.casefold().endswith(".part"):
            part_library_rows.append({"id": item_id, "path": path_value})
        try:
            stat = path.stat()
            if (
                path.suffix.casefold() in MEDIA_EXTENSIONS
                and not statlib.S_ISREG(stat.st_mode)
            ):
                non_regular_media_library_rows.append(
                    {"id": item_id, "path": path_value}
                )
            if stat.st_size == 0:
                zero_row = {"id": item_id, "path": path_value}
                zero_library_rows.append(zero_row)
                if path.suffix.casefold() in MEDIA_EXTENSIONS:
                    zero_media_library_rows.append(zero_row)
        except OSError:
            missing_library_rows.append({"id": item_id, "path": path_value})
    duplicate_db_path_groups = conn.execute(
        """
SELECT COUNT(*) FROM (
  SELECT lower(replace(media_path, '\\\\?\\UNC\\', '\\\\')) AS path_key
  FROM library_item
  GROUP BY path_key
  HAVING COUNT(*) > 1
)
"""
    ).fetchone()[0]
    artifact_evidence_states = {
        state: count
        for state, count in conn.execute(
            "SELECT match_state, COUNT(*) FROM media_import_evidence "
            "WHERE evidence_kind='wp0277_cleanup_artifact' GROUP BY match_state"
        )
    }
    reconciliation_rows = conn.execute(
        "SELECT library_item_id, evidence_kind, source_record_key, match_state, "
        "details_json FROM media_import_evidence "
        "WHERE evidence_kind IN "
        "('wp0277_deleted_part_reduction', 'wp0277_missing_exception')"
    ).fetchall()
    conn.close()

    physical_only = [
        row for key, row in physical.items() if key not in library_paths
    ]
    duplicate_source_failures = []
    duplicate_destination_failures = []
    duplicate_keeper_expectations = {}
    for action in duplicates["actions"]:
        keeper_key = normalize_path(action["keeper_path"])
        keeper_expectation = {
            "path": action["keeper_path"],
            "size_bytes": int(action["size_bytes"]),
            "full_sha256": action["full_sha256"].upper(),
        }
        existing_keeper = duplicate_keeper_expectations.setdefault(
            keeper_key, keeper_expectation
        )
        if existing_keeper != keeper_expectation:
            raise RuntimeError(
                f"conflicting keeper expectation: {action['keeper_path']}"
            )
    duplicate_keeper_failures = []
    for index, expectation in enumerate(
        duplicate_keeper_expectations.values(), start=1
    ):
        error = verify_media_file(
            Path(expectation["path"]),
            expectation["size_bytes"],
            expectation["full_sha256"],
        )
        if error:
            duplicate_keeper_failures.append(
                {"path": expectation["path"], "error": error}
            )
        if index % 25 == 0:
            print(
                f"keeper_hash_progress={index}/"
                f"{len(duplicate_keeper_expectations)}",
                flush=True,
            )
    for index, action in enumerate(duplicates["actions"], start=1):
        if Path(action["source_path"]).exists():
            duplicate_source_failures.append(action["action_id"])
        destination = Path(action["quarantine_path"])
        error = verify_media_file(
            destination,
            int(action["size_bytes"]),
            action["full_sha256"],
        )
        if error:
            duplicate_destination_failures.append(
                {"action_id": action["action_id"], "error": error}
            )
        if index % 25 == 0:
            print(
                f"duplicate_destination_hash_progress={index}/"
                f"{len(duplicates['actions'])}",
                flush=True,
            )
    artifact_source_failures = []
    artifact_destination_failures = []
    for index, action in enumerate(artifacts["actions"], start=1):
        if Path(action["source_path"]).exists():
            artifact_source_failures.append(action["action_id"])
        destination = Path(action["quarantine_path"])
        error = verify_media_file(
            destination,
            int(action["observed_size_bytes"]),
            action["full_sha256"],
        )
        if error:
            artifact_destination_failures.append(
                {"action_id": action["action_id"], "error": error}
            )
        if index % 25 == 0:
            print(
                f"artifact_destination_hash_progress={index}/"
                f"{len(artifacts['actions'])}",
                flush=True,
            )

    normalized_root = normalize_path(str(root))
    in_scope_missing_library_rows = [
        row
        for row in missing_library_rows
        if is_under_root(row["path"], normalized_root)
    ]
    external_missing_library_rows = [
        row
        for row in missing_library_rows
        if not is_under_root(row["path"], normalized_root)
    ]
    expected_deleted_part_missing = {
        record["library_item"]["id"]: normalize_path(
            record["reduction"]["expected_final_path"]
        )
        for record in path_reconcile["deleted_part_rows"]["records"]
        if record["classification"]
        == "deleted_part_reduces_to_absent_final_without_library_row"
    }
    expected_exception_missing = {
        row["library_item"]["id"]: normalize_path(row["library_item"]["media_path"])
        for row in path_reconcile["missing_media"]["exceptions"]
    }
    expected_reconciled_missing = {
        **expected_deleted_part_missing,
        **expected_exception_missing,
    }
    reconciliation_evidence = {}
    for (
        library_item_id,
        evidence_kind,
        source_record_key,
        match_state,
        details_json,
    ) in reconciliation_rows:
        details = json.loads(details_json)
        reconciliation_evidence[library_item_id] = {
            "evidence_kind": evidence_kind,
            "source_record_key": source_record_key,
            "match_state": match_state,
            "details": details,
        }
    observed_in_scope_missing = {
        row["id"]: normalize_path(row["path"])
        for row in in_scope_missing_library_rows
    }
    reconciliation_evidence_failures = []
    for item_id, expected_path in expected_reconciled_missing.items():
        evidence = reconciliation_evidence.get(item_id)
        expected_kind = (
            "wp0277_deleted_part_reduction"
            if item_id in expected_deleted_part_missing
            else "wp0277_missing_exception"
        )
        expected_state = (
            "missing" if item_id in expected_deleted_part_missing else "unresolved"
        )
        if (
            evidence is None
            or evidence["evidence_kind"] != expected_kind
            or evidence["source_record_key"] != item_id
            or evidence["match_state"] != expected_state
            or evidence["details"].get("evidence_sha256", "").upper()
            != path_reconcile_sha
            or (
                item_id in expected_deleted_part_missing
                and normalize_path(evidence["details"].get("target_path", ""))
                != expected_path
            )
        ):
            reconciliation_evidence_failures.append(item_id)
    assertions = {
        "queue_remains_paused": bool(paused and paused[0] == "1"),
        "zero_running_direct_jobs": running == 0,
        "queued_jobs_match_canonical_claims": queued == queued_claims,
        "zero_orphan_queued_jobs": orphan_queued == 0,
        "zero_stale_active_claims": stale_claims == 0,
        "database_quick_check_ok": quick_check == "ok",
        "zero_foreign_key_violations": foreign_keys == 0,
        "zero_part_files": len(part_files) == 0,
        "zero_zero_byte_mp4_files": len(zero_mp4) == 0,
        "zero_zero_byte_media_files": len(zero_media) == 0,
        "zero_non_regular_media_entries": len(non_regular_media) == 0,
        "zero_part_library_rows": len(part_library_rows) == 0,
        "zero_zero_byte_media_library_rows": len(zero_media_library_rows) == 0,
        "zero_non_regular_media_library_rows": (
            len(non_regular_media_library_rows) == 0
        ),
        "all_in_scope_missing_library_rows_reconciled": (
            observed_in_scope_missing == expected_reconciled_missing
        ),
        "all_missing_reconciliation_evidence_matches_pinned_source": (
            not reconciliation_evidence_failures
        ),
        "zero_root_scan_errors": len(scan_errors) == 0,
        "zero_physical_only_media": len(physical_only) == 0,
        "all_duplicate_actions_applied": duplicate_status == {
            "applied": len(duplicates["actions"])
        },
        "all_artifact_actions_applied": artifact_status == {
            "applied": len(artifacts["actions"])
        },
        "all_duplicate_sources_absent": not duplicate_source_failures,
        "all_duplicate_keepers_present_size_and_hash_matched": (
            not duplicate_keeper_failures
        ),
        "all_duplicate_destinations_present_size_and_hash_matched": (
            not duplicate_destination_failures
        ),
        "all_artifact_sources_absent": not artifact_source_failures,
        "all_artifact_destinations_present_size_and_hash_matched": (
            not artifact_destination_failures
        ),
        "all_artifact_evidence_quarantined": artifact_evidence_states == {
            "quarantined": len(artifacts["actions"])
        },
    }
    report = {
        "schema": OUTPUT_SCHEMA,
        "work_packet": "WP-0277",
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "root": str(root),
        "database": str(database),
        "sources": {
            "duplicate_manifest": {
                "path": str(duplicate_path),
                "sha256": duplicate_sha,
            },
            "artifact_result": {
                "path": str(artifact_path),
                "sha256": artifact_sha,
            },
            "path_reconcile_evidence": {
                "path": str(path_reconcile_path),
                "sha256": path_reconcile_sha,
            },
            "duplicate_ledger": str(duplicate_ledger_path),
            "artifact_ledger": str(artifact_ledger_path),
        },
        "assertions": assertions,
        "summary": {
            "assertions_passed": all(assertions.values()),
            "physical_nonzero_media_files": len(physical),
            "physical_only_media_files": len(physical_only),
            "library_rows": len(library_rows),
            "missing_library_rows": len(missing_library_rows),
            "in_scope_reconciled_missing_library_rows": len(
                in_scope_missing_library_rows
            ),
            "external_missing_library_rows": len(external_missing_library_rows),
            "zero_byte_library_rows": len(zero_library_rows),
            "zero_byte_media_library_rows": len(zero_media_library_rows),
            "non_regular_media_library_rows": len(
                non_regular_media_library_rows
            ),
            "duplicate_db_path_groups": duplicate_db_path_groups,
            "queued_direct_jobs": queued,
            "canonical_queued_claims": queued_claims,
            "part_files": len(part_files),
            "zero_byte_mp4_files": len(zero_mp4),
            "zero_byte_media_files": len(zero_media),
            "non_regular_media_entries": len(non_regular_media),
            "duplicate_quarantine_actions": len(duplicates["actions"]),
            "duplicate_keepers_hashed": len(duplicate_keeper_expectations),
            "artifact_quarantine_actions": len(artifacts["actions"]),
        },
        "details": {
            "part_files": part_files,
            "zero_byte_mp4_files": zero_mp4,
            "zero_byte_media_files": zero_media,
            "non_regular_media_entries": non_regular_media,
            "part_library_rows": part_library_rows,
            "zero_byte_library_rows": zero_library_rows,
            "zero_byte_media_library_rows": zero_media_library_rows,
            "non_regular_media_library_rows": non_regular_media_library_rows,
            "missing_library_rows": missing_library_rows,
            "in_scope_missing_library_rows": in_scope_missing_library_rows,
            "external_missing_library_rows": external_missing_library_rows,
            "expected_reconciled_missing_library_rows": expected_reconciled_missing,
            "reconciliation_evidence_failures": reconciliation_evidence_failures,
            "physical_only_media": physical_only,
            "root_scan_errors": scan_errors,
            "duplicate_action_statuses": duplicate_status,
            "artifact_action_statuses": artifact_status,
            "artifact_evidence_states": artifact_evidence_states,
            "duplicate_source_failures": duplicate_source_failures,
            "duplicate_keeper_failures": duplicate_keeper_failures,
            "duplicate_destination_failures": duplicate_destination_failures,
            "artifact_source_failures": artifact_source_failures,
            "artifact_destination_failures": artifact_destination_failures,
        },
    }
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    output_path.with_suffix(output_path.suffix + ".sha256").write_text(
        f"{sha256_file(output_path)}  {output_path.name}\n", encoding="utf-8"
    )
    print(json.dumps(report["summary"], sort_keys=True))
    return 0 if report["summary"]["assertions_passed"] else 2


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"ERROR: {type(error).__name__}: {error}", file=sys.stderr)
        raise

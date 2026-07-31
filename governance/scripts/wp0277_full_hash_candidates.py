#!/usr/bin/env python3
"""Full-SHA256 the remaining WP-0277 duplicate candidates with resume support."""

from __future__ import annotations

import argparse
import collections
import concurrent.futures
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import sys
import threading
import time
from typing import Any


INPUT_SCHEMA = "voxvulgi.wp0277.live_duplicate_candidate_manifest.v1"
ROW_SCHEMA = "voxvulgi.wp0277.full_hash_row.v1"
RESULT_SCHEMA = "voxvulgi.wp0277.full_hash_result.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidates", required=True)
    parser.add_argument("--journal", required=True)
    parser.add_argument("--result", required=True)
    parser.add_argument("--workers", type=int, default=2)
    return parser.parse_args()


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(8 * 1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


def load_candidates(path: Path) -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    with path.open("r", encoding="utf-8") as handle:
        manifest = json.load(handle)
    if manifest.get("schema") != INPUT_SCHEMA:
        raise RuntimeError(f"unexpected candidate schema: {manifest.get('schema')!r}")
    summary = manifest["summary"]
    if (
        summary["remaining_groups_to_full_hash"] != 1092
        or summary["remaining_unique_files_to_full_hash"] != 2198
        or summary["remaining_read_bytes_to_full_hash"] != 154_542_804_852
    ):
        raise RuntimeError(f"candidate summary mismatch: {summary}")
    files: dict[str, dict[str, Any]] = {}
    for group in manifest["remaining_candidate_groups"]:
        for candidate in group["candidates"]:
            key = candidate["normalized_path"]
            prior = files.get(key)
            if prior and (
                prior["size_bytes"] != candidate["size_bytes"]
                or prior["modified_ns"] != candidate["modified_ns"]
                or prior["path"].casefold() != candidate["path"].casefold()
            ):
                raise RuntimeError(f"conflicting candidate path evidence: {key}")
            files[key] = {
                "normalized_path": key,
                "path": candidate["path"],
                "size_bytes": int(candidate["size_bytes"]),
                "modified_ns": int(candidate["modified_ns"]),
            }
    if len(files) != 2198:
        raise RuntimeError(f"candidate file count mismatch: {len(files)}")
    return manifest, files


def load_completed(journal: Path) -> dict[str, dict[str, Any]]:
    rows: dict[str, dict[str, Any]] = {}
    if not journal.is_file():
        return rows
    with journal.open("r", encoding="utf-8") as handle:
        for raw in handle:
            try:
                row = json.loads(raw)
            except json.JSONDecodeError:
                continue
            if (
                row.get("schema") == ROW_SCHEMA
                and row.get("event") == "hashed"
                and row.get("status") == "ok"
            ):
                rows[row["normalized_path"]] = row
    return rows


def hash_one(file_row: dict[str, Any]) -> dict[str, Any]:
    started = time.monotonic()
    row = {
        "schema": ROW_SCHEMA,
        "event": "hashed",
        **file_row,
        "status": "error",
        "observed_size_bytes": None,
        "observed_modified_ns": None,
        "sha256": None,
        "error": None,
    }
    try:
        stat = os.stat(file_row["path"])
        row["observed_size_bytes"] = stat.st_size
        row["observed_modified_ns"] = stat.st_mtime_ns
        if stat.st_size != file_row["size_bytes"]:
            raise RuntimeError(
                f"size drift: {stat.st_size} != {file_row['size_bytes']}"
            )
        if stat.st_mtime_ns != file_row["modified_ns"]:
            raise RuntimeError(
                f"mtime drift: {stat.st_mtime_ns} != {file_row['modified_ns']}"
            )
        row["sha256"] = file_sha256(Path(file_row["path"]))
        after = os.stat(file_row["path"])
        if after.st_size != stat.st_size or after.st_mtime_ns != stat.st_mtime_ns:
            raise RuntimeError("file changed while hashing")
        row["status"] = "ok"
    except Exception as error:
        row["error"] = str(error)
    row["elapsed_ms"] = round((time.monotonic() - started) * 1000)
    row["finished_at_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    return row


def build_result(
    candidate_manifest: dict[str, Any],
    hashes: dict[str, dict[str, Any]],
    candidate_path: Path,
) -> dict[str, Any]:
    groups = []
    exact_group_count = 0
    exact_set_count = 0
    exact_member_files = 0
    redundant_files = 0
    reclaimable_bytes = 0
    split_groups = 0
    for group in candidate_manifest["remaining_candidate_groups"]:
        hash_buckets: dict[str, list[dict[str, Any]]] = collections.defaultdict(list)
        candidates = []
        for candidate in group["candidates"]:
            hash_row = hashes[candidate["normalized_path"]]
            if hash_row.get("status") != "ok":
                raise RuntimeError(f"candidate hash failed: {candidate['path']}")
            enriched = dict(candidate)
            enriched["sha256"] = hash_row["sha256"]
            candidates.append(enriched)
            hash_buckets[hash_row["sha256"]].append(enriched)
        exact_sets = []
        for digest, members in sorted(hash_buckets.items()):
            if len(members) < 2:
                continue
            exact_sets.append(
                {
                    "sha256": digest,
                    "size_bytes": group["candidate_key"]["size_bytes"],
                    "member_count": len(members),
                    "reclaimable_bytes": (
                        group["candidate_key"]["size_bytes"] * (len(members) - 1)
                    ),
                    "members": members,
                }
            )
        if exact_sets:
            exact_group_count += 1
            exact_set_count += len(exact_sets)
            exact_member_files += sum(row["member_count"] for row in exact_sets)
            redundant_files += sum(row["member_count"] - 1 for row in exact_sets)
            reclaimable_bytes += sum(row["reclaimable_bytes"] for row in exact_sets)
        if len(hash_buckets) > 1:
            split_groups += 1
        groups.append(
            {
                "group_id": group["group_id"],
                "candidate_key": group["candidate_key"],
                "media_id": group["media_id"],
                "service": group["service"],
                "candidate_count": group["candidate_count"],
                "distinct_hashes": len(hash_buckets),
                "resolution": "exact_duplicate" if exact_sets else "hash_split",
                "exact_sets": exact_sets,
                "candidates": candidates,
                "source_identity_evidence": group["source_identity_evidence"],
            }
        )
    return {
        "schema": RESULT_SCHEMA,
        "work_packet": "WP-0277",
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "candidate_manifest": {
            "path": str(candidate_path),
            "sha256": file_sha256(candidate_path),
        },
        "summary": {
            "hashed_groups": len(groups),
            "hashed_unique_files": len(hashes),
            "hashed_read_bytes": sum(
                row["size_bytes"] for row in hashes.values()
            ),
            "groups_with_exact_duplicates": exact_group_count,
            "exact_duplicate_sets": exact_set_count,
            "exact_member_files": exact_member_files,
            "redundant_files": redundant_files,
            "reclaimable_bytes": reclaimable_bytes,
            "hash_split_groups": split_groups,
            "hash_errors": 0,
        },
        "groups": groups,
    }


def main() -> int:
    args = parse_args()
    candidate_path = Path(args.candidates)
    journal_path = Path(args.journal)
    result_path = Path(args.result)
    workers = max(1, min(args.workers, 4))
    manifest, files = load_candidates(candidate_path)
    completed = load_completed(journal_path)
    pending = [
        row
        for key, row in files.items()
        if not (
            key in completed
            and completed[key].get("observed_size_bytes") == row["size_bytes"]
            and completed[key].get("observed_modified_ns") == row["modified_ns"]
        )
    ]
    journal_path.parent.mkdir(parents=True, exist_ok=True)
    print(
        json.dumps(
            {
                "event": "start",
                "files": len(files),
                "already_complete": len(files) - len(pending),
                "pending": len(pending),
                "read_bytes": sum(row["size_bytes"] for row in pending),
                "workers": workers,
                "journal": str(journal_path),
            }
        ),
        flush=True,
    )
    counts = {"ok": len(files) - len(pending), "error": 0}
    bytes_done = sum(
        row["size_bytes"]
        for key, row in files.items()
        if key not in {pending_row["normalized_path"] for pending_row in pending}
    )
    lock = threading.Lock()
    started = time.monotonic()
    with journal_path.open("a", encoding="utf-8", newline="\n") as journal:
        with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as executor:
            futures = [executor.submit(hash_one, row) for row in pending]
            for index, future in enumerate(
                concurrent.futures.as_completed(futures),
                start=1,
            ):
                row = future.result()
                with lock:
                    journal.write(
                        json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n"
                    )
                    journal.flush()
                counts[row["status"]] += 1
                if row["status"] == "ok":
                    bytes_done += row["size_bytes"]
                    completed[row["normalized_path"]] = row
                if index % 25 == 0 or index == len(pending):
                    print(
                        json.dumps(
                            {
                                "event": "progress",
                                "completed_this_run": index,
                                "pending_this_run": len(pending),
                                "ok": counts["ok"],
                                "error": counts["error"],
                                "bytes_done": bytes_done,
                                "bytes_total": 154_542_804_852,
                            }
                        ),
                        flush=True,
                    )
    if counts["error"] or len(completed) != len(files):
        print(
            json.dumps(
                {
                    "event": "incomplete",
                    **counts,
                    "completed_unique": len(completed),
                    "expected_unique": len(files),
                }
            ),
            flush=True,
        )
        return 2
    result = build_result(manifest, completed, candidate_path)
    result_path.parent.mkdir(parents=True, exist_ok=True)
    blob = (
        json.dumps(result, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
    ).encode("utf-8")
    partial = result_path.with_suffix(result_path.suffix + ".partial")
    partial.write_bytes(blob)
    os.replace(partial, result_path)
    result_sha = hashlib.sha256(blob).hexdigest().upper()
    result_path.with_suffix(result_path.suffix + ".sha256").write_text(
        f"{result_sha}  {result_path.name}\n",
        encoding="ascii",
    )
    print(
        json.dumps(
            {
                "event": "complete",
                "elapsed_seconds": round(time.monotonic() - started, 3),
                "result": str(result_path),
                "sha256": result_sha,
                "summary": result["summary"],
            }
        ),
        flush=True,
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"ERROR: {error}", file=sys.stderr, flush=True)
        raise

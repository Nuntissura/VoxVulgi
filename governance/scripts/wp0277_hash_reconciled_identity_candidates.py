#!/usr/bin/env python3
"""Full-hash same-size candidates discovered by WP-0277 path reconciliation."""

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
import time
from typing import Any


INPUT_SCHEMA = "voxvulgi.wp0277.reconciled_identity_candidate_manifest.v1"
ROW_SCHEMA = "voxvulgi.wp0277.reconciled_identity_hash_row.v1"
RESULT_SCHEMA = "voxvulgi.wp0277.reconciled_identity_hash_result.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidates", required=True)
    parser.add_argument("--journal", required=True)
    parser.add_argument("--result", required=True)
    parser.add_argument("--reuse-journal", action="append", default=[])
    parser.add_argument("--workers", type=int, default=2)
    return parser.parse_args()


def normalize_path(value: str) -> str:
    value = value.replace("/", "\\")
    if value.casefold().startswith("\\\\?\\unc\\"):
        value = "\\\\" + value[8:]
    return value.casefold()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(8 * 1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


def load_manifest(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle)
    if value.get("schema") != INPUT_SCHEMA:
        raise RuntimeError(f"unexpected schema: {value.get('schema')!r}")
    return value


def required_candidates(
    manifest: dict[str, Any],
) -> tuple[dict[str, dict[str, Any]], set[str]]:
    files: dict[str, dict[str, Any]] = {}
    hashed_group_ids: set[str] = set()
    for group in manifest["groups"]:
        buckets: dict[int, list[dict[str, Any]]] = collections.defaultdict(list)
        for candidate in group["candidates"]:
            buckets[int(candidate["size_bytes"])].append(candidate)
        selected = [
            candidate
            for members in buckets.values()
            if len(members) > 1
            for candidate in members
        ]
        if not selected:
            continue
        hashed_group_ids.add(group["group_id"])
        for candidate in selected:
            files[candidate["normalized_path"]] = {
                "normalized_path": candidate["normalized_path"],
                "path": candidate["path"],
                "size_bytes": int(candidate["size_bytes"]),
                "modified_ns": int(candidate["modified_ns"]),
            }
    expected = {
        "hashed_groups": 567,
        "unique_files": 1272,
        "read_bytes": 226_968_149_694,
    }
    actual = {
        "hashed_groups": len(hashed_group_ids),
        "unique_files": len(files),
        "read_bytes": sum(row["size_bytes"] for row in files.values()),
    }
    if actual != expected:
        raise RuntimeError(f"same-size candidate mismatch: {actual} != {expected}")
    return files, hashed_group_ids


def load_hash_rows(paths: list[Path]) -> dict[str, dict[str, Any]]:
    rows: dict[str, dict[str, Any]] = {}
    for path in paths:
        if not path.is_file():
            continue
        with path.open("r", encoding="utf-8") as handle:
            for raw in handle:
                try:
                    row = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                digest = row.get("sha256")
                source_path = row.get("path")
                if not digest or not source_path:
                    continue
                if row.get("status") not in (None, "ok"):
                    continue
                rows[row.get("normalized_path") or normalize_path(source_path)] = row
    return rows


def reusable_hash(
    candidate: dict[str, Any],
    prior: dict[str, Any] | None,
) -> dict[str, Any] | None:
    if not prior:
        return None
    prior_size = prior.get("observed_size_bytes", prior.get("size_bytes"))
    prior_mtime = prior.get("observed_modified_ns", prior.get("modified_ns"))
    if prior_size is not None and int(prior_size) != candidate["size_bytes"]:
        return None
    if prior_mtime is not None and int(prior_mtime) != candidate["modified_ns"]:
        return None
    stat = os.stat(candidate["path"])
    if (
        stat.st_size != candidate["size_bytes"]
        or stat.st_mtime_ns != candidate["modified_ns"]
    ):
        return None
    return {
        "schema": ROW_SCHEMA,
        "event": "hashed",
        **candidate,
        "observed_size_bytes": stat.st_size,
        "observed_modified_ns": stat.st_mtime_ns,
        "sha256": prior["sha256"],
        "status": "ok",
        "error": None,
        "reused": True,
        "finished_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
    }


def hash_one(candidate: dict[str, Any]) -> dict[str, Any]:
    started = time.monotonic()
    row = {
        "schema": ROW_SCHEMA,
        "event": "hashed",
        **candidate,
        "observed_size_bytes": None,
        "observed_modified_ns": None,
        "sha256": None,
        "status": "error",
        "error": None,
        "reused": False,
    }
    try:
        stat = os.stat(candidate["path"])
        row["observed_size_bytes"] = stat.st_size
        row["observed_modified_ns"] = stat.st_mtime_ns
        if (
            stat.st_size != candidate["size_bytes"]
            or stat.st_mtime_ns != candidate["modified_ns"]
        ):
            raise RuntimeError("candidate size or mtime changed")
        row["sha256"] = sha256_file(Path(candidate["path"]))
        after = os.stat(candidate["path"])
        if after.st_size != stat.st_size or after.st_mtime_ns != stat.st_mtime_ns:
            raise RuntimeError("candidate changed while hashing")
        row["status"] = "ok"
    except Exception as error:
        row["error"] = str(error)
    row["elapsed_ms"] = round((time.monotonic() - started) * 1000)
    row["finished_at_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    return row


def build_result(
    manifest: dict[str, Any],
    hashes: dict[str, dict[str, Any]],
    manifest_path: Path,
) -> dict[str, Any]:
    result_groups = []
    exact_sets = 0
    exact_members = 0
    redundant = 0
    reclaimable = 0
    hash_split_groups = 0
    size_split_groups = 0
    for group in manifest["groups"]:
        size_buckets: dict[int, list[dict[str, Any]]] = collections.defaultdict(list)
        for candidate in group["candidates"]:
            size_buckets[int(candidate["size_bytes"])].append(candidate)
        group_sets = []
        hashed_any = False
        distinct_hashes = 0
        for size, candidates in sorted(size_buckets.items()):
            if len(candidates) < 2:
                continue
            hashed_any = True
            digest_buckets: dict[str, list[dict[str, Any]]] = collections.defaultdict(list)
            for candidate in candidates:
                row = hashes[candidate["normalized_path"]]
                enriched = dict(candidate)
                enriched["sha256"] = row["sha256"]
                digest_buckets[row["sha256"]].append(enriched)
            distinct_hashes += len(digest_buckets)
            for digest, members in sorted(digest_buckets.items()):
                if len(members) < 2:
                    continue
                group_sets.append(
                    {
                        "sha256": digest,
                        "size_bytes": size,
                        "member_count": len(members),
                        "reclaimable_bytes": size * (len(members) - 1),
                        "members": members,
                    }
                )
        if not hashed_any:
            resolution = "size_split"
            size_split_groups += 1
        elif group_sets:
            resolution = "exact_duplicate"
        else:
            resolution = "hash_split"
            hash_split_groups += 1
        exact_sets += len(group_sets)
        exact_members += sum(row["member_count"] for row in group_sets)
        redundant += sum(row["member_count"] - 1 for row in group_sets)
        reclaimable += sum(row["reclaimable_bytes"] for row in group_sets)
        result_groups.append(
            {
                "group_id": group["group_id"],
                "media_id": group["media_id"],
                "identity_state": group["identity_state"],
                "resolution": resolution,
                "distinct_hashes": distinct_hashes,
                "exact_sets": group_sets,
            }
        )
    return {
        "schema": RESULT_SCHEMA,
        "work_packet": "WP-0277",
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "candidate_manifest": {
            "path": str(manifest_path),
            "sha256": sha256_file(manifest_path),
        },
        "summary": {
            "identity_groups": len(result_groups),
            "hashed_same_size_groups": 567,
            "hashed_unique_files": len(hashes),
            "hashed_read_bytes": sum(row["size_bytes"] for row in hashes.values()),
            "size_split_groups": size_split_groups,
            "hash_split_groups": hash_split_groups,
            "exact_duplicate_sets": exact_sets,
            "exact_member_files": exact_members,
            "redundant_files": redundant,
            "reclaimable_bytes": reclaimable,
            "hash_errors": 0,
        },
        "groups": result_groups,
    }


def main() -> int:
    args = parse_args()
    candidate_path = Path(args.candidates)
    journal_path = Path(args.journal)
    result_path = Path(args.result)
    manifest = load_manifest(candidate_path)
    files, _ = required_candidates(manifest)
    prior = load_hash_rows(
        [journal_path] + [Path(value) for value in args.reuse_journal]
    )
    completed: dict[str, dict[str, Any]] = {}
    for key, candidate in files.items():
        reusable = reusable_hash(candidate, prior.get(key))
        if reusable:
            completed[key] = reusable
    pending = [row for key, row in files.items() if key not in completed]
    journal_path.parent.mkdir(parents=True, exist_ok=True)
    with journal_path.open("a", encoding="utf-8", newline="\n") as journal:
        for row in completed.values():
            if row["reused"]:
                journal.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")
        journal.flush()
        print(
            json.dumps(
                {
                    "event": "start",
                    "files": len(files),
                    "reused": len(completed),
                    "pending": len(pending),
                    "pending_read_bytes": sum(row["size_bytes"] for row in pending),
                    "workers": args.workers,
                }
            ),
            flush=True,
        )
        errors = 0
        bytes_done = sum(row["size_bytes"] for row in completed.values())
        with concurrent.futures.ThreadPoolExecutor(
            max_workers=max(1, min(args.workers, 4))
        ) as executor:
            futures = [executor.submit(hash_one, row) for row in pending]
            for index, future in enumerate(
                concurrent.futures.as_completed(futures),
                start=1,
            ):
                row = future.result()
                journal.write(
                    json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n"
                )
                journal.flush()
                if row["status"] == "ok":
                    completed[row["normalized_path"]] = row
                    bytes_done += row["size_bytes"]
                else:
                    errors += 1
                if index % 25 == 0 or index == len(pending):
                    print(
                        json.dumps(
                            {
                                "event": "progress",
                                "completed_this_run": index,
                                "pending_this_run": len(pending),
                                "completed_unique": len(completed),
                                "errors": errors,
                                "bytes_done": bytes_done,
                                "bytes_total": 226_968_149_694,
                            }
                        ),
                        flush=True,
                    )
    if errors or len(completed) != len(files):
        return 2
    result = build_result(manifest, completed, candidate_path)
    blob = (
        json.dumps(result, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
    ).encode("utf-8")
    partial = result_path.with_suffix(result_path.suffix + ".partial")
    partial.write_bytes(blob)
    os.replace(partial, result_path)
    digest = hashlib.sha256(blob).hexdigest().upper()
    result_path.with_suffix(result_path.suffix + ".sha256").write_text(
        f"{digest}  {result_path.name}\n",
        encoding="ascii",
    )
    print(
        json.dumps(
            {
                "event": "complete",
                "result": str(result_path),
                "sha256": digest,
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

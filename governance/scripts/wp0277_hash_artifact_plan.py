#!/usr/bin/env python3
"""Full-hash every WP-0277 cleanup-artifact action before quarantine."""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import sys
import time
from typing import Any


INPUT_SCHEMA = "voxvulgi.cleanup_artifact_quarantine_plan.v1"
ROW_SCHEMA = "voxvulgi.wp0277.cleanup_artifact_hash_row.v1"
RESULT_SCHEMA = "voxvulgi.wp0277.cleanup_artifact_hash_result.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--plan", required=True)
    parser.add_argument("--journal", required=True)
    parser.add_argument("--result", required=True)
    parser.add_argument("--workers", type=int, default=1)
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


def hash_action(action: dict[str, Any], plan_sha256: str) -> dict[str, Any]:
    source = Path(action["source_path"])
    started = time.time()
    try:
        before = source.stat()
        if (
            before.st_size != int(action["observed_size_bytes"])
            or before.st_mtime_ns != int(action["observed_modified_ns"])
        ):
            raise RuntimeError("source stat changed before hashing")
        digest = sha256_file(source)
        after = source.stat()
        if before.st_size != after.st_size or before.st_mtime_ns != after.st_mtime_ns:
            raise RuntimeError("source stat changed during hashing")
        return {
            "schema": ROW_SCHEMA,
            "plan_sha256": plan_sha256,
            "action_id": action["action_id"],
            "source_path": action["source_path"],
            "normalized_source_path": action["normalized_source_path"],
            "size_bytes": after.st_size,
            "modified_ns": after.st_mtime_ns,
            "full_sha256": digest,
            "status": "ok",
            "error": None,
            "elapsed_seconds": round(time.time() - started, 6),
        }
    except Exception as error:
        return {
            "schema": ROW_SCHEMA,
            "plan_sha256": plan_sha256,
            "action_id": action["action_id"],
            "source_path": action["source_path"],
            "normalized_source_path": action["normalized_source_path"],
            "size_bytes": int(action["observed_size_bytes"]),
            "modified_ns": int(action["observed_modified_ns"]),
            "full_sha256": None,
            "status": "error",
            "error": f"{type(error).__name__}: {error}",
            "elapsed_seconds": round(time.time() - started, 6),
        }


def load_journal(
    path: Path,
    actions: dict[str, dict[str, Any]],
    plan_sha256: str,
) -> dict[str, dict[str, Any]]:
    rows: dict[str, dict[str, Any]] = {}
    if not path.exists():
        return rows
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            row = json.loads(line)
            if row.get("schema") != ROW_SCHEMA:
                raise RuntimeError(
                    f"unexpected journal schema on line {line_number}: {row.get('schema')!r}"
                )
            action = actions.get(row.get("action_id"))
            if (
                action is None
                or row.get("plan_sha256") != plan_sha256
                or row.get("status") != "ok"
                or row.get("source_path") != action["source_path"]
                or row.get("normalized_source_path")
                != action["normalized_source_path"]
                or int(row.get("size_bytes", -1))
                != int(action["observed_size_bytes"])
                or int(row.get("modified_ns", -1))
                != int(action["observed_modified_ns"])
            ):
                continue
            try:
                stat = os.stat(action["source_path"])
            except OSError:
                continue
            if (
                stat.st_size != int(action["observed_size_bytes"])
                or stat.st_mtime_ns != int(action["observed_modified_ns"])
            ):
                continue
            rows[row["action_id"]] = row
    return rows


def write_sidecar(path: Path) -> str:
    digest = hashlib.sha256(path.read_bytes()).hexdigest().upper()
    path.with_suffix(path.suffix + ".sha256").write_text(
        f"{digest}  {path.name}\n", encoding="utf-8"
    )
    return digest


def main() -> int:
    args = parse_args()
    plan_path = Path(args.plan)
    journal_path = Path(args.journal)
    result_path = Path(args.result)
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    if plan.get("schema") != INPUT_SCHEMA:
        raise RuntimeError(f"unexpected plan schema: {plan.get('schema')!r}")
    actions = plan["actions"]
    plan_sha256 = hashlib.sha256(plan_path.read_bytes()).hexdigest().upper()
    if len(actions) != 435:
        raise RuntimeError(f"unexpected artifact action count: {len(actions)}")
    if len({row["action_id"] for row in actions}) != len(actions):
        raise RuntimeError("duplicate artifact action ids")
    if len({normalize_path(row["source_path"]) for row in actions}) != len(actions):
        raise RuntimeError("duplicate artifact source paths")

    completed = load_journal(
        journal_path,
        {row["action_id"]: row for row in actions},
        plan_sha256,
    )
    pending = [
        action
        for action in actions
        if completed.get(action["action_id"], {}).get("status") != "ok"
    ]
    journal_path.parent.mkdir(parents=True, exist_ok=True)
    print(
        json.dumps(
            {
                "event": "start",
                "files": len(actions),
                "reused": len(actions) - len(pending),
                "pending": len(pending),
                "pending_read_bytes": sum(
                    int(row["observed_size_bytes"]) for row in pending
                ),
                "workers": max(1, args.workers),
            }
        ),
        flush=True,
    )
    mode = "a" if journal_path.exists() else "w"
    with journal_path.open(mode, encoding="utf-8", newline="\n") as journal:
        with concurrent.futures.ThreadPoolExecutor(
            max_workers=max(1, args.workers)
        ) as pool:
            futures = {
                pool.submit(hash_action, action, plan_sha256): action
                for action in pending
            }
            for completed_count, future in enumerate(
                concurrent.futures.as_completed(futures), 1
            ):
                row = future.result()
                completed[row["action_id"]] = row
                journal.write(json.dumps(row, sort_keys=True) + "\n")
                journal.flush()
                os.fsync(journal.fileno())
                print(
                    json.dumps(
                        {
                            "event": "progress",
                            "completed": completed_count,
                            "pending_total": len(pending),
                            "action_id": row["action_id"],
                            "status": row["status"],
                            "size_bytes": row["size_bytes"],
                        }
                    ),
                    flush=True,
                )

    missing = [
        action["action_id"]
        for action in actions
        if action["action_id"] not in completed
    ]
    if missing:
        raise RuntimeError(f"journal missing {len(missing)} actions")
    result_actions = []
    errors = 0
    for action in actions:
        hashed = completed[action["action_id"]]
        if hashed["status"] != "ok":
            errors += 1
        result_actions.append(
            {
                **action,
                "full_sha256": hashed["full_sha256"],
                "hash_status": hashed["status"],
                "hash_error": hashed["error"],
                "state": (
                    "full_hash_verified"
                    if hashed["status"] == "ok"
                    else "hash_attention"
                ),
            }
        )
    result = {
        "schema": RESULT_SCHEMA,
        "work_packet": "WP-0277",
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "source_plan": {
            "path": str(plan_path),
            "sha256": plan_sha256,
        },
        "summary": {
            "actions": len(actions),
            "hashed_files": len(actions) - errors,
            "hash_errors": errors,
            "hashed_bytes": sum(
                int(row["observed_size_bytes"])
                for row in actions
                if completed[row["action_id"]]["status"] == "ok"
            ),
        },
        "actions": result_actions,
    }
    result_path.parent.mkdir(parents=True, exist_ok=True)
    result_path.write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    result_sha = write_sidecar(result_path)
    journal_sha = write_sidecar(journal_path)
    print(
        json.dumps(
            {
                "event": "complete",
                "result": str(result_path),
                "result_sha256": result_sha,
                "journal_sha256": journal_sha,
                **result["summary"],
            }
        ),
        flush=True,
    )
    return 0 if errors == 0 else 2


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"ERROR: {type(error).__name__}: {error}", file=sys.stderr)
        raise

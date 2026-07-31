#!/usr/bin/env python3
"""Validate WP-0277 physical-only media with ffprobe.

The output is an append-only JSONL receipt. Re-running the command resumes from
successful rows whose size and mtime still match the source evidence.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import time
from typing import Any


SCHEMA = "voxvulgi.path_reconcile_probe.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--timeout-seconds", type=int, default=45)
    return parser.parse_args()


def load_targets(evidence_path: Path) -> list[dict[str, Any]]:
    with evidence_path.open("r", encoding="utf-8") as handle:
        evidence = json.load(handle)
    if evidence.get("schema") != "voxvulgi.path_reconcile_evidence.v1":
        raise RuntimeError(f"unexpected evidence schema: {evidence.get('schema')!r}")
    records = (
        evidence["physical_only"]["canonical_identity_records"]
        + evidence["physical_only"]["unmatched_records"]
    )
    targets: list[dict[str, Any]] = []
    seen: set[str] = set()
    for record in records:
        file_row = record["file"]
        key = file_row["normalized_path"]
        if key in seen:
            raise RuntimeError(f"duplicate normalized path in evidence: {key}")
        seen.add(key)
        targets.append(
            {
                "path": file_row["path"],
                "normalized_path": key,
                "expected_size_bytes": int(file_row["size_bytes"]),
                "expected_modified_ns": int(file_row["modified_ns"]),
                "classification": (
                    "canonical_identity"
                    if "canonical_media_id" in record
                    else "unmatched"
                ),
                "canonical_media_id": record.get("canonical_media_id"),
            }
        )
    expected = evidence["summary"]["counts"]["physical_only_total"]
    if len(targets) != expected:
        raise RuntimeError(f"target count mismatch: {len(targets)} != {expected}")
    return targets


def load_completed(output_path: Path) -> dict[str, dict[str, Any]]:
    completed: dict[str, dict[str, Any]] = {}
    if not output_path.is_file():
        return completed
    with output_path.open("r", encoding="utf-8") as handle:
        for raw in handle:
            try:
                row = json.loads(raw)
            except json.JSONDecodeError:
                continue
            if row.get("event") != "probe" or row.get("status") != "ok":
                continue
            completed[row["normalized_path"]] = row
    return completed


def select_streams(payload: dict[str, Any]) -> dict[str, Any]:
    streams = payload.get("streams") or []
    video = next((row for row in streams if row.get("codec_type") == "video"), None)
    audio = next((row for row in streams if row.get("codec_type") == "audio"), None)
    format_row = payload.get("format") or {}
    duration_ms = None
    duration = format_row.get("duration")
    if duration not in (None, "", "N/A"):
        try:
            duration_ms = round(float(duration) * 1000)
        except (TypeError, ValueError):
            pass
    return {
        "duration_ms": duration_ms,
        "container": format_row.get("format_name"),
        "video_codec": video.get("codec_name") if video else None,
        "audio_codec": audio.get("codec_name") if audio else None,
        "width": video.get("width") if video else None,
        "height": video.get("height") if video else None,
        "stream_count": len(streams),
        "has_video": video is not None,
        "has_audio": audio is not None,
    }


def probe_one(
    target: dict[str, Any],
    ffprobe: str,
    timeout_seconds: int,
) -> dict[str, Any]:
    started = time.monotonic()
    row: dict[str, Any] = {
        "schema": SCHEMA,
        "event": "probe",
        **target,
        "status": "error",
        "observed_size_bytes": None,
        "observed_modified_ns": None,
        "probe": None,
        "error": None,
    }
    try:
        stat = os.stat(target["path"])
        row["observed_size_bytes"] = stat.st_size
        row["observed_modified_ns"] = stat.st_mtime_ns
        if stat.st_size != target["expected_size_bytes"]:
            raise RuntimeError(
                f"size drift: expected {target['expected_size_bytes']}, observed {stat.st_size}"
            )
        if stat.st_mtime_ns != target["expected_modified_ns"]:
            raise RuntimeError(
                "mtime drift: expected "
                f"{target['expected_modified_ns']}, observed {stat.st_mtime_ns}"
            )
        process = subprocess.run(
            [
                ffprobe,
                "-v",
                "error",
                "-show_entries",
                (
                    "format=format_name,duration:"
                    "stream=codec_type,codec_name,width,height"
                ),
                "-of",
                "json",
                target["path"],
            ],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=timeout_seconds,
            check=False,
            creationflags=(
                subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
            ),
        )
        if process.returncode != 0:
            detail = process.stderr.strip() or process.stdout.strip()
            raise RuntimeError(f"ffprobe exit {process.returncode}: {detail}")
        payload = json.loads(process.stdout)
        selected = select_streams(payload)
        if not selected["has_video"] and not selected["has_audio"]:
            raise RuntimeError("ffprobe found no video or audio streams")
        row["status"] = "ok"
        row["probe"] = selected
    except Exception as error:  # The receipt must retain every failed path.
        row["error"] = str(error)
    row["elapsed_ms"] = round((time.monotonic() - started) * 1000)
    row["finished_at_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    return row


def main() -> int:
    args = parse_args()
    evidence_path = Path(args.evidence)
    output_path = Path(args.output)
    workers = max(1, min(args.workers, 8))
    timeout_seconds = max(5, args.timeout_seconds)
    ffprobe = shutil.which("ffprobe")
    if not ffprobe:
        raise RuntimeError("ffprobe was not found on PATH")
    targets = load_targets(evidence_path)
    completed = load_completed(output_path)
    pending = [
        row
        for row in targets
        if not (
            row["normalized_path"] in completed
            and completed[row["normalized_path"]].get("observed_size_bytes")
            == row["expected_size_bytes"]
            and completed[row["normalized_path"]].get("observed_modified_ns")
            == row["expected_modified_ns"]
        )
    ]
    output_path.parent.mkdir(parents=True, exist_ok=True)
    lock = threading.Lock()
    counts = {"ok": len(targets) - len(pending), "error": 0}
    print(
        json.dumps(
            {
                "event": "start",
                "schema": SCHEMA,
                "targets": len(targets),
                "already_complete": counts["ok"],
                "pending": len(pending),
                "workers": workers,
                "ffprobe": ffprobe,
                "output": str(output_path),
            }
        ),
        flush=True,
    )
    started = time.monotonic()
    with output_path.open("a", encoding="utf-8", newline="\n") as output:
        with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as executor:
            futures = [
                executor.submit(probe_one, row, ffprobe, timeout_seconds)
                for row in pending
            ]
            for index, future in enumerate(
                concurrent.futures.as_completed(futures),
                start=1,
            ):
                result = future.result()
                with lock:
                    output.write(
                        json.dumps(result, ensure_ascii=False, sort_keys=True) + "\n"
                    )
                    output.flush()
                counts[result["status"]] += 1
                if index % 50 == 0 or index == len(pending):
                    print(
                        json.dumps(
                            {
                                "event": "progress",
                                "completed_this_run": index,
                                "pending_this_run": len(pending),
                                **counts,
                            }
                        ),
                        flush=True,
                    )
        summary = {
            "schema": SCHEMA,
            "event": "summary",
            "targets": len(targets),
            **counts,
            "elapsed_seconds": round(time.monotonic() - started, 3),
            "completed_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        }
        output.write(json.dumps(summary, sort_keys=True) + "\n")
        output.flush()
    print(json.dumps(summary), flush=True)
    return 0 if counts["error"] == 0 and counts["ok"] == len(targets) else 2


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"ERROR: {error}", file=sys.stderr, flush=True)
        raise

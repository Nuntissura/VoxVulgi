#!/usr/bin/env python3
"""Read-only WP-0286 reconciliation of one VoxVulgi library root.

The report distinguishes canonical subscription membership from physical placement. It never
creates subscriptions, updates SQLite, moves media, or deletes empty directories.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import sqlite3
from collections import Counter, defaultdict
from pathlib import Path, PureWindowsPath
from typing import Any


VIDEO_EXTENSIONS = {
    ".mp4",
    ".mkv",
    ".webm",
    ".mov",
    ".avi",
    ".m4v",
    ".flv",
    ".wmv",
    ".ts",
    ".mts",
    ".m2ts",
}


def normalize_windows_path(value: str) -> str:
    normalized = value.strip().replace("/", "\\").rstrip("\\").lower()
    if normalized.startswith("\\\\?\\unc\\"):
        normalized = "\\\\" + normalized[8:]
    elif normalized.startswith("\\\\?\\"):
        normalized = normalized[4:]
    return normalized


def path_under_root(value: str, root: str) -> bool:
    path_key = normalize_windows_path(value)
    root_key = normalize_windows_path(root)
    return path_key == root_key or path_key.startswith(root_key + "\\")


def top_folder(value: str, root: str) -> str | None:
    if not path_under_root(value, root):
        return None
    path_parts = PureWindowsPath(value).parts
    root_parts = PureWindowsPath(root).parts
    if len(path_parts) <= len(root_parts):
        return None
    return path_parts[len(root_parts)]


def resolved_target(
    row: sqlite3.Row,
    libraries: dict[str, str],
    active_library_root: str,
) -> tuple[str, str]:
    override = (row["output_dir_override"] or "").strip()
    if override:
        return override, "output_dir_override"
    library_root = libraries.get((row["library_id"] or "").strip(), active_library_root)
    return str(PureWindowsPath(library_root) / row["folder_map"]), (
        "library_id" if row["library_id"] else "active_library"
    )


def contains_video(folder: Path) -> bool:
    try:
        for current_root, _, files in os.walk(folder):
            for name in files:
                if Path(name).suffix.lower() in VIDEO_EXTENSIONS:
                    return True
    except OSError:
        return False
    return False


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", required=True, help="VoxVulgi app.sqlite path")
    parser.add_argument("--root", required=True, help="Current library root")
    parser.add_argument("--output", required=True, help="JSON report output path")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    db_path = Path(args.db)
    root = args.root
    output_path = Path(args.output)
    if not db_path.is_file():
        raise SystemExit(f"database not found: {db_path}")
    if not Path(root).is_dir():
        raise SystemExit(f"library root not reachable: {root}")

    before_db = db_path.stat()
    connection = sqlite3.connect(
        f"file:{db_path.as_posix()}?mode=ro",
        uri=True,
        timeout=3,
    )
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA query_only=ON")

    libraries = {
        row["id"]: row["root_path"]
        for row in connection.execute(
            "SELECT id, root_path FROM video_library WHERE active=1"
        )
    }
    active_library_id_row = connection.execute(
        "SELECT value FROM meta WHERE key='active_video_library_id'"
    ).fetchone()
    active_library_id = active_library_id_row[0] if active_library_id_row else None
    active_library_root = (
        libraries.get(active_library_id or "")
        or next(iter(libraries.values()), root)
    )

    subscriptions = list(
        connection.execute(
            """
SELECT id, title, source_url, folder_map, output_dir_override, library_id,
       active, source_status
FROM youtube_subscription
ORDER BY title COLLATE NOCASE, id
"""
        )
    )
    target_subscriptions: dict[str, list[str]] = defaultdict(list)
    subscription_rows: list[dict[str, Any]] = []
    subscription_by_id: dict[str, dict[str, Any]] = {}
    for row in subscriptions:
        target, target_basis = resolved_target(row, libraries, active_library_root)
        target_key = normalize_windows_path(target)
        target_subscriptions[target_key].append(row["id"])
        projected_library_target = None
        no_path_change_normalization = False
        if row["output_dir_override"] and row["library_id"] in libraries:
            projected_library_target = str(
                PureWindowsPath(libraries[row["library_id"]]) / row["folder_map"]
            )
            no_path_change_normalization = (
                normalize_windows_path(projected_library_target) == target_key
            )
        item = {
            "id": row["id"],
            "title": row["title"],
            "source_url": row["source_url"],
            "active": bool(row["active"]),
            "source_status": row["source_status"],
            "folder_map": row["folder_map"],
            "output_dir_override": row["output_dir_override"],
            "library_id": row["library_id"],
            "effective_target": target,
            "target_basis": target_basis,
            "target_exists": Path(target).is_dir(),
            "projected_library_target": projected_library_target,
            "no_path_change_normalization_candidate": no_path_change_normalization,
            "membership_items": 0,
            "items_in_effective_target": 0,
            "items_outside_effective_target": 0,
            "physical_top_folders": [],
        }
        subscription_rows.append(item)
        subscription_by_id[row["id"]] = item

    folder_item_counts: Counter[str] = Counter()
    root_library_items = 0
    # Stored paths include both ordinary UNC and Windows extended `\\?\UNC\...` spellings.
    # Scan the canonical path column once and normalize in one place; a SQL prefix predicate would
    # silently exclude the extended form and make every downstream folder count false.
    for row in connection.execute("SELECT media_path FROM library_item"):
        if not path_under_root(row["media_path"], root):
            continue
        root_library_items += 1
        folder = top_folder(row["media_path"], root)
        if folder:
            folder_item_counts[folder.lower()] += 1

    membership_locations: dict[str, Counter[str]] = defaultdict(Counter)
    for row in connection.execute(
        """
SELECT m.source_subscription_id, li.media_path
FROM media_source_membership m
JOIN media_source_identity i
  ON i.service=m.service AND i.media_id=m.media_id
JOIN library_item li ON li.id=i.library_item_id
WHERE m.source_subscription_id IS NOT NULL
"""
    ):
        subscription_id = row["source_subscription_id"]
        sub = subscription_by_id.get(subscription_id)
        if sub is None:
            continue
        sub["membership_items"] += 1
        if path_under_root(row["media_path"], sub["effective_target"]):
            sub["items_in_effective_target"] += 1
        else:
            sub["items_outside_effective_target"] += 1
        folder = top_folder(row["media_path"], root)
        membership_locations[subscription_id][folder or "__outside_root__"] += 1

    for sub in subscription_rows:
        sub["physical_top_folders"] = [
            {"folder": folder, "items": count}
            for folder, count in membership_locations[sub["id"]].most_common()
        ]

    physical_folders = sorted(
        [entry for entry in Path(root).iterdir() if entry.is_dir()],
        key=lambda value: value.name.lower(),
    )
    unmatched_physical = [
        folder
        for folder in physical_folders
        if normalize_windows_path(str(folder)) not in target_subscriptions
    ]
    unmatched_no_library_rows = [
        folder
        for folder in unmatched_physical
        if folder_item_counts[folder.name.lower()] == 0
    ]
    no_video_folders = [
        str(folder)
        for folder in unmatched_no_library_rows
        if not contains_video(folder)
    ]

    counts = {
        "library_items_total": connection.execute(
            "SELECT COUNT(*) FROM library_item"
        ).fetchone()[0],
        "library_items_under_root": root_library_items,
        "youtube_subscriptions_total": len(subscription_rows),
        "youtube_subscriptions_active": sum(1 for row in subscription_rows if row["active"]),
        "subscription_targets_missing": sum(
            1 for row in subscription_rows if not row["target_exists"]
        ),
        "physical_top_level_folders": len(physical_folders),
        "physical_folders_not_subscription_targets": len(unmatched_physical),
        "unmatched_physical_folders_without_library_rows": len(
            unmatched_no_library_rows
        ),
        "unmatched_no_library_folders_without_video_files": len(no_video_folders),
        "subscriptions_with_membership_items_outside_target": sum(
            1 for row in subscription_rows if row["items_outside_effective_target"] > 0
        ),
        "targets_shared_by_multiple_subscriptions": sum(
            1 for ids in target_subscriptions.values() if len(ids) > 1
        ),
        "no_path_change_normalization_candidates": sum(
            1
            for row in subscription_rows
            if row["no_path_change_normalization_candidate"]
        ),
        "youtube_identities": connection.execute(
            "SELECT COUNT(*) FROM media_source_identity WHERE service='youtube'"
        ).fetchone()[0],
        "linked_youtube_identities": connection.execute(
            "SELECT COUNT(*) FROM media_source_identity WHERE service='youtube' AND library_item_id IS NOT NULL"
        ).fetchone()[0],
        "unlinked_youtube_identities": connection.execute(
            "SELECT COUNT(*) FROM media_source_identity WHERE service='youtube' AND library_item_id IS NULL"
        ).fetchone()[0],
        "source_memberships": connection.execute(
            "SELECT COUNT(*) FROM media_source_membership"
        ).fetchone()[0],
        "download_lineage_rows": connection.execute(
            "SELECT COUNT(*) FROM library_download_lineage"
        ).fetchone()[0],
    }
    connection.close()
    after_db = db_path.stat()
    if before_db.st_size != after_db.st_size or before_db.st_mtime_ns != after_db.st_mtime_ns:
        raise SystemExit("read-only reconciliation observed a database size/mtime change")

    report = {
        "wp": "WP-0286",
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "mode": "read_only_no_media_mutation",
        "database": str(db_path),
        "library_root": root,
        "active_video_library_id": active_library_id,
        "active_video_library_root": active_library_root,
        "counts": counts,
        "missing_subscription_targets": [
            row for row in subscription_rows if not row["target_exists"]
        ],
        "shared_subscription_targets": [
            {"target": target, "subscription_ids": ids}
            for target, ids in sorted(target_subscriptions.items())
            if len(ids) > 1
        ],
        "subscriptions_with_split_physical_locations": [
            row
            for row in subscription_rows
            if row["items_outside_effective_target"] > 0
        ],
        "no_path_change_normalization_candidates": [
            row
            for row in subscription_rows
            if row["no_path_change_normalization_candidate"]
        ],
        "physical_folders_not_subscription_targets": [
            {
                "path": str(folder),
                "indexed_library_items": folder_item_counts[folder.name.lower()],
            }
            for folder in unmatched_physical
        ],
        "unmatched_physical_folders_without_library_rows": [
            str(folder) for folder in unmatched_no_library_rows
        ],
        "unmatched_no_library_folders_without_video_files": no_video_folders,
        "safety": {
            "database_opened_read_only": True,
            "database_size_and_mtime_unchanged": True,
            "nas_files_moved": 0,
            "nas_files_renamed": 0,
            "nas_files_deleted": 0,
            "subscriptions_created": 0,
            "subscriptions_updated": 0,
        },
    }
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps({"output": str(output_path), "counts": counts}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

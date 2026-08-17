# WP-0147 Proof Summary: Subscription Continuity Against Existing Archive Folders

- **WP ID**: WP-0147
- **Date**: 2026-08-17
- **Status**: DONE

## Intent & Scope
Preserve per-subscription archive continuity by reconciling existing mapped folders, media files on disk, and recorded database memberships so that future refreshes only queue truly missing media and never re-download or overwrite existing files.

## Changes Implemented
1. `extract_youtube_id_from_filename`: Robust parser for 11-character YouTube video IDs from brackets `[VIDEO_ID]`, suffix delimiters ` - VIDEO_ID`, `_VIDEO_ID`, and raw stems.
2. `ensure_youtube_subscription_archive_state`:
   - Checks existing app-managed archive file and legacy `voxvulgi_youtube_archive.txt`.
   - Reconciles recorded `media_source_membership` for the subscription.
   - Performs a read-only scan of media files in the resolved output directory (`.mkv`, `.mp4`, `.webm`, `.ts`, `.m4v`), extracting video IDs and merging them into the app-managed archive and rollup tables.
3. Unit test `ensure_archive_state_reconciles_existing_folder_media_files_and_memberships` verified passing:
   - Seeding media files with IDs in bracketed and hyphenated naming conventions.
   - Seeding DB media memberships.
   - Asserting all IDs are accurately loaded into the archive tracking state.

## Verification
- Engine test `ensure_archive_state_reconciles_existing_folder_media_files_and_memberships`: Passed (1 passed; 0 failed).

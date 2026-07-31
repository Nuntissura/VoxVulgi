import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(here, "..");
const repoRoot = path.resolve(desktopRoot, "..", "..");

const page = fs.readFileSync(path.join(desktopRoot, "src", "pages", "LibraryPage.tsx"), "utf8");
const tauri = fs.readFileSync(path.join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");
const library = fs.readFileSync(path.join(repoRoot, "product", "engine", "src", "library.rs"), "utf8");
const jobs = fs.readFileSync(path.join(repoRoot, "product", "engine", "src", "jobs.rs"), "utf8");
const db = fs.readFileSync(path.join(repoRoot, "product", "engine", "src", "db.rs"), "utf8");

test("WP-0284 exposes exact selection actions on both requested screens", () => {
  assert.match(page, /Select loaded/);
  assert.match(page, /aria-label="Subscription video selection actions"/);
  assert.match(page, /aria-label="Media library selection actions"/);
  assert.match(page, /Delete selected \(\{subscriptionSelectedAvailableIds\.length\}\)/);
  assert.match(page, /Redownload selected \(\{subscriptionSelectedDeletedIds\.length\}\)/);
  assert.match(page, /Delete selected \(\{mediaLibrarySelectedAvailableIds\.length\}\)/);
  assert.match(page, /Redownload selected \(\{mediaLibrarySelectedDeletedIds\.length\}\)/);
  assert.match(page, /<option value="trash">Recycle Bin<\/option>/);
  assert.match(page, /<option value="permanent">Permanent<\/option>/);
});

test("WP-0284 keeps deleted media discoverable without mixing it into normal pagination", () => {
  assert.match(page, /<option value="available">Available<\/option>/);
  assert.match(page, /<option value="operator_deleted">Deleted<\/option>/);
  assert.match(page, /<option value="all">All \(deleted last\)<\/option>/);
  assert.match(page, /invoke<LibraryItemsPage>\("library_query"[\s\S]*fileStatus: mediaLibraryFileStatus/);
  assert.match(page, /<section className="sub-video-section" aria-label="Deleted videos">/);
  assert.match(library, /list_subscription_items_by_file_status/);
  assert.match(library, /FROM media_source_membership membership/);
});

test("WP-0284 destructive commands are registered but never agent-safe actions", () => {
  for (const command of ["library_file_delete", "library_operator_deleted_redownload"]) {
    assert.match(tauri, new RegExp(`async fn ${command}`));
    assert.match(tauri, new RegExp(`\\n\\s*${command},`));
  }
  assert.doesNotMatch(
    page,
    /data-agent-safe-action="true"[^>]*>\s*(?:Delete selected|Redownload selected)/,
  );
});

test("WP-0284 tombstone and exact-job capability gate every generic path", () => {
  for (const field of [
    "file_status",
    "file_status_changed_at_ms",
    "file_status_change_source",
    "file_delete_method",
    "file_redownload_authorized_job_id",
  ]) {
    assert.match(db, new RegExp(`"${field}"`));
  }
  assert.match(library, /DownloadSourceClaim::OperatorDeleted/);
  assert.match(library, /allow_operator_deleted: bool/);
  assert.match(jobs, /operator-deleted media requires an explicit selected-item redownload/);
  assert.match(jobs, /SkippedOperatorDeleted/);
  assert.match(
    jobs,
    /file_redownload_authorized_job_id[\s\S]*!= Some\(job_id\)[\s\S]*operator-delete-wp0284/,
  );
  assert.match(library, /authorized_redownload_completed/);
});

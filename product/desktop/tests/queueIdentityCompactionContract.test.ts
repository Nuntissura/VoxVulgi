import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const desktopRoot = path.resolve(import.meta.dirname, "..");
const repoRoot = path.resolve(desktopRoot, "..", "..");
const jobsSource = fs.readFileSync(path.join(repoRoot, "product", "engine", "src", "jobs.rs"), "utf8");
const runnerSource = fs.readFileSync(
  path.join(repoRoot, "product", "engine", "src", "bin", "voxvulgi_queue_identity_compact.rs"),
  "utf8",
);
const optionsSource = fs.readFileSync(
  path.join(desktopRoot, "src", "pages", "OptionsPage.tsx"),
  "utf8",
);

test("every queue compaction apply creates and reopens an online SQLite backup", () => {
  const helperStart = jobsSource.indexOf("fn create_verified_queue_identity_backup(");
  const applyStart = jobsSource.indexOf("pub fn youtube_queue_identity_reconcile(");
  assert.ok(helperStart >= 0 && applyStart > helperStart, "backup and apply boundaries must remain locatable");
  const helper = jobsSource.slice(helperStart, applyStart);
  assert.match(helper, /rusqlite::backup::Backup::new/);
  assert.match(helper, /backup\.run_to_completion/);
  assert.match(helper, /SQLITE_OPEN_READ_ONLY/);
  assert.match(helper, /PRAGMA quick_check/);
  assert.match(helper, /backup_scan != \*expected_scan/);

  const applyBoundary = jobsSource.slice(applyStart, jobsSource.indexOf("fn hydrate_job_target_titles", applyStart));
  assert.match(applyBoundary, /if dry_run \{[\s\S]*return Ok\(summary\);[\s\S]*create_verified_queue_identity_backup/);
  assert.match(applyBoundary, /summary\.backup = Some/);
  assert.match(applyBoundary, /transaction_with_behavior\(TransactionBehavior::Immediate\)/);
  assert.ok(
    applyBoundary.indexOf("create_verified_queue_identity_backup") <
      applyBoundary.indexOf("transaction_with_behavior(TransactionBehavior::Immediate)"),
    "verified backup must precede the mutation transaction",
  );
});

test("CLI and Options consume the engine-owned backup receipt instead of bypassing it", () => {
  assert.doesNotMatch(runnerSource, /--backup <verified-backup\.sqlite>/);
  assert.match(runnerSource, /summary\.backup\.clone\(\)/);
  assert.match(optionsSource, /Verified pre-apply backup:/);
  assert.match(optionsSource, /Create and verify an online database backup/);
});

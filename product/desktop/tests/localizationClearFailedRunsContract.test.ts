import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const desktopRoot = fileURLToPath(new URL("..", import.meta.url));
const app = readFileSync(join(desktopRoot, "src", "App.tsx"), "utf8");
const styles = readFileSync(join(desktopRoot, "src", "App.css"), "utf8");
const tauri = readFileSync(join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");
const jobs = readFileSync(join(desktopRoot, "..", "engine", "src", "jobs.rs"), "utf8");

test("WP-0206 keeps failed-run deletion item-scoped and conservative by default", () => {
  assert.match(jobs, /pub fn clear_failed_jobs_for_item/);
  assert.match(jobs, /WHERE item_id=\?1 AND status=\?2/);
  assert.match(jobs, /purge_orphan_artifacts: false/);
  assert.match(jobs, /if options\.purge_orphan_artifacts/);
  assert.match(jobs, /fn clear_failed_jobs_for_item_only_removes_failed_for_that_item/);
  assert.match(jobs, /fn clear_failed_jobs_for_item_purges_orphan_artifacts_when_opted_in/);
  assert.match(tauri, /fn jobs_clear_failed_for_item/);
  assert.match(tauri, /jobs::clear_failed_jobs_for_item/);
});

test("WP-0206 exposes the failed count and clear action on the default setup-first home", () => {
  const setupFirst = app.slice(app.indexOf("if (setupFirstHome)"), app.indexOf("function App()"));
  assert.match(setupFirst, /loc-setup-recent-row/);
  assert.match(setupFirst, /failed_jobs_count/);
  assert.match(setupFirst, /Clear failed runs/);
  assert.match(setupFirst, /clearFailedRunsForItem/);
  assert.match(setupFirst, /status\.failed_jobs_count <= 0/);
  assert.match(app, /purge_orphan_artifacts: purgeArtifacts/);
  assert.match(app, /Successful runs and deliverables are never touched/);
  assert.match(styles, /\.loc-setup-recent-row/);
});

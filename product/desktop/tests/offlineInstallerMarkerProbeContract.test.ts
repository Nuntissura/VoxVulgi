import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

test("offline core marker probe uses supported NSIS file commands", () => {
  const installer = readFileSync(
    join(root, "src-tauri", "installer", "templates", "installer.nsi"),
    "utf8",
  );
  const staleReleaseScript = join(
    root,
    "..",
    "..",
    "offline-installer-runtime",
    "scripts",
    "build_offline_release_fast.ps1",
  );
  const start = installer.indexOf("Function RunOfflineMarkerProbe");
  const end = installer.indexOf("FunctionEnd", start);

  assert.notEqual(start, -1, "RunOfflineMarkerProbe must exist");
  assert.notEqual(end, -1, "RunOfflineMarkerProbe must be closed");

  const probe = installer.slice(start, end);
  assert.match(installer, /\/VVMARKERPROBEENV=/);
  assert.doesNotMatch(installer, /\/VVMARKERPROBE=/);
  assert.match(probe, /ReadEnvStr \$OfflineMarkerProbePath "\$OfflineMarkerProbeEnvironment"/);
  assert.match(probe, /FileOpen \$0 "\$OfflineMarkerProbePath" w/);
  assert.match(probe, /FileWrite \$0 "schema=voxvulgi\.offline_core_marker_probe\.v1/);
  assert.match(probe, /FileClose \$0/);
  assert.doesNotMatch(
    probe,
    /\bFileFlush\b/,
    "FileFlush is not an NSIS command; FileClose completes the marker write",
  );
  assert.equal(existsSync(staleReleaseScript), false);
});

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

test("App does not pre-mount Diagnostics after startup settles", () => {
  const appSource = readFileSync(join(root, "src", "App.tsx"), "utf8");

  assert.equal(
    appSource.includes("prev.diagnostics ? prev : { ...prev, diagnostics: true }"),
    false,
    "startup settlement must not mark Diagnostics as visited; hidden Diagnostics runs heavy tool probes",
  );
});

test("Diagnostics initial load is gated by visible prop", () => {
  const diagnosticsSource = readFileSync(
    join(root, "src", "pages", "DiagnosticsPage.tsx"),
    "utf8",
  );

  assert.match(
    diagnosticsSource,
    /useEffect\(\(\) => \{\s+if \(!visible\) return;/,
    "DiagnosticsPage initial refresh effect must return immediately while hidden",
  );
});

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";
import { classifySafeAgentActions } from "../src/lib/agentUiAudit.ts";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("agent UI audit allows structural activation and refuses generic buttons", () => {
  assert.deepEqual(classifySafeAgentActions("summary", "button", false, false), [
    "scroll_into_view",
    "click",
  ]);
  assert.deepEqual(classifySafeAgentActions("button", "tab", false, false), [
    "scroll_into_view",
    "click",
  ]);
  assert.deepEqual(classifySafeAgentActions("button", "button", true, false), [
    "scroll_into_view",
    "click",
  ]);
  assert.deepEqual(classifySafeAgentActions("button", "option", true, false), [
    "scroll_into_view",
    "click",
  ]);
  assert.deepEqual(classifySafeAgentActions("button", "button", false, false), [
    "scroll_into_view",
  ]);
});

test("agent UI audit bridge is headless-only and has no arbitrary eval route", () => {
  const rust = readRepoFile("src-tauri", "src", "lib.rs");
  assert.match(rust, /\("POST", "\/agent\/ui_audit"\)/);
  assert.match(rust, /\("POST", "\/agent\/ui_action"\)/);
  assert.match(rust, /agent_headless/);
  assert.doesNotMatch(rust, /\/agent\/eval/);
  assert.doesNotMatch(rust, /\/agent\/execute_script/);
  assert.match(rust, /agent_bridge_marker_owned_by_process/);
});

test("agent UI audit includes app chrome while preserving stateful-only activation", () => {
  const auditSource = readRepoFile("src", "lib", "agentUiAudit.ts");
  const app = readRepoFile("src", "App.tsx");
  assert.match(auditSource, /const root = document\.body/);
  assert.match(app, /className=\{`safe-mode-pill[\s\S]{0,320}aria-pressed=\{safeMode\?\.enabled \?\? false\}/);
  assert.match(app, /aria-label="Dismiss Safe Mode exit notice"[\s\S]{0,120}data-agent-safe-action="true"/);
  assert.doesNotMatch(app, /className="win-btn"[\s\S]{0,160}data-agent-safe-action="true"/);
});

test("Windows headless startup hides without blocking the setup thread", () => {
  const rust = readRepoFile("src-tauri", "src", "lib.rs");
  const windowsHide = rust.match(
    /#\[cfg\(target_os = "windows"\)\]\s*fn hide_agent_headless_window[\s\S]*?\r?\n\}\r?\n/,
  )?.[0];
  assert.ok(windowsHide, "Windows headless hide helper must exist");
  assert.match(windowsHide, /ShowWindowAsync\(hwnd, SW_HIDE\)/);
  assert.doesNotMatch(windowsHide, /window\.hide\(\)/);
  assert.match(rust, /if cli_agent_headless[\s\S]*?hide_agent_headless_window\(&window\)/);
});

test("headless startup supports an absolute isolated app-data root without changing normal launches", () => {
  const rust = readRepoFile("src-tauri", "src", "lib.rs");
  assert.match(rust, /VOXVULGI_AGENT_HEADLESS_BASE_DIR/);
  assert.match(
    rust,
    /if !agent_headless \{[\s\S]{0,120}return Ok\(default_base_dir\)/,
    "normal launches must ignore the agent-only override",
  );
  assert.match(rust, /override_dir\.is_absolute\(\)/);
  assert.match(
    rust,
    /resolve_agent_headless_base_dir\([\s\S]{0,160}cli_agent_headless/,
  );
});

test("Video Archiver workflow tabs and subscription rows expose semantic selection state", () => {
  const source = readRepoFile("src", "pages", "LibraryPage.tsx");
  const auditSource = readRepoFile("src", "lib", "agentUiAudit.ts");
  assert.match(source, /role="tablist"[\s\S]*?aria-label="Video Archiver workflow"/);
  assert.match(source, /aria-pressed=\{videoArchiverTab === "youtube_single"\}/);
  assert.match(source, /aria-pressed=\{videoArchiverTab === "youtube_recurring"\}/);
  assert.match(source, /aria-pressed=\{videoArchiverTab === "website"\}/);
  assert.match(source, /role="option"[\s\S]*?aria-selected=\{selected\}/);
  assert.match(auditSource, /role === "option"/);
  assert.match(auditSource, /"tab", "option"/);
});

test("Jobs group disclosures expose semantic expanded state", () => {
  const source = readRepoFile("src", "pages", "JobsPage.tsx");
  assert.match(source, /aria-expanded=\{expanded\}[\s\S]*?setExpandedGroups/);
});

test("Localization current-item navigation is safe for a headless read-only probe", () => {
  const source = readRepoFile("src", "App.tsx");
  assert.match(
    source,
    /data-agent-safe-action="true"[\s\S]*?data-testid="localization-open-current-item"[\s\S]*?onClick=\{\(\) => onOpenEditor\(currentHomeItem\.id\)\}/,
  );
});

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";
import {
  classifySafeAgentActions,
  normalizeAgentActorId,
  requireExpectedProductActionId,
} from "../src/lib/agentUiAudit.ts";

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
  assert.deepEqual(
    classifySafeAgentActions("button", "button", false, false, "youtube.test-current-session", "external_probe"),
    ["scroll_into_view", "activate_product_action"],
  );
  assert.deepEqual(
    classifySafeAgentActions("select", "combobox", false, false, "youtube.browser-source", "reversible_state_change", "select"),
    ["scroll_into_view", "select_option"],
  );
  assert.deepEqual(
    classifySafeAgentActions("input", "textbox", false, false, "youtube.test-url", "reversible_state_change", "text"),
    ["scroll_into_view", "set_value"],
  );
  assert.deepEqual(
    classifySafeAgentActions("button", "button", false, false, "youtube.disconnect", "destructive"),
    ["scroll_into_view"],
  );
});

test("agent UI audit is token-authenticated live, actions are explicitly enabled, and arbitrary eval stays absent", () => {
  const rust = readRepoFile("src-tauri", "src", "lib.rs");
  assert.match(rust, /\("POST", "\/agent\/ui_audit"\)/);
  assert.match(rust, /\("POST", "\/agent\/ui_action"\)/);
  assert.match(rust, /operation == "action" && !agent_headless && !live_actions_enabled/);
  assert.match(rust, /VOXVULGI_AGENT_LIVE_ACTIONS/);
  assert.match(rust, /"bridge_token": action_token/);
  assert.match(rust, /constant_time_token_eq/);
  assert.match(rust, /object\.remove\("bridge_token"\)/);
  assert.match(rust, /let supplied_token = parsed[\s\S]{0,900}if !token_matches/);
  const stateHandler = rust.match(/fn agent_handle_state\(\)[\s\S]*?\n\}/)?.[0] ?? "";
  assert.ok(stateHandler, "agent state handler must remain inspectable");
  assert.doesNotMatch(stateHandler, /bridge_token/);
  assert.doesNotMatch(rust, /\/agent\/eval/);
  assert.doesNotMatch(rust, /\/agent\/execute_script/);
  assert.match(rust, /agent_bridge_marker_owned_by_process/);
});

test("hidden agent UI requests do not wait for animation frames", () => {
  const app = readRepoFile("src", "App.tsx");
  const listener = app.match(/>\("agent-ui-request", async \(event\) => \{[\s\S]*?\n\s*\}\);/)?.[0] ?? "";
  assert.ok(listener, "agent UI request listener must remain inspectable");
  assert.doesNotMatch(
    listener,
    /requestAnimationFrame/,
    "hidden WebViews may suspend animation frames, so audit and action receipts must complete immediately",
  );
});

test("operator-equivalent actions require bounded stable actor attribution", () => {
  assert.equal(normalizeAgentActorId("codex.agent-1", true), "codex.agent-1");
  assert.equal(normalizeAgentActorId(undefined, false), "agent");
  assert.throws(() => normalizeAgentActorId(undefined, true), /actor_id is required/);
  assert.throws(() => normalizeAgentActorId("bad actor!", true), /stable 1-64 character token/);
  assert.throws(() => normalizeAgentActorId(`a${"b".repeat(64)}`, true), /stable 1-64 character token/);
  assert.equal(
    requireExpectedProductActionId("youtube.test-current-session", "youtube.test-current-session"),
    "youtube.test-current-session",
  );
  assert.throws(
    () => requireExpectedProductActionId(undefined, "youtube.test-current-session"),
    /expected_product_action_id is required/,
  );
  assert.throws(
    () => requireExpectedProductActionId("youtube.wrong", "youtube.test-current-session"),
    /does not match/,
  );

  const audit = readRepoFile("src", "lib", "agentUiAudit.ts");
  const app = readRepoFile("src", "App.tsx");
  assert.match(audit, /activate_product_action[\s\S]*?normalizeAgentActorId\(request\.actor_id, true\)/);
  assert.match(audit, /select_option[\s\S]*?normalizeAgentActorId\(request\.actor_id, true\)/);
  assert.match(audit, /set_value[\s\S]*?normalizeAgentActorId\(request\.actor_id, true\)/);
  assert.equal(
    (audit.match(/requireExpectedProductActionId\(request\.expected_product_action_id, before\.product_action_id\)/g) ?? []).length,
    3,
  );
  assert.match(app, /effect_class:[\s\S]{0,320}actor_id:/);
});

test("YouTube operator controls opt into bounded semantic actions without exposing credentials", () => {
  const options = readRepoFile("src", "pages", "OptionsPage.tsx");
  const audit = readRepoFile("src", "lib", "agentUiAudit.ts");
  assert.match(options, /data-agent-action-id="youtube\.browser-source"[\s\S]{0,180}data-agent-input-kind="select"/);
  assert.match(audit, /available_choices: inputKind === "select" \? availableChoices : null/);
  assert.match(audit, /\.slice\(0, 100\)/);
  assert.match(audit, /filter\(\(option\) => !option\.disabled\)/);
  assert.match(options, /data-agent-action-id="youtube\.connect-selected-browser"[\s\S]{0,260}onClick=\{connectYoutubeBrowser\}/);
  assert.match(options, /data-agent-action-id="youtube\.test-current-session"[\s\S]{0,260}onClick=\{testCurrentYoutubeAuth\}/);
  assert.match(options, /async function testCurrentYoutubeAuth\(\)[\s\S]*?invoke<YoutubeAuthPreflightResult>\("config_youtube_auth_preflight"/);
  assert.match(audit, /isSensitiveAgentInput/);
  assert.match(audit, /cookie\|token\|password\|secret\|credential/);
  assert.match(audit, /refused text interaction without explicit non-secret semantic opt-in/);
  assert.doesNotMatch(options, /data-agent-action-id="[^"]*(?:cookie|token|password|secret|credential)/i);
  assert.doesNotMatch(readRepoFile("src-tauri", "src", "lib.rs"), /\/agent\/youtube_auth_preflight/);
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


test("managed voice plan exposes exact current-item original select/save handlers", () => {
  const source = readRepoFile("src", "pages", "SubtitleEditorPage.tsx");
  assert.ok(source.includes('data-agent-action-id={`localization.voice-plan-backend.${itemId}`}'));
  assert.ok(source.includes('data-agent-action-id={`localization.voice-plan-save.${itemId}`}'));
  assert.ok(source.includes('saveItemVoicePlan().catch(() => undefined)'));
  for (const id of ["localization.voice-plan-backend.item-1", "localization.voice-plan-backend.item-2"]) {
    assert.deepEqual(classifySafeAgentActions("select", "combobox", false, false, id, "reversible_state_change", "select"), ["scroll_into_view", "select_option"]);
  }
  assert.deepEqual(classifySafeAgentActions("button", "button", false, false, "localization.voice-plan-save.item-1", "reversible_state_change"), ["scroll_into_view", "activate_product_action"]);
  assert.throws(() => requireExpectedProductActionId("localization.voice-plan-save.item-1", "localization.voice-plan-save.item-2"));
});


test("original dub activation is conditional on clone readiness and guards confirmation races", () => {
  const source = readRepoFile("src", "pages", "SubtitleEditorPage.tsx");
  assert.ok(source.includes('clonePreflightSummary?.ready ? `localization.voice-preserving-dub.${itemId}` : undefined'));
  assert.ok(source.includes('onClick={enqueueDubVoicePreservingV1}'));
  assert.ok(source.includes('shouldRefuseQuietCloneConfirmation(quietActivation, preflight.ready)'));
  assert.ok(source.includes('event?.nativeEvent.isTrusted === false'));
  assert.ok(source.indexOf('shouldRefuseQuietCloneConfirmation(quietActivation, preflight.ready)') < source.indexOf('const proceed = await confirm(msg'));
  assert.ok(source.includes('if (quietActivation) throw new Error("Voice-preserving dub not admitted: required voice pack is not ready.'));
});

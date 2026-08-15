import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  optionsSettingById,
  projectOptionsSettingRuntime,
  redactOptionsSettingValue,
} from "../src/lib/optionsSettingsRegistry.ts";
import {
  DEFAULT_YOUTUBE_BROWSER_DRAFT,
  projectYoutubeBrowserStatus,
  reconcileYoutubeAuthStatus,
} from "../src/lib/youtubeAuthStatus.ts";

const browserDescriptor = optionsSettingById("video-archiver.youtube-browser-session");
const manualCookieDescriptor = optionsSettingById("video-archiver.youtube-manual-cookies");
const desktopRoot = join(dirname(fileURLToPath(import.meta.url)), "..");

test("connected Chrome to Disconnect clears canonical saved and effective browser state", () => {
  const connected = reconcileYoutubeAuthStatus({
    manual_cookie_configured: false,
    browser_cookie_source: "chrome",
    last_verified_at_ms: 100,
    reconnect_required_at_ms: null,
    credential_generation: 7,
    credential_fingerprint: "fingerprint-7",
  });
  assert.equal(connected.credentialGeneration, 7);
  assert.equal(connected.credentialFingerprint, "fingerprint-7");
  const connectedProjection = projectOptionsSettingRuntime(
    browserDescriptor,
    projectYoutubeBrowserStatus(connected, false),
  );
  assert.equal(connectedProjection.savedBaseline, "chrome");
  assert.equal(connectedProjection.effectiveRuntimeValue, "chrome");
  assert.equal(connectedProjection.dirty, false);

  const disconnected = reconcileYoutubeAuthStatus({
    manual_cookie_configured: false,
    browser_cookie_source: null,
    last_verified_at_ms: null,
    reconnect_required_at_ms: null,
  }, connected.browserDraftSource);
  const disconnectedProjection = projectOptionsSettingRuntime(
    browserDescriptor,
    projectYoutubeBrowserStatus(disconnected, false),
  );

  assert.equal(disconnected.browserDraftSource, DEFAULT_YOUTUBE_BROWSER_DRAFT);
  assert.equal(disconnectedProjection.savedBaselineAvailable, true);
  assert.equal(disconnectedProjection.effectiveRuntimeAvailable, true);
  assert.equal(disconnectedProjection.savedBaseline, null);
  assert.equal(disconnectedProjection.effectiveRuntimeValue, null);
  assert.equal(disconnectedProjection.dirty, false);
  assert.equal(disconnectedProjection.invalid, false);
});

test("Options sends the last canonical credential revision with every auth replacement", () => {
  const options = readFileSync(join(desktopRoot, "src", "pages", "OptionsPage.tsx"), "utf8");
  const tauri = readFileSync(join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");
  const config = readFileSync(join(desktopRoot, "..", "engine", "src", "config.rs"), "utf8");
  assert.match(options, /authRevisionRef/);
  assert.match(options, /if \(!authRevisionHydrated \|\| !expected\)[\s\S]*?YouTube sign-in status has not loaded; reload Options before changing sign-in/);
  assert.match(options, /expectedCredentialGeneration: expected\.generation/);
  assert.match(options, /expectedCredentialFingerprint: expected\.fingerprint/);
  assert.match(options, /async function replaceYoutubeAuth/);
  assert.equal((options.match(/config_youtube_auth_set/g) ?? []).length, 1);
  assert.match(tauri, /YOUTUBE_AUTH_OPERATION_LOCK/);
  assert.match(tauri, /expected_credential_generation/);
  assert.match(tauri, /expected_credential_fingerprint/);
  assert.match(config, /OBSERVED_YOUTUBE_AUTH_REVISIONS/);
  assert.match(config, /youtube_auth_public_mark_rejects_result_for_replaced_observed_credential/);
});

test("YouTube auth replacement commits CAS before qualified runtime-block cleanup", () => {
  const tauri = readFileSync(join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");
  const jobs = readFileSync(join(desktopRoot, "..", "engine", "src", "jobs.rs"), "utf8");
  const commandStart = tauri.indexOf("fn config_youtube_auth_set(");
  const commandEnd = tauri.indexOf("const YOUTUBE_SIGN_IN_URL", commandStart);
  const command = tauri.slice(commandStart, commandEnd);
  assert.match(command, /replace_youtube_auth_config_and_clear_previous_block/);
  assert.doesNotMatch(command, /clear_youtube_auth_block\(&state\.paths\)/);

  const publicHelperStart = jobs.indexOf("pub fn replace_youtube_auth_config_and_clear_previous_block(");
  const helperStart = jobs.indexOf("fn replace_youtube_auth_config_with_cleanup<F>(", publicHelperStart);
  const helperEnd = jobs.indexOf("fn active_youtube_auth_block(", helperStart);
  const publicHelper = jobs.slice(publicHelperStart, helperStart);
  const helper = jobs.slice(helperStart, helperEnd);
  const casIndex = helper.indexOf("config::replace_youtube_auth_config(");
  const cleanupIndex = helper.indexOf("cleanup(paths, &previous_auth_key)");
  assert.match(publicHelper, /clear_youtube_auth_block_for_key/);
  assert.ok(casIndex >= 0 && cleanupIndex > casIndex, "CAS must complete before runtime cleanup");
  assert.match(helper, /previous_auth_key\.and_then/);
  assert.match(helper, /cleanup_warning/);
});

test("connected Chrome to manual cookies surfaces no browser and only redacted credential state", () => {
  const manual = reconcileYoutubeAuthStatus({
    manual_cookie_configured: true,
    browser_cookie_source: null,
    last_verified_at_ms: null,
    reconnect_required_at_ms: null,
  }, "chrome");
  const browserProjection = projectOptionsSettingRuntime(
    browserDescriptor,
    projectYoutubeBrowserStatus(manual, false),
  );
  const manualProjection = projectOptionsSettingRuntime(manualCookieDescriptor, {
    draftValue: manual.manualCookieConfigured,
    savedBaseline: manual.manualCookieConfigured,
    effectiveRuntimeValue: manual.manualCookieConfigured,
  });

  assert.equal(browserProjection.savedBaseline, null);
  assert.equal(browserProjection.effectiveRuntimeValue, null);
  assert.equal(manualProjection.savedBaseline, "[credential configured]");
  assert.equal(manualProjection.effectiveRuntimeValue, "[credential configured]");
  assert.equal(redactOptionsSettingValue(manualCookieDescriptor, "SID=never-render-this"), "[credential configured]");
  assert.doesNotMatch(JSON.stringify({ manual, browserProjection, manualProjection }), /never-render-this|SID=/);
});

test("missing browser status field is unknown while explicit null is known unconfigured", () => {
  const unknown = reconcileYoutubeAuthStatus({ manual_cookie_configured: false }, "chrome");
  const unknownProjection = projectOptionsSettingRuntime(
    browserDescriptor,
    projectYoutubeBrowserStatus(unknown, false),
  );
  assert.equal(unknown.browserDraftSource, "chrome");
  assert.equal(unknownProjection.savedBaselineAvailable, false);
  assert.equal(unknownProjection.effectiveRuntimeAvailable, false);
  assert.equal(unknownProjection.dirty, false);
  assert.equal(browserDescriptor.defaultValue, null);
});

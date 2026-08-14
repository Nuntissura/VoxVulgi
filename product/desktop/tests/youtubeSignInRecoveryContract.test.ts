import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("YouTube Options leads with browser sign-in and exact recovery steps", () => {
  const source = readRepoFile("src", "pages", "OptionsPage.tsx");

  assert.match(source, /Open YouTube in \$\{youtubeBrowserLabel\(authBrowserSource\)\}/);
  assert.match(source, /I've signed in — connect and test/);
  assert.match(source, /Sign-in required/);
  assert.match(source, /sign out of YouTube and sign back in/);
  assert.match(source, /Confirm that a normal YouTube video plays/);
  assert.match(source, /Close every \{youtubeBrowserLabel\(authBrowserSource\)\} window/);
  assert.match(source, /Try connection again/);
  assert.match(source, /Manual cookie import \(advanced fallback\)/);
  assert.match(source, /Save and test manual cookies/);
});

test("YouTube connection truth persists verification and reconnect timestamps", () => {
  const optionsSource = readRepoFile("src", "pages", "OptionsPage.tsx");
  const authStatusSource = readRepoFile("src", "lib", "youtubeAuthStatus.ts");
  const configSource = readRepoFile("..", "engine", "src", "config.rs");
  const jobsSource = readRepoFile("..", "engine", "src", "jobs.rs");

  assert.match(authStatusSource, /last_verified_at_ms\?: number \| null/);
  assert.match(authStatusSource, /reconnect_required_at_ms\?: number \| null/);
  assert.match(optionsSource, /applyYoutubeAuthStatusReceipt\(saved\)/);
  assert.match(configSource, /pub last_verified_at_ms: Option<i64>/);
  assert.match(configSource, /pub reconnect_required_at_ms: Option<i64>/);
  assert.match(jobsSource, /mark_youtube_auth_verified\(paths, checked_at_ms\)/);
  assert.match(jobsSource, /mark_youtube_auth_reconnect_required\(paths, checked_at_ms\)/);
  assert.match(jobsSource, /save_youtube_auth_block\(paths, &state\)\?;[\s\S]{0,120}mark_youtube_auth_reconnect_required\(paths, now\)/);
});

test("YouTube sign-in launcher targets the selected browser instead of silently changing profiles", () => {
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  assert.match(tauriSource, /fn youtube_auth_open_sign_in\(browser_source: String\)/);
  assert.match(tauriSource, /normalize_browser_cookie_source\(Some\(&browser_source\)\)/);
  assert.match(tauriSource, /launch_youtube_sign_in_in_browser\(&browser_source\)/);
  assert.match(tauriSource, /Could not find \{browser_source\} on this computer/);
});

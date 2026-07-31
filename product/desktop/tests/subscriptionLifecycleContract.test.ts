import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";
import { classifyFailure } from "../src/lib/failureStates.ts";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("HTTP 404 is presented as unavailable without claiming channel deletion", () => {
  const failure = classifyFailure("Unable to download API page: HTTP Error 404: Not Found");
  assert.equal(failure.label, "Unavailable");
  assert.match(failure.requirement, /does not prove its hosting channel was deleted/i);

  const network = classifyFailure("network connection timed out");
  assert.notEqual(network.label, "Unavailable");

  const searchResult = classifyFailure("YouTube said: This channel does not exist.");
  assert.equal(searchResult.label, "Channel/handle not found");
});

test("subscription manager uses preserved manual status instead of destructive Delete", () => {
  const source = readRepoFile("src", "pages", "LibraryPage.tsx");
  assert.match(source, /youtube_subscriptions_set_manual_status/);
  assert.match(
    source,
    /"Restore subscription"[\s\S]*?: "Mark subscription deleted"/,
  );
  assert.match(source, /saved videos, subtitles, source memberships, metadata, and job history will be kept/i);
  assert.match(source, /This does not prove its hosting channel was deleted/i);
  assert.doesNotMatch(source, /onClick=\{\(\) => deleteSubscription\(sub\.id\)\}/);
});

test("assistant status endpoint is headless-only and actor-attributed", () => {
  const source = readRepoFile("src-tauri", "src", "lib.rs");
  assert.match(source, /\("POST", "\/agent\/subscription_status"\)/);
  assert.match(source, /subscription status changes require --agent-headless/);
  assert.match(source, /YOUTUBE_SUBSCRIPTION_STATUS_ACTOR_ASSISTANT/);
  assert.match(source, /subscription_manual_status_changed/);
});

test("engine keeps deleted manual-only and unavailable exact-404-only", () => {
  const source = readRepoFile("..", "engine", "src", "subscriptions.rs");
  assert.match(source, /manual subscription status must be normal or deleted/);
  assert.match(source, /manual subscription status actor must be operator or assistant/);
  assert.match(source, /is_confirmed_http_404_refresh_error/);
  assert.match(source, /source_status <> 'deleted'/);
  assert.match(source, /automatic refresh outcomes cannot change deleted/);
});

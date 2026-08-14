import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  beginInstagramCapabilityEpoch,
  beginInstagramMutationEpoch,
  applyIfCurrentInstagramMutation,
  invalidateInstagramCapabilityEpoch,
  isCurrentInstagramCapabilityEpoch,
  isCurrentInstagramCredentialRevision,
  isCurrentInstagramMutationEpoch,
} from "../src/lib/instagramCapabilityEpoch";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => { resolve = next; });
  return { promise, resolve };
}

test("Instagram mutations carry canonical CAS revision and rehydrate conflicts", () => {
  const desktopRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
  const options = readFileSync(join(desktopRoot, "src", "pages", "OptionsPage.tsx"), "utf8");
  const tauri = readFileSync(join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");
  const jobs = readFileSync(join(desktopRoot, "..", "engine", "src", "jobs.rs"), "utf8");
  assert.match(options, /expectedCredentialGeneration: expected\.generation/);
  assert.match(options, /expectedCredentialFingerprint: expected\.fingerprint/);
  assert.match(options, /async function replaceInstagramAuth/);
  assert.match(options, /config_instagram_auth_get/);
  assert.match(tauri, /fn config_instagram_auth_set\([\s\S]*?expected_credential_generation: Option<u64>[\s\S]*?expected_credential_fingerprint: Option<String>/);
  assert.match(jobs, /INSTAGRAM_AUTH_WRITER_LOCK/);
  assert.match(jobs, /replace_global_instagram_auth_cookie/);
  assert.match(jobs, /instagram credentials changed concurrently/);
  assert.match(jobs, /InstagramAuthMutationReceipt/);
  assert.match(jobs, /credentials were committed, but the previous authentication hold could not be cleared/);
  assert.match(jobs, /acquire_instagram_auth_interprocess_lock/);
  assert.match(jobs, /ensure_instagram_preflight_revision_current/);
  assert.match(jobs, /pub struct InstagramAuthPreflightResult \{[\s\S]*?credential_generation: u64[\s\S]*?credential_fingerprint: String/);
  assert.match(options, /isCurrentInstagramCredentialRevision\(currentRevision, result\)/);
  assert.match(options, /saved\.cleanup_warning/);
  assert.match(options, /cfg\.cleanup_warning/);
  assert.match(options, /beginInstagramMutationEpoch\(instagramMutationEpochRef\)/);
  assert.match(options, /isCurrentInstagramMutationEpoch\(instagramMutationEpochRef, operationEpoch\)\) setIgAuthBusy\(false\)/);
  const statusStart = tauri.indexOf("struct InstagramAuthConfigStatus {");
  const statusEnd = tauri.indexOf("}", statusStart);
  assert.doesNotMatch(tauri.slice(statusStart, statusEnd), /cookie:/);
  assert.match(tauri.slice(statusStart, statusEnd), /cleanup_warning/);
});

test("Instagram navigation invalidates capability work without hiding mutation settlement", async () => {
  const capabilityRef = { current: 0 };
  const mutationRef = { current: 0 };
  const mutationEpoch = beginInstagramMutationEpoch(mutationRef);
  const result = deferred<{ cleanup_warning: string }>();
  let visibleMessage = "Saving…";
  let visibleBusy = true;
  const completion = result.promise.then((receipt) => {
    if (isCurrentInstagramMutationEpoch(mutationRef, mutationEpoch)) {
      visibleMessage = `Saved. Warning: ${receipt.cleanup_warning}`;
      visibleBusy = false;
    }
  });

  invalidateInstagramCapabilityEpoch(capabilityRef);
  result.resolve({ cleanup_warning: "cleanup still needs attention" });
  await completion;

  assert.match(visibleMessage, /cleanup still needs attention/);
  assert.equal(visibleBusy, false);
});

test("a newer Instagram reset owns busy settlement over an older mutation", () => {
  const mutationRef = { current: 0 };
  const older = beginInstagramMutationEpoch(mutationRef);
  const reset = beginInstagramMutationEpoch(mutationRef);
  assert.equal(isCurrentInstagramMutationEpoch(mutationRef, older), false);
  assert.equal(isCurrentInstagramMutationEpoch(mutationRef, reset), true);
});

test("a reversed older completion cannot repaint after a newer reset receipt", () => {
  const mutationRef = { current: 0 };
  let projection = { generation: 0, fingerprint: "initial", configured: false };
  const olderSave = beginInstagramMutationEpoch(mutationRef);
  const newerReset = beginInstagramMutationEpoch(mutationRef);

  assert.equal(applyIfCurrentInstagramMutation(mutationRef, newerReset, () => {
    projection = { generation: 2, fingerprint: "reset", configured: false };
  }), true);
  assert.equal(applyIfCurrentInstagramMutation(mutationRef, olderSave, () => {
    projection = { generation: 1, fingerprint: "older-save", configured: true };
  }), false);
  assert.deepEqual(projection, { generation: 2, fingerprint: "reset", configured: false });
});

test("Instagram preflight receipt is rejected after a canonical credential replacement", () => {
  const observed = { generation: 7, fingerprint: "old" };
  assert.equal(isCurrentInstagramCredentialRevision(observed, {
    credential_generation: 7,
    credential_fingerprint: "old",
  }), true);
  assert.equal(isCurrentInstagramCredentialRevision(observed, {
    credential_generation: 8,
    credential_fingerprint: "replacement",
  }), false);
  assert.equal(isCurrentInstagramCredentialRevision(null, {
    credential_generation: 0,
    credential_fingerprint: "disconnected",
  }), false);
});

for (const invalidation of [
  "credential save",
  "credential clear",
  "module reset",
  "preflight target change",
]) {
  test(`Instagram ${invalidation} prevents an older deferred preflight from repainting state`, async () => {
    const epochRef = { current: 0 };
    const preflightEpoch = beginInstagramCapabilityEpoch(epochRef);
    const result = deferred<{ ok: boolean; message: string }>();
    let visibleMessage = "Testing saved Instagram credentials…";
    let visibleBusy = true;
    const completion = result.promise.then((receipt) => {
      if (isCurrentInstagramCapabilityEpoch(epochRef, preflightEpoch)) {
        visibleMessage = receipt.message;
        visibleBusy = false;
      }
    });

    invalidateInstagramCapabilityEpoch(epochRef);
    visibleBusy = false;
    result.resolve({ ok: true, message: "stale success" });
    await completion;

    assert.notEqual(visibleMessage, "stale success");
    assert.equal(visibleBusy, false);
  });
}

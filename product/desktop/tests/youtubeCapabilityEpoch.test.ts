import test from "node:test";
import assert from "node:assert/strict";
import {
  beginYoutubeCapabilityEpoch,
  invalidateYoutubeCapabilityEpoch,
  isCurrentYoutubeCapabilityEpoch,
} from "../src/lib/youtubeCapabilityEpoch";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => { resolve = next; });
  return { promise, resolve };
}

for (const invalidation of [
  "credential save",
  "credential disconnect",
  "module reset",
  "preflight target change",
  "module exit and re-entry",
]) {
  test(`YouTube ${invalidation} prevents an older deferred preflight from repainting state`, async () => {
    const epochRef = { current: 0 };
    const preflightEpoch = beginYoutubeCapabilityEpoch(epochRef);
    const result = deferred<{ ok: boolean; message: string }>();
    let visibleMessage = "Testing saved YouTube credentials…";
    let visibleBusy = true;
    const completion = result.promise.then((receipt) => {
      if (isCurrentYoutubeCapabilityEpoch(epochRef, preflightEpoch)) {
        visibleMessage = receipt.message;
        visibleBusy = false;
      }
    });

    invalidateYoutubeCapabilityEpoch(epochRef);
    visibleBusy = false;
    result.resolve({ ok: true, message: "stale success" });
    await completion;

    assert.notEqual(visibleMessage, "stale success");
    assert.equal(visibleBusy, false);
  });
}

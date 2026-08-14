import test from "node:test";
import assert from "node:assert/strict";
import { segmentAudioRange, segmentAudioReachedEnd } from "../src/lib/segmentAudioRange.ts";

test("segment audio range converts valid subtitle timing", () => {
  assert.deepEqual(segmentAudioRange(1_250, 2_750), {
    startSeconds: 1.25,
    endSeconds: 2.75,
    durationMs: 1_500,
  });
});

test("segment audio range rejects empty, reversed, negative, and non-finite timing", () => {
  assert.equal(segmentAudioRange(1_000, 1_000), null);
  assert.equal(segmentAudioRange(2_000, 1_000), null);
  assert.equal(segmentAudioRange(-1, 1_000), null);
  assert.equal(segmentAudioRange(Number.NaN, 1_000), null);
  assert.equal(segmentAudioRange(0, Number.POSITIVE_INFINITY), null);
});

test("segment end comparison tolerates media timeupdate precision without stopping early", () => {
  assert.equal(segmentAudioReachedEnd(2.97, 3), false);
  assert.equal(segmentAudioReachedEnd(2.98, 3), true);
  assert.equal(segmentAudioReachedEnd(Number.NaN, 3), false);
});

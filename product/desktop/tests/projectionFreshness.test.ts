import test from "node:test";
import assert from "node:assert/strict";
import { isProjectionRequestCurrent } from "../src/lib/projectionFreshness";

test("render-time query identity rejects an old result before passive-effect cleanup", () => {
  const oldRequest = { generation: 7, queryKey: "youtube-single|old-search" };
  const renderTimeCurrent = { generation: 7, queryKey: "youtube-single|new-search" };

  assert.equal(isProjectionRequestCurrent(oldRequest, renderTimeCurrent), false);
});

test("a newer request generation rejects an earlier result for the same query", () => {
  const oldRequest = { generation: 7, queryKey: "media-library|all" };
  const currentRequest = { generation: 8, queryKey: "media-library|all" };

  assert.equal(isProjectionRequestCurrent(oldRequest, currentRequest), false);
  assert.equal(isProjectionRequestCurrent(currentRequest, currentRequest), true);
});

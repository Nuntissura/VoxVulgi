import test from "node:test";
import assert from "node:assert/strict";
import { formatStorageFreeSpace } from "../src/lib/optionsStorage.ts";

test("storage space preserves verified zero and distinguishes unavailable probes", () => {
  assert.equal(formatStorageFreeSpace(0), "0 B");
  assert.equal(formatStorageFreeSpace(1024 ** 3 * 12.5), "12.5 GiB");
  for (const value of [null, undefined, -1, NaN, Infinity]) assert.equal(formatStorageFreeSpace(value), "Unknown");
});

test("capacity refresh does not present a previous successful observation as current", () => {
  assert.equal(formatStorageFreeSpace(4096, true), "Unknown");
  assert.equal(formatStorageFreeSpace(4096, false, "storage probe unavailable"), "Unknown");
  assert.equal(formatStorageFreeSpace(4096, false, null), "4.0 KiB");
});

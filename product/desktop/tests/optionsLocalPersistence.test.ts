import assert from "node:assert/strict";
import test from "node:test";

import {
  loadOptionsLocalPreferenceBaselines,
  persistOptionsLocalPreference,
  resetOptionsLocalPreference,
  verifyOptionsLocalPreferencePersistence,
} from "../src/lib/optionsLocalPersistence";
import { verifiedLocalStorageSet } from "../src/lib/persist";
import {
  getDesktopFontScaleBaseline,
  resetStoredDesktopFontScalePct,
  setStoredDesktopFontScalePct,
} from "../src/lib/fontScale";
import {
  OPTIONS_SETTINGS_REGISTRY,
  projectOptionsSettingRuntime,
} from "../src/lib/optionsSettingsRegistry";

type StorageMode = "normal" | "quota" | "security" | "discard";

function installStorage(initial: Record<string, string> = {}, mode: StorageMode = "normal") {
  const values = new Map(Object.entries(initial));
  const localStorage = {
    getItem(key: string) {
      if (mode === "security") throw new DOMException("blocked", "SecurityError");
      return values.get(key) ?? null;
    },
    setItem(key: string, value: string) {
      if (mode === "quota") throw new DOMException("full", "QuotaExceededError");
      if (mode === "security") throw new DOMException("blocked", "SecurityError");
      if (mode !== "discard") values.set(key, value);
    },
    removeItem(key: string) {
      if (mode === "security") throw new DOMException("blocked", "SecurityError");
      values.delete(key);
    },
  };
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { localStorage },
  });
  return values;
}

test("verified local persistence throws for quota, security, and discarded writes", () => {
  installStorage({}, "quota");
  assert.throws(() => verifiedLocalStorageSet("setting", "draft"), /QuotaExceededError|full/);

  installStorage({}, "security");
  assert.throws(() => verifiedLocalStorageSet("setting", "draft"), /SecurityError|blocked/);

  installStorage({}, "discard");
  assert.throws(
    () => verifiedLocalStorageSet("setting", "draft"),
    /was not retained by browser storage/,
  );
});

test("saved Options baseline is independently reloaded after a frontend restart", () => {
  const values = installStorage();
  const key = "voxvulgi.v1.library.legacy_archive_root" as const;
  const before = loadOptionsLocalPreferenceBaselines();
  assert.equal(before[key].value, "");
  assert.equal(before[key].available, true);

  const receipt = persistOptionsLocalPreference(key, "D:\\archive");
  assert.deepEqual(receipt, { value: "D:\\archive", available: true, error: null });
  assert.equal(values.get(key), "D:\\archive");

  // A new load models a new WebView/frontend process: it does not reuse the prior draft object.
  installStorage(Object.fromEntries(values));
  const afterRestart = loadOptionsLocalPreferenceBaselines();
  assert.equal(afterRestart[key].value, "D:\\archive");
  assert.equal(afterRestart[key].available, true);
});

test("cleanup run localStorage projection requires an exact persisted readback", () => {
  const key = "voxvulgi.v1.library.cleanup_run_id" as const;
  installStorage({}, "discard");
  assert.throws(
    () => persistOptionsLocalPreference(key, "cleanup-run-42"),
    /was not retained by browser storage/,
  );

  installStorage({}, "quota");
  assert.throws(
    () => persistOptionsLocalPreference(key, "cleanup-run-42"),
    /QuotaExceededError|full/,
  );

  const values = installStorage();
  persistOptionsLocalPreference(key, "cleanup-run-42");
  installStorage(Object.fromEntries(values));
  assert.equal(loadOptionsLocalPreferenceBaselines()[key].value, "cleanup-run-42");
});

test("localStorage capability probe reports denial and restores its probe key", () => {
  installStorage({}, "security");
  assert.throws(
    () => verifyOptionsLocalPreferencePersistence(),
    /Browser storage is unavailable|SecurityError|blocked/,
  );

  const probeKey = "voxvulgi.v1.options.persistence_probe";
  const values = installStorage({ [probeKey]: "operator-value" });
  verifyOptionsLocalPreferencePersistence();
  assert.equal(values.get(probeKey), "operator-value");
});

test("inaccessible storage produces an unavailable baseline instead of a clean default", () => {
  installStorage({}, "security");
  const baselines = loadOptionsLocalPreferenceBaselines();
  for (const baseline of Object.values(baselines)) {
    assert.equal(baseline.available, false);
    assert.match(baseline.error ?? "", /SecurityError|blocked/);
  }
});

test("unavailable hydration never persists fallback defaults", () => {
  let writes = 0;
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      localStorage: {
        getItem() { throw new DOMException("blocked", "SecurityError"); },
        setItem() { writes += 1; },
        removeItem() { writes += 1; },
      },
    },
  });
  const baselines = loadOptionsLocalPreferenceBaselines();
  assert.equal(writes, 0);
  assert.ok(Object.values(baselines).every(({ available }) => !available));
});

test("font scale reset cannot report success when persistence fails", () => {
  installStorage({}, "normal");
  assert.equal(setStoredDesktopFontScalePct(120), 120);
  assert.deepEqual(getDesktopFontScaleBaseline(), { value: 120, available: true, error: null });

  installStorage({}, "quota");
  assert.throws(() => resetStoredDesktopFontScalePct(), /QuotaExceededError|full/);
});

test("local preference reset restores prior bytes when readback detects a partial write", () => {
  const key = "voxvulgi.v1.library.cleanup_root" as const;
  const values = new Map<string, string>([[key, "D:\\original"]]);
  let writes = 0;
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      localStorage: {
        getItem(storageKey: string) {
          return values.get(storageKey) ?? null;
        },
        setItem(storageKey: string, value: string) {
          writes += 1;
          values.set(storageKey, writes === 1 ? `${value}-corrupt` : value);
        },
        removeItem(storageKey: string) {
          values.delete(storageKey);
        },
      },
    },
  });

  assert.throws(
    () => resetOptionsLocalPreference(key, ""),
    /previous value was restored/,
  );
  assert.equal(values.get(key), "D:\\original");
  assert.equal(writes, 2, "one failed reset write plus one verified rollback");
});

test("local preference reset reports applied only after exact readback", () => {
  const key = "voxvulgi.v1.library.cleanup_root" as const;
  const values = installStorage({ [key]: "D:\\old" });
  const receipt = resetOptionsLocalPreference(key, "");
  assert.deepEqual(receipt, {
    key,
    previousValue: "D:\\old",
    value: "",
    outcome: "applied",
  });
  assert.equal(values.get(key), "");
});

test("registry restart projections stay immediate because no current adapter requires restart", () => {
  for (const descriptor of OPTIONS_SETTINGS_REGISTRY) {
    assert.equal(descriptor.restartRequirement, "none", descriptor.id);
    const projection = projectOptionsSettingRuntime(descriptor, {
      draftValue: "changed",
      savedBaseline: "saved",
    });
    assert.equal(projection.restartPending, false, descriptor.id);
  }
});

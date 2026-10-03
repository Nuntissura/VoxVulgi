import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const source = readFileSync(new URL("../scripts/qualify_offline_runtime.ps1", import.meta.url), "utf8");
const block = source.slice(source.indexOf("  # ModelStore resolves"), source.indexOf("  Copy-Tree $hfSource"));

test("qualification resolves flat ASR inputs into manifest paths and refuses corrupt bytes", () => {
  assert.ok(block.includes("$modelRequired"));
  const root = mkdtempSync(join(tmpdir(), "vv-model-layout-"));
  try {
    const models = join(root, "input");
    const generation = join(root, "generation");
    mkdirSync(models);
    mkdirSync(generation);
    const bytes = Buffer.from("actual-fixture-model");
    const manifest = join(root, "manifest.json");
    writeFileSync(manifest, JSON.stringify({ models: ["whispercpp-large-v3-q5_0", "whispercpp-tiny"].map(id => ({
      id, version: "fixture", files: [{ path: `${id}.bin`, size_bytes: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") }],
    })) }));
    for (const id of ["whispercpp-large-v3-q5_0", "whispercpp-tiny"]) writeFileSync(join(models, `${id}.bin`), bytes);
    const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;
    const command = `$ErrorActionPreference='Stop';function Resolve-File($p,$l){(Get-Item -LiteralPath $p -ErrorAction Stop).FullName};$ModelManifestPath=${quote(manifest)};$modelsSource=${quote(models)};$generation=${quote(generation)};${block};$modelRequired|ConvertTo-Json -Compress`;
    const run = () => spawnSync("pwsh", ["-NoProfile", "-Command", command], { encoding: "utf8", windowsHide: true });
    const valid = run();
    assert.equal(valid.status, 0, valid.stderr);
    assert.deepEqual(JSON.parse(valid.stdout.trim()), [
      "models/whispercpp-large-v3-q5_0/fixture/whispercpp-large-v3-q5_0.bin",
      "models/whispercpp-tiny/fixture/whispercpp-tiny.bin",
    ]);
    assert.deepEqual(readFileSync(join(generation, "models", "whispercpp-large-v3-q5_0", "fixture", "whispercpp-large-v3-q5_0.bin")), bytes);
    writeFileSync(join(models, "whispercpp-tiny.bin"), Buffer.alloc(bytes.length));
    const corrupt = run();
    assert.notEqual(corrupt.status, 0);
    assert.match(corrupt.stderr, /fails exact product manifest identity/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, writeFileSync, mkdirSync, rmSync, readdirSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const source = readFileSync(new URL("../scripts/qualify_offline_runtime.ps1", import.meta.url), "utf8");
const block = source.slice(source.indexOf("  # ModelStore resolves"), source.indexOf("  Copy-Tree $hfSource"));

test("qualification extracts the actual CosyVoice graph producer and refuses missing or duplicate source", () => {
  const tools = readFileSync(new URL("../../product/engine/src/tools.rs", import.meta.url), "utf8");
  const expected = tools.match(/^fn cosyvoice_model_graph_code\(\) -> &'static str \{\r?\n    r#"([\s\S]*?)"#\r?\n\}/m);
  assert.ok(expected, "actual product graph producer required");
  const extractor = source.slice(source.indexOf("function Get-CosyVoiceModelGraphProducer("), source.indexOf("function Test-PythonRuntime("));
  assert.ok(extractor.length > 0);
  const root = mkdtempSync(join(tmpdir(), "vv-cosy-graph-source-"));
  let passed = false;
  try {
    const fixture = join(root, "tools.rs");
    const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;
    const command = `$ErrorActionPreference='Stop';function Resolve-File($p,$l){(Get-Item -LiteralPath $p -ErrorAction Stop).FullName};${extractor};Get-CosyVoiceModelGraphProducer ${quote(fixture)}|ConvertTo-Json -Compress`;
    const run = () => spawnSync("pwsh", ["-NoProfile", "-NonInteractive", "-Command", command], { encoding: "utf8", windowsHide: true });
    writeFileSync(fixture, tools);
    const valid = run();
    assert.equal(valid.status, 0, valid.stderr);
    const actual = JSON.parse(valid.stdout.trim());
    assert.equal(actual.code, expected[1], "execute exact product Python, not a duplicate graph");
    assert.equal(actual.code_sha256, createHash("sha256").update(expected[1]).digest("hex"));
    assert.equal(actual.producer_sha256, createHash("sha256").update(expected[0]).digest("hex"));
    writeFileSync(fixture, "// producer absent\n");
    const missing = run();
    assert.notEqual(missing.status, 0);
    assert.match(missing.stderr, /Require one exact CosyVoice graph producer/);
    writeFileSync(fixture, `${expected[0]}\n${expected[0]}\n`);
    const duplicate = run();
    assert.notEqual(duplicate.status, 0);
    assert.match(duplicate.stderr, /Require one exact CosyVoice graph producer/);
    writeFileSync(fixture, expected[0].replace('r#"', 'r##"'));
    const changedShape = run();
    assert.notEqual(changedShape.status, 0);
    assert.match(changedShape.stderr, /raw block is missing or changed shape/);
    passed = true;
  } finally {
    if (passed) rmSync(root, { recursive: true, force: true });
    else console.error(`Retained CosyVoice source extraction fixture: ${root}`);
  }
});

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

test("qualification composes authoritative venv packages without portable bootstrap collisions", { skip: process.platform !== "win32" }, () => {
  const root = mkdtempSync(join(tmpdir(), "vv-runtime-package-layout-"));
  let passed = false;
  try {
    const portable = join(root, "portable");
    const venv = join(root, "venv");
    const destination = join(root, "runtime");
    const put = (path: string, text: string) => { mkdirSync(join(path, ".."), { recursive: true }); writeFileSync(path, text); };
    const metadata = (base: string, directory: string, name: string, version: string) => put(join(base, "Lib", "site-packages", directory, "METADATA"), `Name: ${name}\nVersion: ${version}\n`);
    put(join(portable, "python.exe"), "owned-nonexecuted-fixture");
    put(join(portable, "python311.dll"), "base-dll");
    put(join(portable, "python311.zip"), "base-stdlib-zip");
    put(join(portable, "Lib", "encodings", "__init__.py"), "base-stdlib");
    put(join(portable, "DLLs", "fixture.pyd"), "base-extension");
    metadata(portable, "setuptools-65.5.0.dist-info", "setuptools", "65.5.0");
    put(join(portable, "Lib", "site-packages", "portable_only.py"), "bootstrap-only");
    metadata(venv, "setuptools-84.0.0.dist-info", "setuptools", "84.0.0");
    metadata(venv, "wheel-0.48.0.dist-info", "wheel", "0.48.0");
    put(join(venv, "Lib", "site-packages", "setuptools", "__init__.py"), "authoritative-package");
    const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;
    const resolve = source.slice(source.indexOf("function Resolve-File("), source.indexOf("function Assert-NoReparsePathChain("));
    const functions = source.slice(source.indexOf("function Copy-Tree("), source.indexOf("function Test-PythonRuntime("));
    assert.ok(functions.includes("Get-PythonDistributionMetadata"));
    const run = (target: string) => spawnSync("pwsh", ["-NoProfile", "-Command", `$ErrorActionPreference='Stop';${resolve};${functions};New-SelfContainedPythonRuntime ${quote(portable)} ${quote(venv)} ${quote(target)}`], { encoding: "utf8", windowsHide: true });
    const valid = run(destination);
    assert.equal(valid.status, 0, `${root}\n${valid.stderr}`);
    assert.deepEqual(readdirSync(join(destination, "Lib", "site-packages")).sort(), ["setuptools", "setuptools-84.0.0.dist-info", "wheel-0.48.0.dist-info"]);
    assert.equal(readFileSync(join(destination, "Lib", "site-packages", "setuptools", "__init__.py"), "utf8"), "authoritative-package");
    for (const [file, expected] of [["python311.dll", "base-dll"], ["python311.zip", "base-stdlib-zip"], ["Lib/encodings/__init__.py", "base-stdlib"], ["DLLs/fixture.pyd", "base-extension"]]) {
      assert.equal(readFileSync(join(destination, file), "utf8"), expected);
    }
    assert.match(readFileSync(join(destination, "python311._pth"), "utf8"), /Lib\\site-packages\r?\nimport site/);
    const occupied = run(destination);
    assert.notEqual(occupied.status, 0);
    assert.match(occupied.stderr, /destination must be fresh/);
    metadata(venv, "Setuptools-65.5.0.dist-info", "Setuptools", "65.5.0");
    const rejectedDestination = join(root, "rejected");
    const duplicate = run(rejectedDestination);
    assert.notEqual(duplicate.status, 0);
    assert.match(duplicate.stderr, /Duplicate active Python distribution metadata: setuptools/);
    assert.equal(existsSync(rejectedDestination), false, "duplicate source must fail before copying");
    passed = true;
  } finally {
    if (passed) rmSync(root, { recursive: true, force: true });
    else console.error(`Retained runtime package composition fixture: ${root}`);
  }
});

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, writeFileSync, mkdirSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("../../governance/scripts/freeze_prepared_payload.ps1", import.meta.url));
const source = readFileSync(script, "utf8");
const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;

test("prepared freeze copies five exact trees, excludes nested Cosy, and refuses overwrite", { skip: process.platform !== "win32" }, () => {
  const parent = fileURLToPath(new URL("../../product/desktop/build_target/tool_artifacts", import.meta.url));
  mkdirSync(parent, { recursive: true });
  const root = mkdtempSync(join(parent, "freeze_prepared_fixture_"));
  let passed = false;
  try {
    const fixtureScript = join(root, "governance", "scripts", "freeze_prepared_payload.ps1");
    mkdirSync(join(root, "governance", "scripts"), { recursive: true });
    writeFileSync(fixtureScript, source);
    const output = join(root, "product", "desktop", "build_target", "offline_payload_cache", "prepared");
    const oldInput = join(output, "prepared_old_input");
    mkdirSync(oldInput, { recursive: true });
    const roots = ["tools", "models", "huggingface", "cosy", "voice"].map(name => join(oldInput, name));
    for (const folder of roots) { mkdirSync(folder); writeFileSync(join(folder, "fixture.bin"), folder); }
    mkdirSync(join(roots[0], "python", "venv_cosyvoice"), { recursive: true });
    writeFileSync(join(roots[0], "python", "venv_cosyvoice", "excluded.bin"), "stale source");
    const before = roots.map(folder => readFileSync(join(folder, "fixture.bin"), "utf8"));
    const args = ["-NoProfile", "-NonInteractive", "-File", fixtureScript,
      "-ToolsDir", roots[0], "-ModelsDir", roots[1], "-HuggingFaceDir", roots[2],
      "-CosyVoiceVenvDir", roots[3], "-VoiceBackendsDir", roots[4], "-PreparedParent", output];
    const first = spawnSync("pwsh", args, { encoding: "utf8", windowsHide: true });
    assert.equal(first.status, 0, `${root}\n${first.stderr}`);
    const receiptPath = first.stdout.trim();
    const receipt = JSON.parse(readFileSync(receiptPath, "utf8"));
    assert.equal(receipt.schema, "voxvulgi.immutable_prepared_payload.v1");
    assert.match(receipt.contract_sha256, /^[A-F0-9]{64}$/);
    assert.equal(receipt.working_runtime_claim, false);
    assert.deepEqual(Object.keys(receipt.trees), ["tools", "models", "huggingface", "cosyvoice_venv", "voice_backends"]);
    assert.equal(receipt.trees.tools.file_count, 1);
    assert.deepEqual(receipt.trees.tools.excluded_prefixes, ["python/venv_cosyvoice"]);
    for (const [index, name] of Object.keys(receipt.trees).entries()) {
      const tree = receipt.trees[name];
      assert.equal(readFileSync(join(tree.root, "fixture.bin"), "utf8"), before[index]);
      assert.match(tree.tree_sha256, /^[A-F0-9]{64}$/);
      assert.equal(tree.expanded_bytes, Buffer.byteLength(before[index]));
    }
    assert.deepEqual(readdirSync(receipt.trees.cosyvoice_venv.root), ["fixture.bin"]);
    const qualifier = readFileSync(new URL("../scripts/qualify_offline_runtime.ps1", import.meta.url), "utf8");
    const resolveFile = qualifier.slice(qualifier.indexOf("function Resolve-File("), qualifier.indexOf("function Assert-NoReparsePathChain("));
    const consumer = qualifier.slice(qualifier.indexOf("function Import-PreparedPayloadIdentity("), qualifier.indexOf("function Copy-Tree("));
    const fixtureScripts = join(root, "offline-installer-runtime", "scripts"); mkdirSync(fixtureScripts, { recursive: true });
    const expectedTrees = Object.entries(receipt.trees).map(([name, tree]: [string, any]) => `${name}=@{root=${quote(tree.root)};excludes=@(${tree.excluded_prefixes.map(quote).join(",")})}`).join(";");
    const consumerScript = join(fixtureScripts, "consume_prepared_fixture.ps1");
    writeFileSync(consumerScript, `$ErrorActionPreference='Stop';${resolveFile};${consumer};Import-PreparedPayloadIdentity ${quote(receiptPath)} @{${expectedTrees}} | ConvertTo-Json -Depth 8 -Compress`);
    const consumed = spawnSync("pwsh", ["-NoProfile", "-NonInteractive", "-File", consumerScript], { encoding: "utf8", windowsHide: true });
    assert.equal(consumed.status, 0, consumed.stderr);
    const actualConsumer = JSON.parse(consumed.stdout.trim());
    assert.equal(actualConsumer.contract_sha256, receipt.contract_sha256.toLowerCase());
    for (const name of Object.keys(receipt.trees)) {
      assert.equal(actualConsumer.trees[name].tree_sha256, receipt.trees[name].tree_sha256.toLowerCase());
      assert.equal(actualConsumer.trees[name].file_count, receipt.trees[name].file_count);
      assert.equal(actualConsumer.trees[name].total_bytes, receipt.trees[name].expanded_bytes);
    }

    assert.deepEqual(roots.map(folder => readFileSync(join(folder, "fixture.bin"), "utf8")), before);
    const second = spawnSync("pwsh", args, { encoding: "utf8", windowsHide: true });
    assert.notEqual(second.status, 0);
    assert.match(second.stderr, /refuse overwrite/);
    assert.deepEqual(readdirSync(output).sort(), [`prepared_${receipt.contract_sha256.toLowerCase()}`, "prepared_old_input"].sort());
    const overlap = spawnSync("pwsh", args.slice(0, -1).concat(roots[0]), { encoding: "utf8", windowsHide: true });
    assert.notEqual(overlap.status, 0);
    assert.match(overlap.stderr, /canonical immutable prepared-payload parent/);
    const equalSourceArgs = [...args]; equalSourceArgs[equalSourceArgs.indexOf("-ToolsDir") + 1] = output;
    const equalSource = spawnSync("pwsh", equalSourceArgs, { encoding: "utf8", windowsHide: true });
    assert.notEqual(equalSource.status, 0); assert.match(equalSource.stderr, /inside or equal to a source tree/);
    passed = true;
  } finally {
    if (passed) {
      const clean = spawnSync("pwsh", ["-NoProfile", "-NonInteractive", "-Command", `$ErrorActionPreference='Stop';Get-ChildItem -LiteralPath ${quote(root)} -File -Recurse -Force | ForEach-Object {$_.IsReadOnly=$false}`], { encoding: "utf8", windowsHide: true });
      assert.equal(clean.status, 0, clean.stderr);
      rmSync(root, { recursive: true, force: true });
    } else console.error(`Retained freeze fixture: ${root}`);
  }
});

test("prepared freeze identity rejects source drift and reparse trees", { skip: process.platform !== "win32" }, () => {
  const parent = fileURLToPath(new URL("../../product/desktop/build_target/tool_artifacts", import.meta.url));
  mkdirSync(parent, { recursive: true });
  const root = mkdtempSync(join(parent, "freeze_prepared_drift_"));
  const fixture = join(root, "fixture");
  mkdirSync(fixture); writeFileSync(join(fixture, "value.bin"), "before");
  const functions = source.slice(source.indexOf("function Assert-OrdinaryPath("), source.indexOf("$parent=Assert-OrdinaryPath"));
  const run = (body: string) => spawnSync("pwsh", ["-NoProfile", "-NonInteractive", "-Command", `$ErrorActionPreference='Stop';${functions};${body}`], { encoding: "utf8", windowsHide: true });
  const changed = run(`$before=Get-FrozenTreeIdentity ${quote(fixture)};[IO.File]::WriteAllText(${quote(join(fixture, "value.bin"))},'after');Assert-IdentityEqual $before (Get-FrozenTreeIdentity ${quote(fixture)}) 'natural source drift'`);
  assert.notEqual(changed.status, 0); assert.match(changed.stderr, /identity changed/);
  const link = join(root, "linked");
  const linked = run(`& cmd.exe /d /c mklink /J ${quote(link)} ${quote(fixture)} | Out-Null;if($LASTEXITCODE -ne 0){throw 'junction fixture failed'};Assert-OrdinaryPath ${quote(link)}`);
  assert.notEqual(linked.status, 0); assert.match(linked.stderr, /ordinary directory ancestor/);
  rmSync(root, { recursive: true, force: true });
});

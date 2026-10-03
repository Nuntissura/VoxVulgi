import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const source = readFileSync(new URL("../scripts/qualify_offline_runtime.ps1", import.meta.url), "utf8");
const functionSource = source.slice(source.indexOf("function Test-PythonRuntime("), source.indexOf("function New-Archive("));
const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;

test("actual qualification probe bounds inherited pipes after its parent exits", () => {
  assert.match(functionSource, /TimeoutMilliseconds = 180000/);
  // Only the import payload is substituted; the production process/deadline code runs unchanged.
  // The owned descendant exits itself after four seconds; no process enumeration or foreign stop.
  const fixture = "import subprocess,sys; subprocess.Popen([sys.executable,'-I','-c','import time; time.sleep(4)'],stdout=sys.stdout,stderr=sys.stderr,creationflags=subprocess.CREATE_NO_WINDOW); print('QUALIFIED_IMPORTS_OK',flush=True)";
  const injected = functionSource.replace(/\$probe = "[^\r\n]*"/, `$probe = ${quote(fixture)}`);
  assert.notEqual(injected, functionSource);
  const command = `$ErrorActionPreference='Stop';${injected};$watch=[Diagnostics.Stopwatch]::StartNew();try{Test-PythonRuntime (Get-Command python -ErrorAction Stop).Source @() 'pipe fixture' 500;$message='unexpected success'}catch{$message=$_.Exception.Message};[ordered]@{elapsed_ms=$watch.ElapsedMilliseconds;message=$message}|ConvertTo-Json -Compress`;
  const result = spawnSync("pwsh", ["-NoProfile", "-Command", command], { encoding: "utf8", windowsHide: true, timeout: 12000 });
  assert.equal(result.status, 0, result.stderr || String(result.error));
  const observation = JSON.parse(result.stdout.trim().split(/\r?\n/).at(-1)!);
  assert.match(observation.message, /redirected output did not close within the 500 millisecond deadline/);
  assert.ok(observation.elapsed_ms < 2500, JSON.stringify(observation));
});

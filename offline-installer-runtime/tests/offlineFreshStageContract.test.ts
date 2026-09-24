import {
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));
const buildTarget = readFileSync(
  join(repoRoot, "governance", "scripts", "build_desktop_target.ps1"),
  "utf8",
);
const guide = readFileSync(join(repoRoot, "offline-installer-runtime", "GUIDE.md"), "utf8");
const cleanupSource = readFileSync(
  join(repoRoot, "offline-installer-runtime", "scripts", "remove_offline_release_attempt.ps1"),
  "utf8",
);
const reconcileSource = readFileSync(
  join(
    repoRoot,
    "offline-installer-runtime",
    "scripts",
    "reconcile_offline_python_environments.ps1",
  ),
  "utf8",
);

test("desktop release build accepts an explicit guaranteed-new offline payload stage", () => {
  assert.match(buildTarget, /\[string\]\$OfflinePayloadStageBaseDir/);
  assert.match(
    buildTarget,
    /OfflinePayloadStageBaseDir requires -RefreshOfflinePayload or -ForceRefreshOfflinePayload/,
  );
  assert.match(
    buildTarget,
    /Join-Path \$buildRoot 'fresh_offline_payload'/,
  );
  assert.match(
    buildTarget,
    /OfflinePayloadStageBaseDir must be one direct child/,
  );
  assert.match(
    buildTarget,
    /OfflinePayloadStageBaseDir must be guaranteed-new/,
  );
  assert.match(buildTarget, /BuildLogPath must be guaranteed-new/);
  assert.match(buildTarget, /build_desktop_target_release_/);
  assert.match(buildTarget, /VoxVulgiOfflineReleaseCurrentGenerationV1/);
  assert.match(buildTarget, /AbandonedMutexException/);
  assert.match(buildTarget, /unresolved full-offline release ownership marker/);
  assert.ok(
    buildTarget.indexOf("$currentGenerationMutex.WaitOne(0)") <
      buildTarget.indexOf("Initialize-DesktopBuildTargetLayout -RepoRoot"),
  );
});

test("nested junction stage is rejected without mutating its external target", () => {
  const nonce = `${process.pid}_${Date.now()}`;
  const allowedRoot = join(
    repoRoot,
    "product",
    "desktop",
    "build_target",
    "fresh_offline_payload",
  );
  const junction = join(allowedRoot, `junction_${nonce}`);
  const external = join(tmpdir(), `voxvulgi_fresh_stage_external_${nonce}`);
  const sentinel = join(external, "outside_sentinel.txt");
  const nestedStage = join(junction, `release_${nonce}`);
  mkdirSync(allowedRoot, { recursive: true });
  mkdirSync(external);
  writeFileSync(sentinel, "must-remain", "utf8");
  try {
    symlinkSync(external, junction, "junction");
    const result = spawnSync(
      "pwsh",
      [
        "-NoProfile",
        "-File",
        join(repoRoot, "governance", "scripts", "build_desktop_target.ps1"),
        "-RefreshOfflinePayload",
        "-OfflinePayloadStageBaseDir",
        nestedStage,
        "-WorkPackets",
        "WP-0308",
      ],
      { cwd: repoRoot, encoding: "utf8" },
    );
    assert.notEqual(result.status, 0);
    assert.match(
      `${result.stdout}\n${result.stderr}`,
      /OfflinePayloadStageBaseDir must be one direct child/,
    );
    assert.equal(readFileSync(sentinel, "utf8"), "must-remain");
    assert.equal(existsSync(nestedStage), false);
  } finally {
    try {
      if (existsSync(junction)) {
        rmSync(junction, { recursive: true, force: true });
      }
    } finally {
      if (existsSync(external)) {
        rmSync(external, { recursive: true, force: true });
      }
    }
  }
  assert.equal(existsSync(junction), false);
  assert.equal(existsSync(external), false);
});

test("fresh payload stage is passed to prep and deleted after a failed build", () => {
  assert.match(
    buildTarget,
    /Invoke-OfflinePayloadPrep[^\r\n]*-StageBaseDir \$resolvedOfflinePayloadStage/,
  );
  assert.match(
    buildTarget,
    /if \(-not \$buildSucceeded -and \$ownsOfflinePayloadStage[^\r\n]*\) \{/,
  );
  assert.match(
    buildTarget,
    /Remove-OwnedOfflinePayloadStage -StagePath \$resolvedOfflinePayloadStage/,
  );
  assert.ok(
    buildTarget.indexOf("$buildSucceeded = $true") <
      buildTarget.indexOf("if (-not $buildSucceeded -and $ownsOfflinePayloadStage"),
  );
});

test("owned direct-child stage is actually deleted after a post-creation failure", () => {
  const nonce = `${process.pid}_${Date.now()}`;
  const stage = join(
    repoRoot,
    "product",
    "desktop",
    "build_target",
    `fresh_offline_payload_selftest_${nonce}`,
    `release_selftest_${nonce}`,
  );
  const selfTestRoot = join(stage, "..");
  const external = join(tmpdir(), `voxvulgi_fresh_stage_sentinel_${nonce}`);
  const sentinel = join(external, "outside_sentinel.txt");
  mkdirSync(external);
  writeFileSync(sentinel, "must-remain", "utf8");
  try {
    const result = spawnSync(
      "pwsh",
      [
        "-NoProfile",
        "-File",
        join(repoRoot, "governance", "scripts", "build_desktop_target.ps1"),
        "-RefreshOfflinePayload",
        "-OfflinePayloadStageBaseDir",
        stage,
        "-OfflinePayloadStageSafetySelfTest",
        "-OfflinePayloadStageSafetySelfTestRoot",
        selfTestRoot,
      ],
      { cwd: repoRoot, encoding: "utf8" },
    );
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
    assert.match(result.stdout, /OFFLINE_PAYLOAD_STAGE_SAFETY_SELFTEST_OK/);
    assert.match(result.stdout, /OFFLINE_PAYLOAD_STAGE_PARENT_SWAP_BLOCKED/);
    assert.match(result.stdout, /OFFLINE_PAYLOAD_STAGE_CHILD_SWAP_BLOCKED/);
    assert.equal(existsSync(stage), false);
    assert.equal(existsSync(selfTestRoot), false);
    assert.equal(readFileSync(sentinel, "utf8"), "must-remain");
  } finally {
    if (existsSync(stage)) rmSync(stage, { recursive: true, force: true });
    if (existsSync(selfTestRoot)) rmSync(selfTestRoot, { recursive: true, force: true });
    if (existsSync(external)) rmSync(external, { recursive: true, force: true });
  }
  assert.equal(existsSync(stage), false);
  assert.equal(existsSync(external), false);
});

test("stale fast-release entrypoint stays deleted and the guide names only the package-only replacement", () => {
  const stalePath = join(
    repoRoot,
    "offline-installer-runtime",
    "scripts",
    "build_offline_release_fast.ps1",
  );
  assert.equal(existsSync(stalePath), false);
  assert.match(guide, /invalid, unwanted stale artifact/i);
  assert.match(guide, /scripts\/package_offline_release\.ps1/i);
  assert.match(guide, /only authorized commands are the two explicit entrypoints/i);
  assert.match(cleanupSource, /Get-BoundDirectoryIdentities/);
  assert.match(cleanupSource, /Current ownership marker SHA-256 mismatch/);
});

test("Python reconciliation is UTF-8 deterministic and preserves native fallback diagnostics", () => {
  assert.match(reconcileSource, /\$env:PYTHONUTF8 = '1'/);
  assert.match(reconcileSource, /\$env:PYTHONIOENCODING = 'utf-8'/);
  assert.match(
    reconcileSource,
    /\$PSNativeCommandUseErrorActionPreference = \$false[\s\S]{0,220}\$output = & \$Python @Arguments 2>&1/,
  );
  assert.match(
    reconcileSource,
    /function Copy-TreeExact[\s\S]{0,600}\$PSNativeCommandUseErrorActionPreference = \$false[\s\S]{0,300}& robocopy\.exe[\s\S]{0,300}\$code -ge 0 -and \$code -le 7/,
  );
  assert.match(reconcileSource, /\$downloadCode = \$LASTEXITCODE/);
  assert.match(reconcileSource, /wheel acquisition failed after pip download exit=/);
  assert.match(reconcileSource, /Remove-Item Env:PYTHONUTF8/);
  assert.match(reconcileSource, /Remove-Item Env:PYTHONIOENCODING/);
  assert.match(reconcileSource, /\.PSObject\.Properties\['direct_url'\]/);
  assert.doesNotMatch(reconcileSource, /\$row\.direct_url/);
});

test("governed attempt cleanup is aggregate and never follows an external link", () => {
  const result = spawnSync(
    "pwsh",
    [
      "-NoProfile",
      "-File",
      join(repoRoot, "offline-installer-runtime", "scripts", "remove_offline_release_attempt.ps1"),
      "-SafetySelfTest",
    ],
    { cwd: repoRoot, encoding: "utf8" },
  );
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  assert.match(result.stdout, /OFFLINE_RELEASE_ATTEMPT_CLEANUP_AGGREGATE_OK/);
  assert.match(result.stdout, /OFFLINE_RELEASE_ATTEMPT_CLEANUP_EXTERNAL_UNCHANGED/);
  assert.match(result.stdout, /OFFLINE_RELEASE_ATTEMPT_CLEANUP_PARENT_SWAP_BLOCKED/);
  assert.match(result.stdout, /OFFLINE_RELEASE_CURRENT_GENERATION_MUTEX_BLOCKED/);
  assert.match(result.stdout, /OFFLINE_RELEASE_DIRECT_PATH_CLEANUP_MUTEX_BLOCKED/);
  assert.match(result.stdout, /OFFLINE_RELEASE_ABANDONED_MUTEX_RECOVERY_OK/);
  assert.match(result.stdout, /OFFLINE_RELEASE_DIRECTORY_IDENTITY_BOUND_OK/);
  assert.match(result.stdout, /OFFLINE_RELEASE_DIRECTORY_IDENTITY_REPARSE_BLOCKED/);
  assert.match(result.stdout, /OFFLINE_RELEASE_CURRENT_MARKER_HASH_MISMATCH_BLOCKED/);
  assert.match(result.stdout, /OFFLINE_RELEASE_CURRENT_CHANGED_DESCENDANT_BLOCKED/);
  assert.match(result.stdout, /OFFLINE_RELEASE_CURRENT_RECREATED_GENERATION_BLOCKED/);
  assert.match(result.stdout, /OFFLINE_RELEASE_PUBLISHER_OUTPUT_PRESERVED_ON_BASE_MISMATCH/);
  assert.match(result.stdout, /OFFLINE_RELEASE_CURRENT_EXACT_GENERATION_CLEANUP_OK/);
  assert.match(result.stdout, /OFFLINE_RELEASE_CURRENT_MARKER_RELEASE_OK/);
  assert.match(result.stdout, /OFFLINE_RELEASE_ATTEMPT_CLEANUP_SELFTEST_OK/);
});

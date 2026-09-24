import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));
const read = (...parts: string[]) => readFileSync(join(repoRoot, ...parts), "utf8");

test("deleted fast entrypoint cannot be selected and every surviving mention marks it invalid", () => {
  const stale = join(
    repoRoot,
    "offline-installer-runtime",
    "scripts",
    "build_offline_release_fast.ps1",
  );
  assert.equal(existsSync(stale), false);
  const guide = read("offline-installer-runtime", "GUIDE.md");
  assert.match(guide, /deleted[^\n]+invalid, unwanted stale artifact/i);
  assert.match(guide, /build_offline_full_installer\.ps1[^\n]+legacy nonconforming/i);
});

test("guide selects and proves the latest yt-dlp through VoxVulgi before unchanged packaging", () => {
  const guide = read("offline-installer-runtime", "GUIDE.md");
  assert.match(guide, /\[VV-INSTALL-022\][^\n]+latest published yt-dlp release[^\n]+version and SHA-256/i);
  assert.match(guide, /\[VV-INSTALL-023\][^\n]+VoxVulgi's real selected-engine resolver[^\n]+exact selected yt-dlp binary/i);
  assert.match(guide, /same selected-engine resolver and yt-dlp adapter[\s\S]{0,400}no-network local-fixture parse invocation/i);
  assert.match(guide, /exact version and SHA-256[\s\S]{0,500}expected structured local-fixture result/i);
  assert.match(guide, /payload_tools\.7z[\s\S]{0,200}no separate yt-dlp archive or installer workflow/i);
  assert.match(guide, /\[VV-INSTALL-025\][^\n]+package-only[^\n]+neither check for nor download an yt-dlp update/i);
});

test("qualification creates reusable non-solid self-contained runtime artifacts separately", () => {
  const source = read(
    "offline-installer-runtime",
    "scripts",
    "qualify_offline_runtime.ps1",
  );
  assert.match(source, /voxvulgi\.qualified_runtime\.v1/);
  assert.match(source, /runtime_main/);
  assert.match(source, /runtime_cosyvoice/);
  assert.match(source, /python\*\._pth/);
  assert.match(source, /python\[0-9\]\[0-9\]\[0-9\]\.dll/);
  assert.match(source, /'Lib', 'DLLs', 'Lib\\site-packages', 'import site'/);
  assert.match(source, /-ms=off/);
  assert.match(source, /QUALIFIED_RUNTIME_REUSED/);
  assert.match(source, /Qualification output may not use the production profile root/);
  assert.match(source, /Assert-NoReparsePathChain \$full 'Qualification output'/);
  assert.match(source, /Assert-DisjointPaths \$output \$source/);
  assert.match(source, /\[Parameter\(Mandatory\)\]\[string\]\$SelectedYtDlpPath/);
  assert.match(source, /\[Parameter\(Mandatory\)\]\[string\]\$SelectedYtDlpVersion/);
  assert.match(source, /\[Parameter\(Mandatory\)\]\[string\]\$SelectedYtDlpSha256/);
  assert.match(source, /\[Parameter\(Mandatory\)\]\[string\]\$PreparedPayloadReceipt/);
  assert.match(source, /voxvulgi\.immutable_prepared_payload\.v1/);
  assert.match(source, /prepared_payload_contract_sha256/);
  assert.match(source, /Get-FileHash -LiteralPath \$selectedYtDlp -Algorithm SHA256/);
  assert.match(source, /Join-Path \$tools 'yt-dlp\\yt-dlp\.exe'/);
  assert.match(source, /selected_ytdlp = \$selectedYtDlpIdentity/);
  assert.match(source, /'tools\/yt-dlp\/yt-dlp\.exe'/);
  assert.doesNotMatch(source, /\bpip(?:3)?(?:\.exe)?\b|Invoke-WebRequest|Start-BitsTransfer|huggingface-cli|hf download/i);
});

test("package entrypoint consumes qualified bytes and cannot build or repair product/runtime inputs", () => {
  const source = read(
    "offline-installer-runtime",
    "scripts",
    "package_offline_release.ps1",
  );
  assert.match(source, /voxvulgi\.qualified_runtime\.v1/);
  assert.match(source, /qualified_runtime_reused=\$true/);
  assert.match(source, /\[IO\.File\]::Open\(\$lockPath, \[IO\.FileMode\]::OpenOrCreate, \[IO\.FileAccess\]::ReadWrite, \[IO\.FileShare\]::Read\)/);
  assert.match(source, /Join-Path \$output 'state\.json'/);
  assert.match(source, /state='CANDIDATE_NOT_RELEASED'/);
  assert.match(source, /New-Item -ItemType HardLink/);
  assert.match(source, /version_and_changelog_unchanged=\$true/);
  assert.match(source, /candidate_requires_exact_iso_acceptance/);
  assert.match(source, /Join-Path \$stage 'simple-offline-installer\.iso'/);
  assert.match(source, /Assert-NoReparsePathChain \$full 'Packaging output'/);
  assert.match(source, /Assert-DisjointPaths \$output \$qualified/);
  assert.match(source, /Refusing to clean a replaced\/reparse package stage/);
  assert.match(source, /\$sevenZip l -slt/);
  assert.doesNotMatch(
    source,
    /(?:^|[;&|]\s*)(?:cargo|npm|npx|pnpm|yarn|pip|python|python3|git|hf|huggingface-cli|tauri)\b/im,
  );
  assert.doesNotMatch(source, /build_desktop_target|prep_offline_bundle|reconcile_offline_python|warmup|Invoke-WebRequest|Start-BitsTransfer/i);
});

test("installer writes an immutable LocalAppData generation and atomically selects its manifest", () => {
  const source = read(
    "offline-installer-runtime",
    "installer",
    "VoxVulgi_offline_full.iss",
  );
  assert.match(source, /UserDataRoot := ExpandConstant\('\{userappdata\}\\com\.voxvulgi\.voxvulgi'\)/);
  assert.match(source, /DataRoot := ExpandConstant\('\{localappdata\}\\com\.voxvulgi\.voxvulgi\\runtime'\)/);
  assert.match(source, /generations\\' \+ RuntimeId/);
  assert.match(source, /RuntimeManifestSHA256/);
  assert.match(source, /MoveFileReplaceExisting or MoveFileWriteThrough/);
  assert.match(source, /current\.json/);
  assert.match(source, /previous\.json/);
  assert.match(source, /RollbackRuntimeActivation/);
  assert.match(source, /PromoteRuntimeGeneration/);
  assert.match(source, /RollbackRuntimeGeneration/);
  assert.match(source, /LongRenameFile\(StageRoot, TargetGeneration\)/);
  assert.doesNotMatch(source, /RewritePyVenvConfig|python\\venv(?:_cosyvoice)?/i);
});

test("PowerShell entrypoints parse without errors", () => {
  for (const relative of [
    "offline-installer-runtime/scripts/qualify_offline_runtime.ps1",
    "offline-installer-runtime/scripts/package_offline_release.ps1",
  ]) {
    const file = join(repoRoot, relative);
    const escaped = file.replaceAll("'", "''");
    const result = spawnSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-Command",
        `$t=$null;$e=$null;[Management.Automation.Language.Parser]::ParseFile('${escaped}',[ref]$t,[ref]$e)|Out-Null;if($e.Count){$e|% Message;exit 1}`,
      ],
      { encoding: "utf8" },
    );
    assert.equal(result.status, 0, `${relative}\n${result.stdout}\n${result.stderr}`);
  }
});

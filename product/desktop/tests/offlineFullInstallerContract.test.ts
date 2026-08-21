import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const desktopRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = path.resolve(desktopRoot, "..", "..");

function readRepoFile(...parts: string[]): string {
  return fs.readFileSync(path.join(repoRoot, ...parts), "utf8");
}

function installerSources(): { definition: string; driver: string } {
  return {
    definition: readRepoFile(
      "product",
      "desktop",
      "src-tauri",
      "installer",
      "VoxVulgi_offline_full.iss",
    ),
    driver: readRepoFile("governance", "scripts", "build_offline_full_installer.ps1"),
  };
}

test("single-unit installer requires Inno Setup 7 and archive extraction", () => {
  const { definition, driver } = installerSources();

  assert.match(
    definition,
    /#if Ver < EncodeVer\(7, 0, 0\)[\s\S]*#error VoxVulgi full-offline installers require Inno Setup 7/,
  );
  assert.match(driver, /Inno Setup 7\\ISCC\.exe/);
  assert.match(
    driver,
    /if \(\$major -lt 7\)[\s\S]*offline Python payload contains installed paths beyond MAX_PATH/,
  );
  assert.doesNotMatch(driver, /Programs\\Inno Setup 6\\ISCC\.exe/);
  assert.match(definition, /^ArchiveExtraction=enhanced\/nopassword$/m);
});

test("public installer is one ISO entrypoint with five external non-solid archives", () => {
  const { definition, driver } = installerSources();

  assert.match(definition, /^OutputBaseFilename=Install_VoxVulgi$/m);
  assert.match(definition, /^DiskSpanning=no$/m);
  assert.match(definition, /^SolidCompression=no$/m);
  assert.match(definition, /^SetupLogging=yes$/m);
  assert.match(definition, /^SetupArchitecture=x64$/m);
  for (const archive of [
    "payload_tools",
    "payload_models",
    "payload_huggingface",
    "payload_cosyvoice_venv",
    "payload_voice_backends",
  ]) {
    assert.match(
      definition,
      new RegExp(
        `Source: "\\{src\\}\\\\payload\\\\${archive}\\.7z";[^\\n]*ExternalSize:[^\\n]*Hash:[^\\n]*Flags: external extractarchive`,
      ),
      `${archive}.7z must be extracted directly from the mounted ISO with truthful progress bytes.`,
    );
  }
  assert.doesNotMatch(definition, /^DiskSpanning=yes$/m);
  assert.doesNotMatch(definition, /^Source:.*\.bin/m);
  assert.doesNotMatch(definition, /\{#PayloadDir\}/);

  assert.match(driver, /VoxVulgi_\{0\}_x64_offline_full\.iso/);
  assert.match(driver, /user_required_download_count = 1/);
  assert.match(driver, /Assert-IsoContents/);
  assert.match(driver, /legacy Inno \.bin slices are present/);
  assert.doesNotMatch(driver, /FinalizeExistingArtifacts/);
  assert.doesNotMatch(driver, /payload_slices/);
});

test("archive build is fast, reusable, integrity-checked, and path-audited", () => {
  const { driver } = installerSources();

  assert.match(driver, /7-Zip 26\.02 or newer is required/);
  assert.match(driver, /The x64 full 7-Zip CLI is required/);
  assert.doesNotMatch(driver, /7zr\.exe/);
  assert.match(driver, /'-mx=1'/);
  assert.match(driver, /'-m0=LZMA2'/);
  assert.match(driver, /'-ms=off'/);
  assert.match(driver, /Reusing content-matched payload archive/);
  assert.match(driver, /@\('t', '-t7z', \$Archive\)/);
  assert.match(driver, /Symbolic Link\|Hard Link/);
  assert.match(driver, /Archive contains an unsafe path/);
  assert.match(driver, /source_sha256_data_and_names/);
  assert.match(driver, /uncompressed_bytes = Get-ArchiveUncompressedBytes/);
  assert.match(driver, /ArchiveSha256=/);
});

test("ISO uses UDF and the wrapper preserves update elevation and durable logs", () => {
  const { definition, driver } = installerSources();

  assert.match(driver, /& \$oscdimg -m -o -u2 -udfver102 \$isoRoot \$isoPath/);
  assert.match(
    definition,
    /Filename: "\{tmp\}\\VoxVulgi_app_setup\.exe";.*BeforeInstall: BeginCoreInstall; AfterInstall: VerifyCoreInstall; Flags: shellexec waituntilterminated/,
  );
  assert.doesNotMatch(
    definition,
    /Filename: "\{tmp\}\\VoxVulgi_app_setup\.exe";[^\n]*Flags:[^\n]*logoutput/,
    "The required shellexec elevation path cannot use Inno's logoutput flag.",
  );
  assert.match(definition, /procedure DeinitializeSetup/);
  assert.match(
    definition,
    /com\.voxvulgi\.voxvulgi\\diagnostics\\installer/,
    "Installer diagnostics must survive temp-folder cleanup.",
  );
});

test("installer logging proves payload boundaries and the installed core version", () => {
  const { definition } = installerSources();

  for (const phase of [
    "tools",
    "models",
    "huggingface",
    "cosyvoice_venv",
    "voice_backends",
  ]) {
    assert.match(
      definition,
      new RegExp(`BeginPayloadPhase\\('${phase}',[^\\n]*CompletePayloadPhase\\('${phase}'\\)`),
      `${phase} must log both its start and its completion.`,
    );
  }

  for (const event of [
    "wrapper_started",
    "payload_phase_started",
    "payload_phase_completed",
    "core_installer_started",
    "core_installer_returned",
    "installed_state",
    "core_install_verification",
    "installer_finished",
  ]) {
    assert.match(definition, new RegExp(`VV_INSTALLER_EVENT ${event}`));
  }

  assert.match(definition, /installer_\{#AppVersion\}_latest\.log/);
  assert.match(definition, /GetDateTimeString\('yyyymmdd_hhnnss'/);
  assert.match(definition, /RegQueryStringValue\(HKLM64/);
  assert.match(definition, /RegQueryStringValue\(HKLM32/);
  assert.match(definition, /GetVersionNumbersString\(InstalledBinary, BinaryVersion\)/);
  assert.match(definition, /observed_registry_version=/);
  assert.match(definition, /observed_binary_version=/);
  assert.match(definition, /RaiseException\('VoxVulgi application installation verification failed:/);
});

test("performance and user-data boundaries are governed by executable checks", () => {
  const { definition } = installerSources();
  const performance = readRepoFile(
    "governance",
    "scripts",
    "test_offline_installer_performance.ps1",
  );

  assert.match(definition, /^Uninstallable=no$/m);
  assert.doesNotMatch(definition, /Remove-Item|DelTree|DeleteFile/i);
  assert.match(performance, /\[double\]\$MinimumSpeedup = 2\.0/);
  assert.match(performance, /legacy_raw_seconds/);
  assert.match(performance, /external_archive_seconds/);
  assert.match(performance, /Fixture output mismatch/);
  assert.match(performance, /Refusing fixture cleanup outside the bounded WP-0308 temp root/);
});

test("one canonical installer manual is linked from every startup authority surface", () => {
  const manualPath = "governance/release/OFFLINE_INSTALLER_BUILD_MANUAL.md";
  const manual = readRepoFile("governance", "release", "OFFLINE_INSTALLER_BUILD_MANUAL.md");

  for (const startupFile of ["AGENTS.md", "CLAUDE.md", "PROJECT_CODEX.md", "build_rules.md"]) {
    assert.match(
      readRepoFile(startupFile),
      new RegExp(manualPath.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")),
      `${startupFile} must route a no-context installer builder to the canonical manual.`,
    );
  }
  assert.match(manual, /^file_id: VV-INSTALLER-BUILD-MANUAL$/m);
  assert.match(manual, /<topic id="build-procedure"[^>]*ingestable="true"/);
  assert.match(manual, /build_desktop_target\.ps1[\s\S]*build_offline_full_installer\.ps1/);
  assert.match(manual, /user_required_download_count: 1/);
  assert.match(manual, /Do not publish until all are true/);
  assert.match(
    readRepoFile(
      "governance",
      "workflow",
      "work_packets",
      "WP-0265_INSTALLER_PACKAGING_THIN_NSIS_AND_OFFLINE_PLAN.md",
    ),
    /Historical record only[\s\S]*OFFLINE_INSTALLER_BUILD_MANUAL\.md/,
  );
});

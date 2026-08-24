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

test("public installer is one ISO entrypoint with five external bounded-solid archives", () => {
  const { definition, driver } = installerSources();

  assert.match(definition, /^OutputBaseFilename=Install_VoxVulgi$/m);
  assert.match(definition, /^DiskSpanning=no$/m);
  assert.match(definition, /^SolidCompression=no$/m);
  assert.match(definition, /^SetupLogging=yes$/m);
  assert.match(definition, /^SetupArchitecture=x64$/m);
  const payloadMappings = [
    {
      phase: "tools",
      archive: "payload_tools.7z",
      destination: "tools",
      hash: "ToolsArchiveSha256",
    },
    {
      phase: "models",
      archive: "payload_models.7z",
      destination: "models",
      hash: "ModelsArchiveSha256",
    },
    {
      phase: "huggingface",
      archive: "payload_huggingface.7z",
      destination: "cache\\huggingface",
      hash: "HuggingFaceArchiveSha256",
    },
    {
      phase: "cosyvoice_venv",
      archive: "payload_cosyvoice_venv.7z",
      destination: "tools\\python\\venv_cosyvoice",
      hash: "CosyVoiceVenvArchiveSha256",
    },
    {
      phase: "voice_backends",
      archive: "payload_voice_backends.7z",
      destination: "voice_backends",
      hash: "VoiceBackendsArchiveSha256",
    },
  ] as const;
  const queuedPayloadMappings = [
    ...definition.matchAll(
      /VerifyAndQueuePayload\(\s*'([^']+)'\s*,\s*'([^']+)'\s*,\s*'([^']+)'\s*,\s*'\{#([^}]+)\}'\s*\)/g,
    ),
  ].map((match) => ({
    phase: match[1],
    archive: match[2],
    destination: match[3],
    hash: match[4],
  }));
  assert.deepEqual(
    queuedPayloadMappings,
    payloadMappings,
    "The extraction queue must contain exactly the five governed mappings, once each and in order.",
  );
  for (const { phase, archive, destination, hash } of payloadMappings) {
    const escaped = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    assert.match(
      definition,
      new RegExp(
        `VerifyAndQueuePayload\\(\\s*'${escaped(phase)}'\\s*,\\s*` +
          `'${escaped(archive)}'\\s*,\\s*'${escaped(destination)}'\\s*,\\s*` +
          `'\\{#${escaped(hash)}\\}'\\s*\\)`,
      ),
      `${archive} must map to only its governed destination and SHA-256 define.`,
    );
  }
  assert.match(definition, /CreateExtractionPage\([\s\S]*@PayloadExtractionProgress\)/);
  assert.match(definition, /PayloadExtractionPage\.Add\(ArchivePath, DestinationPath, True\)/);
  assert.match(definition, /PayloadExtractionPage\.Extract/);
  assert.match(definition, /PayloadGenerationId|CreatePayloadGeneration/i);
  assert.match(definition, /installer_payload_stage_[^'"\s+]+|installer_payload_stage_'\s*\+/i);
  assert.match(definition, /installer_payload_backup_[^'"\s+]+|installer_payload_backup_'\s*\+/i);
  const activeGenerationRootFunctions = definition.match(
    /function PayloadStageRoot\b[\s\S]*?\nend;[\s\S]*?function PayloadBackupRoot\b[\s\S]*?\nend;/i,
  )?.[0];
  assert.ok(activeGenerationRootFunctions);
  assert.doesNotMatch(activeGenerationRootFunctions, /installer_payload_(?:stage|backup)_current/i);
  const managedRoots = ["tools", "models", "cache\\huggingface", "voice_backends"];
  const promotedRoots = [
    ...definition.matchAll(
      /PromoteManagedPayloadRoot\(\s*'([^']+)'\s*(?:,\s*'[^']+'\s*)?\)/g,
    ),
  ].map((match) => match[1]);
  assert.deepEqual(
    promotedRoots,
    managedRoots,
    "Promotion must cover exactly the four complete managed roots, once each and in order.",
  );
  for (const root of managedRoots) {
    assert.match(
      definition,
      new RegExp(
        `PromoteManagedPayloadRoot\\(\\s*'${root.replace(/\\/g, "\\\\")}'` +
          `\\s*(?:,\\s*'[^']+'\\s*)?\\)`,
      ),
      `The complete ${root} managed root must be promoted as one governed unit.`,
    );
  }
  assert.doesNotMatch(
    definition,
    /Flags:[^\n]*extractarchive/,
    "The per-member [Files] extraction path regresses Python-tree install speed.",
  );
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

test("runtime installer coordination is single-writer and closes the shipped app", () => {
  const { definition } = installerSources();

  assert.match(definition, /^SetupMutex=\S+$/m);
  assert.match(definition, /^CloseApplications=(?:yes|force)$/m);
  assert.match(definition, /^CloseApplicationsFilter=.*(?:desktop\.exe|VoxVulgi.*\.exe).*$/mi);
});

test("payload promotion is journaled and rolls back all managed roots as a unit", () => {
  const { definition } = installerSources();

  assert.match(definition, /payload_(?:promotion_)?journal/i);
  assert.match(definition, /WritePayloadJournal|PersistPayloadJournal|SavePayloadJournal/i);
  assert.match(definition, /Rollback(?:All)?ManagedPayloadRoots|RollbackPayloadPromotion/i);
  assert.match(definition, /VV_INSTALLER_EVENT payload_promotion_(?:started|journaled)/);
  assert.match(definition, /VV_INSTALLER_EVENT payload_promotion_(?:completed|committed)/);
  assert.match(definition, /VV_INSTALLER_EVENT payload_promotion_(?:failed|rolled_back)/);

  const rollbackBody = definition.match(
    /procedure\s+(?:Rollback(?:All)?ManagedPayloadRoots|RollbackPayloadPromotion)\b[\s\S]*?\nend;/i,
  )?.[0];
  assert.ok(rollbackBody, "The installer must have one explicit all-root rollback routine.");
  for (const root of ["tools", "models", "cache\\huggingface", "voice_backends"]) {
    assert.match(
      rollbackBody,
      new RegExp(root.replace(/\\/g, "\\\\")),
      `Rollback must cover ${root}, not just the root whose promotion failed.`,
    );
  }
});

test("payload extraction preflights disk capacity and accounts for every archive", () => {
  const { definition } = installerSources();

  for (const archiveBytes of [
    "ToolsArchiveBytes",
    "ModelsArchiveBytes",
    "HuggingFaceArchiveBytes",
    "CosyVoiceVenvArchiveBytes",
    "VoiceBackendsArchiveBytes",
  ]) {
    assert.match(
      definition,
      new RegExp(`\\{#${archiveBytes}\\}`),
      `${archiveBytes} must participate in the runtime disk calculation.`,
    );
  }
  assert.match(definition, /GetSpaceOnDisk64\s*\(/);
  assert.match(definition, /VV_INSTALLER_EVENT payload_disk_preflight/);
  assert.match(definition, /required_bytes=/);
  assert.match(definition, /free_bytes=/);
  assert.match(definition, /insufficient.*(?:disk|space)|(?:disk|space).*insufficient/i);
});

test("every installer terminal path is durable and log copies are checked", () => {
  const { definition } = installerSources();

  assert.match(definition, /VV_INSTALLER_EVENT payload_(?:extraction|transaction)_terminal/);
  for (const event of [
    "payload_promotion_terminal",
    "core_install_terminal",
    "core_install_verification",
    "installer_finished",
  ]) {
    assert.match(definition, new RegExp(`VV_INSTALLER_EVENT ${event}`));
  }
  assert.match(definition, /InstallerOutcome\s*:=\s*'success'/);
  assert.match(definition, /InstallerOutcome\s*:=\s*'[^'\r\n]*cancelled[^'\r\n]*'/);
  assert.match(definition, /InstallerOutcome\s*:=\s*'[^'\r\n]*failed[^'\r\n]*'/);
  assert.doesNotMatch(
    definition,
    /^\s*CopyFile\s*\(/m,
    "Durable checkpoint/final-log copies must never ignore CopyFile failure.",
  );
  assert.match(definition, /if not CopyFile\(SourceLog, DestLog, False\) then/);
  assert.match(definition, /VV_INSTALLER_EVENT (?:log_copy_failed|installer_log_persist_failed)/);
  assert.match(
    definition,
    /if not CopyFile\(SourceLog, DestLog, False\) then[\s\S]{0,420}RaiseException/,
    "A failed checkpoint or final-log copy must abort instead of silently losing durable diagnostics.",
  );
  assert.match(
    definition,
    /RaiseException\('Unable to persist the durable installer checkpoint: ' \+ DestLog\)/,
  );
  assert.match(
    definition,
    /RaiseException\('Unable to persist the final installer log: ' \+ DestLog\)/,
  );
});

test("archive build is fast, reusable, integrity-checked, and path-audited", () => {
  const { driver } = installerSources();

  assert.match(driver, /7-Zip 26\.02 or newer is required/);
  assert.match(driver, /The x64 full 7-Zip CLI is required/);
  assert.doesNotMatch(driver, /7zr\.exe/);
  assert.match(driver, /'-mx=1'/);
  assert.match(driver, /'-m0=LZMA2'/);
  assert.match(driver, /'-ms=64m'/);
  assert.doesNotMatch(driver, /'-ms=off'/);
  assert.match(driver, /'-mtm=off'/);
  assert.match(driver, /'-mtr=off'/);
  assert.match(driver, /bounded_solid_lzma2_fast_64m_no_restorable_metadata_v3/);
  assert.match(driver, /archive_policy = \$archivePolicy/);
  assert.match(driver, /Assert-ArchiveOmitsRestorableFileMetadata/);
  assert.match(driver, /restorable per-file metadata forbidden by the fast extraction policy/);
  assert.match(driver, /function Get-ArchiveBoundedSolidAudit/);
  assert.match(
    driver,
    /& \$Executable @Arguments 2>&1 \| ForEach-Object \{ Write-Host "\$_" \}/,
    "non-capture 7-Zip diagnostics must not leak into structured archive return values",
  );
  assert.match(driver, /@\('l', '-slt', '-t7z', \$Archive\)/);
  assert.match(driver, /\$blockStats\.Count -ne \$summaryBlockCount/);
  assert.match(
    driver,
    /\$stats\.uncompressed_bytes -gt \$BlockLimitBytes[\s\S]{0,240}\$stats\.file_count -ne 1/,
  );
  assert.match(driver, /one oversized singleton file/);
  assert.match(
    driver,
    /\(-not \$solid\)[\s\S]{0,100}\$maxBlockFileCount -gt 1[\s\S]{0,180}does not report the governed bounded-solid mode/,
  );
  assert.match(driver, /solid_block_limit_bytes = \$solidAudit\.solid_block_limit_bytes/);
  assert.match(driver, /block_count = \$solidAudit\.block_count/);
  assert.match(driver, /max_block_uncompressed_bytes = \$solidAudit\.max_block_uncompressed_bytes/);
  assert.match(driver, /oversized_singleton_block_count = \$solidAudit\.oversized_singleton_block_count/);
  assert.match(
    driver,
    /archive_policy = 'bounded_solid_lzma2_fast_64m_no_restorable_metadata_v3'/,
  );
  assert.match(driver, /realized_solid_archive_count = \$realizedSolidArchiveCount/);
  assert.match(
    driver,
    /realized_non_solid_archive_count = \(\$archives\.Count - \$realizedSolidArchiveCount\)/,
  );
  assert.match(
    driver,
    /all_archives_realized_solid = \(\$realizedSolidArchiveCount -eq \$archives\.Count\)/,
  );
  assert.doesNotMatch(driver, /archive_format = '7z'; solid = \$true/);
  assert.match(driver, /compression_profile = 'fast-lzma2-bounded-solid-64m'/);
  assert.match(driver, /Reusing content-matched payload archive/);
  assert.match(driver, /@\('t', '-t7z', \$Archive\)/);
  assert.match(driver, /Symbolic Link\|Hard Link/);
  assert.match(driver, /Archive contains an unsafe path/);
  assert.match(driver, /source_sha256_data_and_names/);
  assert.match(driver, /uncompressed_bytes = Get-ArchiveUncompressedBytes/);
  assert.match(driver, /ArchiveSha256=/);
  assert.match(driver, /offline_full_build_\{0\}_\{1\}\.log/);
  assert.match(driver, /Start-Transcript -LiteralPath \$buildTranscriptPath -Force/);
  assert.match(driver, /tools\s*=\s*\[ordered\]@\{[\s\S]{0,420}iscc[\s\S]{0,420}seven_zip[\s\S]{0,420}oscdimg/);
  assert.match(driver, /Where-Object \{ \$_ \}/);
  assert.match(driver, /Unable to resolve non-empty Oscdimg provenance/);
});

test("ISO uses UDF and the wrapper preserves update elevation and durable logs", () => {
  const { definition, driver } = installerSources();

  assert.match(definition, /five bounded-solid-policy \.7z archives/);
  assert.match(definition, /external 7z payloads use a separately[\s\S]{0,120}64 MiB bounded-solid policy/);
  assert.doesNotMatch(definition, /five non-solid \.7z archives/);
  assert.doesNotMatch(definition, /external payload archives non-solid/);
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
      new RegExp(`VerifyAndQueuePayload\\('${phase}',`),
      `${phase} must enter the shared verified bulk-extraction path.`,
    );
  }
  assert.match(definition, /BeginPayloadPhase\(PhaseName, Message\)/);
  assert.match(definition, /CompletePayloadPhase\(ActivePayloadPhase\)/);

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
  assert.doesNotMatch(definition, /Remove-Item/i);
  const deletedFiles = [...definition.matchAll(/DeleteFile\(([^)]+)\)/g)].map((match) =>
    match[1].trim(),
  );
  assert.ok(deletedFiles.length > 0, "Completed/recovered transaction journals must be removed.");
  assert.ok(
    deletedFiles.every((target) => target === "PayloadJournalPath"),
    `Only the owned transaction journal may be deleted; found: ${deletedFiles.join(", ")}`,
  );
  assert.doesNotMatch(definition, /DelTree\(PayloadBaseDir/);
  assert.match(definition, /DeleteManagedPayloadTree\(PayloadStageRoot/);
  assert.match(definition, /DeleteManagedPayloadTree\(PayloadBackupRoot/);
  assert.match(performance, /\[double\]\$MinimumSpeedup = 2\.0/);
  assert.match(performance, /schema_version = 3/);
  assert.match(performance, /legacy_raw_trial_seconds/);
  assert.match(performance, /bulk_archive_trial_seconds/);
  assert.match(performance, /legacy_raw_median_seconds/);
  assert.match(performance, /bulk_archive_median_seconds/);
  assert.match(performance, /trial_count_per_mode/);
  assert.match(performance, /production_shaped_t_extraction_wizard_page_bounded_solid_64m/);
  assert.match(performance, /'-mtm=off'/);
  assert.match(performance, /'-mtr=off'/);
  assert.match(performance, /restorable_file_timestamps = \$false/);
  assert.match(performance, /restorable_file_attributes = \$false/);
  assert.match(performance, /Assert-FixtureArchiveOmitsRestorableFileMetadata/);
  assert.match(performance, /Fixture archive retained forbidden restorable file metadata/);
  assert.match(performance, /tree identity mismatch/);
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
  assert.match(manual, /-TauriArgs '--bundles=nsis'/);
  assert.match(manual, /three explicitly configured build-tool executable paths/);
  assert.match(manual, /offline_full_build_<timestamp>_<version>\.log/);
  assert.match(manual, /user_required_download_count: 1/);
  assert.match(manual, /Do not publish until all are true/);
  assert.match(manual, /setup mutex[\s\S]*second writer/i);
  assert.match(manual, /generation-specific staging and backup paths/i);
  assert.match(manual, /persistent promotion journal/i);
  assert.match(manual, /all four managed roots/i);
  assert.match(manual, /Insufficient disk:[\s\S]*before extraction/i);
  assert.match(manual, /Locked VoxVulgi runtime:[\s\S]*close-application flow/i);
  assert.match(manual, /Core-installer failure after payload promotion:/i);
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

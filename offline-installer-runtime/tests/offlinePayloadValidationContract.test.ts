import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = fileURLToPath(new URL("../..", import.meta.url));
const read = (relative: string) => fs.readFileSync(path.join(root, relative), "utf8");

test("fresh payload validator is hash-bound and fail-closed", () => {
  const source = read("product/engine/src/offline_payload_validation.rs");
  assert.match(source, /voxvulgi\.offline_payload_validation\.v1/);
  assert.match(source, /refusing to replace an existing payload-validation receipt/);
  assert.match(source, /FILE_ATTRIBUTE_REPARSE_POINT/);
  assert.match(source, /stage\/export tree mismatch/);
  assert.match(source, /ModelStore::new/);
  assert.match(source, /verify_model_by_id\("whispercpp-tiny"\)/);
  assert.match(source, /required_weight_files[\s\S]*Demucs/);
  assert.match(source, /pip[\s\S]*inspect[\s\S]*--local/);
  assert.match(source, /editable, direct-URL, or non-exact/);
  assert.match(source, /complete environment lock[\s\S]*does not equal pip inspect inventory/);
  assert.match(source, /torch_cuda_build/);
  assert.match(source, /exclusive_source_lock_double_sha256_v1/);
  assert.match(source, /assert_held_for_final_rehash/);
  assert.match(source, /ensure_validation_window_unchanged/);
  assert.match(source, /initial_lock_set_sha256/);
  assert.match(source, /owner_alive_at_final_rehash/);
  assert.match(source, /verify_external_source_lock_record/);
  assert.match(source, /record_bytes_verified/);
  assert.match(source, /file\.sync_all\(\)/);
});

test("environment reconciliation installs only exact hashed wheels", () => {
  const source = read("offline-installer-runtime/scripts/reconcile_offline_python_environments.ps1");
  assert.match(source, /pip', 'inspect', '--local'/);
  assert.match(source, /pip', 'freeze', '--all'/);
  assert.match(source, /--only-binary=:all:/);
  assert.match(source, /--require-hashes/);
  assert.match(source, /--no-index/);
  assert.match(source, /--require-hashes', '--only-binary=:all:', '--no-deps', '-r'/);
  assert.match(source, /& \$Python -m pip check/);
  assert.match(source, /Assert-EquivalentPipCheck/);
  assert.match(source, /rebuilt pip-check diagnostics differ from the canonical source environment/);
  assert.match(source, /https:\/\/pypi\.org\/simple/);
  assert.match(source, /https:\/\/download\.pytorch\.org\/whl\/cpu/);
  assert.match(source, /Assert-TrustedDescendantPath -Path \$wheelPath -TrustedRoot \$TrustedDirectWheelRoot/);
  assert.match(source, /direct wheel package mismatch/);
  assert.match(source, /direct wheel version mismatch/);
  assert.match(source, /direct wheel fragment hash mismatch/);
  assert.match(source, /governed en-core-web-sm byte-size drift/);
  assert.match(source, /governed en-core-web-sm SHA-256 drift/);
  assert.match(source, /governed OpenVoice SHA-256 drift/);
  assert.match(source, /governed_pure_wheels\.manifest\.json/);
  assert.match(source, /https:\/\/github\.com\/explosion\/spacy-models\/releases\/download\/en_core_web_sm-3\.8\.0\/en_core_web_sm-3\.8\.0-py3-none-any\.whl/);
  assert.match(source, /\$downloadCode -ne 0 -or \$downloadedMatch\.Count -eq 0/);
  assert.match(source, /matching_wheels=\$\(\$downloadedMatch\.Count\)/);
  assert.match(source, /cp\(3\[2-9\]\|310\|311\)-abi3-win_amd64/);
  assert.match(source, /\$isExactCp311OrUniversal -or \$isCompatibleAbi3/);
  assert.match(source, /Remove-ExactTree \$wheelRoot/);
  assert.match(source, /\.voxvulgi_python_environment_transactions/);
  assert.match(source, /prepared = \(Join-Path \$workspaceFull 'prepared'\)/);
  assert.match(source, /payload validator failed after environment reconciliation/);
});

test("CosyVoice source-only ANTLR dependency becomes a governed wheel without executing source", () => {
  const source = read("product/engine/resources/tooling/prepare_cosyvoice_wheelhouse.py");
  const requirements = read("product/engine/resources/tooling/requirements.cosyvoice.txt");
  assert.match(source, /antlr4-python3-runtime[\s\S]*4\.9\.3/);
  assert.match(source, /f224469b4168294902bb1efa80a8bf7855f24c99aef99cbefc1bcd3cce77881b/);
  assert.match(source, /urllib\.request\.ProxyHandler\(\{\}\)/);
  assert.match(source, /source\.extractfile\(member\)/);
  assert.match(source, /Generator: voxvulgi-hash-pinned-source-wheel-v1/);
  assert.match(source, /validate_wheel\(wheel_path\)/);
  assert.match(source, /"governed_source_wheels": governed_source_wheels/);
  assert.match(source, /612ee75c546f53e92e70049c9dbfcc18c935a2b9a53b66085ce9ef6a6e5c0934/);
  assert.match(source, /dda83a855986efa5cd87f0248b0199c0086eb0e8e7fece7d6741959c5ce39536/);
  assert.match(source, /APPROVED_PTH_HOOKS/);
  assert.match(source, /if item\.is_dir\(\):/);
  assert.match(source, /directory_mode not in \{0, stat\.S_IFDIR\}/);
  assert.doesNotMatch(source, /setup\.py|bdist_wheel/);
  assert.doesNotMatch(source, /run_pip\([\s\S]{0,160}["']wheel["']/);
  assert.match(requirements, /^protobuf==4\.25\.0$/m);
  assert.doesNotMatch(requirements, /^protobuf==4\.25$/m);
});

test("environment reconciliation is journaled, exclusive, and generation-atomic", () => {
  const source = read("offline-installer-runtime/scripts/reconcile_offline_python_environments.ps1");
  assert.match(source, /FileAccess\]::ReadWrite, \[IO\.FileShare\]::Read/);
  assert.match(source, /voxvulgi\.offline_payload_source_lock\.v1/);
  assert.match(source, /voxvulgi\.python_environment_transaction\.v2/);
  assert.match(source, /voxvulgi\.python_environment_transaction_root_owner\.v1/);
  assert.match(source, /voxvulgi\.python_environment_transaction_unit_owner\.v1/);
  assert.match(source, /GetFileInformationByHandleEx/);
  assert.match(source, /collection_root_id/);
  assert.match(source, /transaction_root_id/);
  assert.match(source, /workspace_id/);
  assert.match(source, /prepared_directory_id/);
  assert.match(source, /RECONCILIATION_V2_TOPOLOGY_SELFTEST_OK/);
  assert.match(source, /RECONCILIATION_EXPORT_MANIFEST_SELFTEST_OK/);
  assert.match(source, /function Get-ExactPayloadTreeBytes/);
  assert.match(source, /function Update-ExportManifestPayloadBytes/);
  assert.match(source, /Export manifest payload_bytes refreshed after reconciliation/);
  assert.match(source, /-RollbackAction \$manifestRollbackAction/);
  assert.match(source, /Update-ExportManifestPayloadBytes[\s\S]*?& \$validator @validatorArgs/);
  assert.match(source, /preparation_intent_durable/);
  assert.match(source, /backup_intent_durable/);
  assert.match(source, /publish_intent_durable/);
  assert.match(source, /validator_intent_durable/);
  assert.match(source, /committed_at_utc/);
  assert.match(source, /Complete-CommittedCleanup/);
  assert.match(source, /Recover-Transaction/);
  assert.match(source, /restart_recovery_uncommitted/);
  assert.match(source, /restart_recovery_committed/);
  assert.match(source, /New-V2TransactionUnit -Name 'lock_set'/);
  assert.match(source, /product\\desktop\\build_target\\\.voxvulgi_offline_payload_source\.lock/);
  assert.doesNotMatch(source, /Copy-Item -LiteralPath \$mainLockTemp -Destination/);
  for (const boundary of [
    "before_main_backup_move", "after_main_backup_move",
    "before_main_publish_move", "after_main_publish_move",
    "before_cosyvoice_backup_move", "after_cosyvoice_backup_move",
    "before_cosyvoice_publish_move", "after_cosyvoice_publish_move",
    "before_export_backup_move", "after_export_backup_move",
    "before_export_publish_move", "after_export_publish_move",
    "before_lock_set_backup_move", "after_lock_set_backup_move",
    "before_lock_set_publish_move", "after_lock_set_publish_move",
    "before_validator", "after_validator", "before_commit", "after_commit",
  ]) {
    assert.match(source, new RegExp(boundary));
  }
});

test("pinned manifest names both complete environment locks", () => {
  const manifest = JSON.parse(read("product/engine/resources/tooling/pinned_dependency_manifest.json"));
  assert.deepEqual(manifest.offline_python_environment_locks, {
    main_windows_x64_cp311: "final_environment_locks/main_windows_x64_cp311.lock.json",
    cosyvoice_windows_x64_cp311: "final_environment_locks/cosyvoice_windows_x64_cp311.lock.json",
  });
});

test("pack locks agree on every package installed into the shared main environment", () => {
  const toolingRoot = "product/engine/resources/tooling";
  const manifest = JSON.parse(read(`${toolingRoot}/pinned_dependency_manifest.json`));
  const engineSource = read("product/engine/src/tools.rs");
  const normalizedEngineSource = engineSource.replaceAll("_", "");
  const seen = new Map<string, { pack: string; version: string; url: string; sha256: string }>();

  for (const [pack, lockRelative] of Object.entries(manifest.lockfiles as Record<string, string>)) {
    const lock = JSON.parse(read(`${toolingRoot}/${lockRelative}`));
    const buildBackend = lock.source_build?.build_backend;
    if (buildBackend) {
      const lockedBackend = lock.packages.find(
        (packageRecord: { name: string }) =>
          String(packageRecord.name).toLowerCase().replaceAll("_", "-") ===
          String(buildBackend.name).toLowerCase().replaceAll("_", "-"),
      );
      assert.ok(lockedBackend, `${pack} source-build backend is absent from its package lock`);
      assert.deepEqual(
        { version: buildBackend.version, url: buildBackend.url, sha256: buildBackend.sha256 },
        { version: lockedBackend.version, url: lockedBackend.url, sha256: lockedBackend.sha256 },
        `${pack} source-build backend differs from its package lock`,
      );
      assert.ok(
        engineSource.includes(`backend.version != "${buildBackend.version}"`) &&
          engineSource.includes(`backend.filename != "${buildBackend.filename}"`) &&
          normalizedEngineSource.includes(`backend.filebytes != ${buildBackend.file_bytes}`) &&
          engineSource.includes(`backend.sha256 != "${buildBackend.sha256}"`),
        `${pack} runtime source-build contract differs from its package lock`,
      );
    }
    for (const sourcePin of lock.source_pins ?? []) {
      const separator = String(sourcePin).lastIndexOf("==");
      assert.ok(separator > 0, `${pack} source pin is not exact: ${sourcePin}`);
      const name = String(sourcePin).slice(0, separator).toLowerCase().replaceAll("_", "-");
      const version = String(sourcePin).slice(separator + 2);
      const lockedPackage = lock.packages.find(
        (packageRecord: { name: string }) =>
          String(packageRecord.name).toLowerCase().replaceAll("_", "-") === name,
      );
      assert.ok(lockedPackage, `${pack} source pin ${sourcePin} is absent from its package lock`);
      assert.equal(
        lockedPackage.version,
        version,
        `${pack} source pin ${sourcePin} differs from its package lock`,
      );
    }
    if (Array.isArray(lock.source_pins) && Array.isArray(manifest[pack]?.pinned)) {
      const manifestSourcePins = [
        ...(manifest[pack].compatibility_upgrades ?? []),
        ...manifest[pack].pinned,
      ];
      assert.deepEqual(lock.source_pins, manifestSourcePins, `${pack} source pins differ from its manifest pins`);
    }
    for (const packageRecord of lock.packages) {
      const name = String(packageRecord.name).toLowerCase().replaceAll("_", "-");
      const current = {
        pack,
        version: String(packageRecord.version),
        url: String(packageRecord.url),
        sha256: String(packageRecord.sha256),
      };
      const prior = seen.get(name);
      if (prior) {
        assert.deepEqual(
          { version: current.version, url: current.url, sha256: current.sha256 },
          { version: prior.version, url: prior.url, sha256: prior.sha256 },
          `${name} differs between shared-environment pack locks ${prior.pack} and ${current.pack}`,
        );
      } else {
        seen.set(name, current);
      }
    }
  }
});

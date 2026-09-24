import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const desktopRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = join(desktopRoot, "..", "..");
const installer = readFileSync(
  join(desktopRoot, "src-tauri", "installer", "templates", "installer.nsi"),
  "utf8",
);
const language = readFileSync(
  join(desktopRoot, "src-tauri", "installer", "languages", "English.nsh"),
  "utf8",
);
const config = JSON.parse(
  readFileSync(join(desktopRoot, "src-tauri", "tauri.conf.json"), "utf8"),
);
const productSpec = readFileSync(
  join(repoRoot, "governance", "spec", "PRODUCT_SPEC.md"),
  "utf8",
);
const technicalDesign = readFileSync(
  join(repoRoot, "governance", "spec", "TECHNICAL_DESIGN.md"),
  "utf8",
);

test("reinstall actions remain in the CRC-checked parent installer", () => {
  const leavePage = installer.slice(
    installer.indexOf("Function PageLeaveReinstall"),
    installer.indexOf("FunctionEnd", installer.indexOf("Function PageLeaveReinstall")),
  );
  assert.match(
    leavePage,
    /\$MaintenanceAction == "reinstall_keep"[\s\S]*\$MaintenanceAction == "full_reinstall"[\s\S]*maintenance_reinstall_in_process[\s\S]*Goto reinst_done/,
  );
  assert.doesNotMatch(leavePage, /\$MaintenanceAction == "reinstall_keep"[\s\S]{0,180}\/UPDATE/);
  assert.doesNotMatch(leavePage, /\$MaintenanceAction == "full_reinstall"[\s\S]{0,180}\/UPDATE/);
});

test("reinstall replaces managed app files and only full reinstall removes user data", () => {
  const installSection = installer.slice(
    installer.indexOf("Section Install"),
    installer.indexOf("SectionEnd", installer.indexOf("Section Install")),
  );
  assert.match(installSection, /reinstall_cleanup_begin/);
  assert.match(installSection, /Delete "\$INSTDIR\\\$\{MAINBINARYNAME\}\.exe"/);
  assert.match(installSection, /RMDir \/r "\$INSTDIR\\\\\{\{this\}\}"/);
  assert.match(
    installSection,
    /\$MaintenanceAction == "full_reinstall"[\s\S]*RmDir \/r "\$APPDATA\\\$\{BUNDLEID\}"[\s\S]*RmDir \/r "\$LOCALAPPDATA\\\$\{BUNDLEID\}"/,
  );
  assert.match(language, /replace all managed app files in one installer run/);
});

test("maintenance is observable and deterministically testable", () => {
  assert.match(installer, /\/VVMAINTENANCE=/);
  for (const action of [
    "update",
    "reinstall_keep",
    "full_reinstall",
    "uninstall_keep",
    "full_uninstall",
  ]) {
    assert.match(installer, new RegExp(`\\$0 == "${action}"`));
  }
  assert.match(installer, /voxvulgi_installer_logs\\maintenance_latest\.log/);
  assert.match(
    installer,
    /Function AppendInstallerLog[\s\S]*\$\{If\} \$\{Errors\}[\s\S]*ClearErrors[\s\S]*FileOpen[\s\S]*FileSeek \$1 0 END[\s\S]*ClearErrors[\s\S]*SetErrors/,
  );
  assert.match(installer, /Function \.onInstFailed[\s\S]*installer_failed/);
  assert.match(installer, /Function \.onInstSuccess[\s\S]*installer_success/);
  assert.equal(config.bundle.windows.nsis.compression, "zlib");
  assert.match(productSpec, /one CRC-checked installer process/);
  assert.match(technicalDesign, /does not hand continuation to the prior uninstaller/);
});

test("current-user installer cannot inherit the legacy Program Files target", () => {
  const init = installer.slice(
    installer.indexOf("Function .onInit"),
    installer.indexOf("FunctionEnd", installer.indexOf("Function .onInit")),
  );
  assert.match(config.bundle.windows.nsis.installMode, /^currentUser$/);
  assert.match(init, /ReadRegStr \$LegacyMachineInstallDir HKLM "\$\{MANUPRODUCTKEY\}" ""/);
  assert.match(init, /ReadRegStr \$LegacyMachineVersion HKLM "\$\{UNINSTKEY\}" "DisplayVersion"/);
  assert.match(init, /legacy_machine_install_detected/);
  assert.match(init, /StrCpy \$INSTDIR "\$LOCALAPPDATA\\\$\{PRODUCTNAME\}"/);
  assert.match(init, /current_user_install_target_enforced/);
  assert.ok(
    init.indexOf('StrCpy $INSTDIR "$LOCALAPPDATA\\${PRODUCTNAME}"') <
      init.indexOf("Call RestorePreviousInstallLocation"),
  );
  assert.match(productSpec, /must not inherit, accept, or restore a Program Files target/);
  assert.match(technicalDesign, /HKLM state must never select the write target/);
});

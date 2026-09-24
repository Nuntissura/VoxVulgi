import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const desktopRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = join(desktopRoot, "..", "..");
const paths = readFileSync(join(repoRoot, "product", "engine", "src", "paths.rs"), "utf8");
const tools = readFileSync(join(repoRoot, "product", "engine", "src", "tools.rs"), "utf8");
const downloadEngines = readFileSync(
  join(repoRoot, "product", "engine", "src", "download_engines.rs"),
  "utf8",
);
const jobs = readFileSync(join(repoRoot, "product", "engine", "src", "jobs.rs"), "utf8");
const desktop = readFileSync(join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");
const diagnostics = readFileSync(join(desktopRoot, "src", "pages", "DiagnosticsPage.tsx"), "utf8");
const buildTarget = readFileSync(join(repoRoot, "governance", "scripts", "build_desktop_target.ps1"), "utf8");
const packageJson = readFileSync(join(desktopRoot, "package.json"), "utf8");
const buildTargetPaths = readFileSync(join(repoRoot, "governance", "scripts", "desktop_build_target_paths.ps1"), "utf8");

test("installed app separates roaming user data from LocalAppData managed runtime", () => {
  assert.match(desktop, /app\.path\(\)\.app_local_data_dir\(\)\?\.join\("runtime"\)/);
  assert.match(desktop, /AppPaths::installed\(/);
  assert.match(paths, /runtime_root\.join\("current\.json"\)/);
  assert.match(paths, /join\("generations"\)/);
  assert.match(paths, /selected managed runtime manifest hash mismatch/);
  assert.match(paths, /previous\.json/);
  assert.match(paths, /persistence::atomic_write_bytes\(&current/);
  assert.match(paths, /python_portable_dir[\s\S]{0,180}managed_offline\(\)[\s\S]{0,80}"runtime_main"/);
});

test("managed runtime fails closed instead of using PATH or in-app repair", () => {
  assert.match(paths, /if path\.exists\(\) \|\| self\.managed_offline\(\)/);
  assert.match(
    downloadEngines,
    /fn packaged_executable\(paths: &AppPaths\)[\s\S]{0,160}paths\.tools_dir\(\)\.join\("yt-dlp"\)\.join\("yt-dlp"\)/,
  );
  assert.doesNotMatch(downloadEngines, /PathBuf::from\("yt-dlp(?:\.exe)?"\)/);
  assert.match(tools, /installer-managed offline runtime; install a qualified runtime generation instead/);
  assert.match(tools, /if paths\.managed_offline\(\) \{\s*return None;\s*\}/);
  assert.match(
    jobs,
    /let selected_result = if protected_youtube \{[\s\S]{0,260}download_engines::resolve_selected_engine\(paths\)/,
  );
  assert.match(jobs, /unverified PATH\/Python fallbacks are disabled/);
  assert.match(desktop, /if paths\.managed_offline\(\) \{\s*return Ok\(\(\)\);\s*\}/);
});

test("headless verification refuses production app data and runtime provenance is visible", () => {
  assert.match(desktop, /is required for --agent-headless; refusing the production app-data root/);
  assert.match(desktop, /must not resolve to the production app-data root/);
  assert.match(desktop, /immutable installed input/);
  assert.doesNotMatch(desktop, /let runtime_root = if cli_agent_headless/);
  assert.match(desktop, /"runtime": state\.runtime/);
  assert.match(desktop, /AppPaths::invalid_managed/);
  assert.match(paths, /ManagedOfflineInvalid/);
  assert.match(diagnostics, /<h2>Data and runtime<\/h2>/);
  assert.match(diagnostics, /Selected generation/);
  assert.match(diagnostics, /Dependency fallback/);
});

test("invalid stale release entrypoint remains absent", () => {
  assert.equal(
    existsSync(
      join(
        repoRoot,
        "offline-installer-runtime",
        "scripts",
        "build_offline_release_fast.ps1",
      ),
    ),
    false,
  );
});

test("desktop and installer builds cannot mutate release identity", () => {
  assert.match(buildTarget, /\[string\]\$ExpectedVersion/);
  assert.match(buildTarget, /Actual desktop builds require -ExpectedVersion <already-assigned-version>/);
  assert.match(buildTarget, /Version retained: \$nextVersion/);
  assert.match(buildTarget, /Verifying release identity remained unchanged/);
  assert.match(buildTarget, /Desktop build mutated protected release identity input/);
  assert.match(buildTarget, /governance\\release\\BUILD_CHANGELOG\.md/);
  assert.doesNotMatch(buildTarget, /function Bump-PatchVersion/);
  assert.doesNotMatch(buildTarget, /function Set-JsonVersion/);
  assert.doesNotMatch(buildTarget, /function Set-CargoPackageVersion/);
  assert.doesNotMatch(buildTarget, /Append-BuildChangelogEntry/);
});

test("core-only desktop builds are independent of offline payload construction", () => {
  assert.match(buildTarget, /\[switch\]\$CoreOnly/);
  assert.match(buildTarget, /if \(\$CoreOnly\) \{[\s\S]{0,300}external managed runtime/);
  assert.match(buildTarget, /No dependency pack is built, bundled, refreshed, installed, or certified/);
  assert.match(buildTarget, /-CoreOnly builds the desktop app against an external managed runtime and cannot be combined with offline-payload or pack-warmup flags/);
  assert.match(packageJson, /"build:desktop:target:core-only"/);
  assert.match(buildTargetPaths, /CargoCacheDir = Join-Path \$buildRoot 'cargo_cache'/);
  assert.match(buildTarget, /\$env:CARGO_TARGET_DIR = \$cargoCacheDir/);
  assert.doesNotMatch(buildTarget, /\$env:CARGO_TARGET_DIR = \$currentDir/);
  assert.match(buildTarget, /Rebasing copied compiler cache build-script outputs to the stable cache path/);
  assert.match(buildTarget, /\.voxvulgi_cache_root\.txt/);
  assert.match(buildTarget, /Publishing app deliverables from the reusable compiler cache/);
  assert.ok(
    buildTarget.indexOf('Step "Publishing app deliverables from the reusable compiler cache"') <
      buildTarget.indexOf('Step "Verifying release identity remained unchanged"'),
  );
  assert.ok(
    buildTarget.indexOf('Step "Publishing app deliverables from the reusable compiler cache"') <
      buildTarget.indexOf('Step "Binding published Current generation to release ownership marker"'),
  );
});

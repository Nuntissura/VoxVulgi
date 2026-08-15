import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const desktopRoot = fileURLToPath(new URL("..", import.meta.url));
const app = readFileSync(join(desktopRoot, "src", "App.tsx"), "utf8");
const editor = readFileSync(join(desktopRoot, "src", "pages", "SubtitleEditorPage.tsx"), "utf8");
const jobs = readFileSync(join(desktopRoot, "..", "engine", "src", "jobs.rs"), "utf8");

test("WP-0214 keeps the default Localization home setup-first and output-root linked", () => {
  assert.match(app, /safeLocalStorageGet\(LOCALIZATION_HOME_LEGACY_KEY\) !== "1"/);
  assert.match(app, /<section className="loc-setup-workbench" aria-label="Localization setup">/);
  assert.match(app, /<span>Source language<\/span>/);
  assert.match(app, /<span>Subtitles<\/span>[\s\S]{0,500}<span>Dub<\/span>/);
  assert.match(app, /currentExportDir \|\| localizationRootDir/);
  assert.match(app, /Change in Options/);
  assert.match(app, /Start localization[\s\S]{0,500}>\s*Stop\s*</);
  assert.match(app, /aria-label=\{`Progress \$\{currentProgressPct\}%`\}/);
  assert.match(app, /aria-label="Successful localization jobs"/);
  assert.match(app, /Latest usable outputs/);
});

test("WP-0214 persists one source-copy preference and the editor consumes it", () => {
  const storageKey = "voxvulgi.v1.editor.export_include_source_copy";
  assert.match(app, new RegExp(storageKey.replace(/\./g, "\\.")));
  assert.match(app, /safeLocalStorageSet\(LOCALIZATION_INCLUDE_SOURCE_COPY_KEY, includeSourceCopy \? "1" : "0"\)/);
  assert.match(app, /Include source copy in output folder/);
  assert.match(editor, new RegExp(storageKey.replace(/\./g, "\\.")));
  assert.match(editor, /export_source_copy: exportIncludeSourceCopy/);
  assert.match(editor, /if \(exportIncludeSourceCopy\)[\s\S]{0,500}item_export_source_media/);
});

test("WP-0214 preserves language-marked exports and the subtitle-only stop gate", () => {
  assert.match(app, /\.source\.mkv/);
  assert.match(app, /\.sub-en\.srt/);
  assert.match(app, /\.dub-en\.mkv/);
  assert.match(editor, /`\$\{sourceBaseStem\}\.sub-en\.srt`/);
  assert.match(editor, /`\$\{sourceBaseStem\}\.sub-en\.vtt`/);
  assert.match(editor, /`\$\{sourceBaseStem\}\.dub-en\.\$\{dubExt\}`/);
  assert.match(jobs, /output_mode == "subtitles"[\s\S]{0,300}track\.kind == "translated"/);
  assert.match(jobs, /English subtitles are ready; no dubbing stages were requested\./);
  assert.match(jobs, /fn enqueue_localization_run_v1_stops_at_english_subtitles_when_requested/);
});

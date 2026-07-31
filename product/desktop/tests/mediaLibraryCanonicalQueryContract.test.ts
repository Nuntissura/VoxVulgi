import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(here, "..");
const repoRoot = path.resolve(desktopRoot, "..", "..");
const page = fs.readFileSync(path.join(desktopRoot, "src", "pages", "LibraryPage.tsx"), "utf8");
const tauri = fs.readFileSync(path.join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");
const library = fs.readFileSync(path.join(repoRoot, "product", "engine", "src", "library.rs"), "utf8");

test("WP-0286 sends every Media Library predicate to one canonical backend query", () => {
  assert.match(page, /invoke<LibraryItemsPage>\("library_query"/);
  for (const argument of [
    "fileStatus: mediaLibraryFileStatus",
    "query: mediaLibrarySearch || null",
    "mediaType: mediaLibraryTypeFilter",
    "source: mediaLibrarySourceFilter",
    "singleVideoOnly: mediaLibrarySingleVideoOnly",
    "sortBy: mediaLibrarySortBy",
    "direction: mediaLibrarySortDirection",
  ]) {
    assert.ok(page.includes(argument), `missing canonical query argument: ${argument}`);
  }
  assert.match(page, /const filteredMediaItems = items;/);
  assert.doesNotMatch(page, /const filteredMediaItems = useMemo\(\(\) => \{[\s\S]*items\.filter/);
});

test("WP-0286 backend filters before pagination and returns matching truth", () => {
  assert.match(library, /pub struct LibraryPage \{[\s\S]*pub filtered_total: usize/);
  assert.match(library, /pub fn query_items_page\(/);
  assert.match(library, /FROM canonical[\s\S]*\{where_sql\}[\s\S]*ORDER BY[\s\S]*LIMIT \?6 OFFSET \?7/);
  assert.match(library, /COALESCE\([\s\S]*lineage\.service,[\s\S]*identity_service\.service/);
  assert.match(library, /GROUP BY library_item_id/);
  assert.match(tauri, /async fn library_query\(/);
  assert.match(tauri, /\n\s*library_query,/);
  assert.match(page, /Loaded \{items\.length\} of \{mediaLibraryFilteredTotal\} matching item/);
});

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("browser-cookie source defaults to Firefox while Chrome and Edge remain selectable", () => {
  const librarySource = readRepoFile("src", "pages", "LibraryPage.tsx");
  const optionsStart = librarySource.indexOf("const browserCookieSourceOptions");
  const optionsEnd = librarySource.indexOf("function ThumbnailPreview", optionsStart);
  const optionsBlock = librarySource.slice(optionsStart, optionsEnd);

  assert.match(
    librarySource,
    /const\s+DEFAULT_BROWSER_COOKIE_SOURCE\s*=\s*"firefox";/,
    "Library browser-cookie source default must stay Firefox",
  );
  assert.match(optionsBlock, /\{\s*value:\s*"firefox",\s*label:\s*"Firefox \(default\)"\s*\}/);
  assert.match(optionsBlock, /\{\s*value:\s*"chrome",\s*label:\s*"Chrome"\s*\}/);
  assert.match(optionsBlock, /\{\s*value:\s*"edge",\s*label:\s*"Edge"\s*\}/);
  assert.match(
    librarySource,
    /instagram_batch_browser_cookie_source"\)\s*\|\|\s*DEFAULT_BROWSER_COOKIE_SOURCE/s,
    "blank or missing Instagram batch browser source must initialize to Firefox",
  );
  assert.match(
    librarySource,
    /instagram_subscription_browser_cookie_source"[\s\S]{0,160}\|\|\s*DEFAULT_BROWSER_COOKIE_SOURCE/,
    "blank or missing subscription browser source must initialize to Firefox",
  );
  assert.match(
    librarySource,
    /browserCookieSource:\s*effectiveBrowserCookieSource/,
    "Instagram batch enqueue should send the normalized browser source",
  );
  assert.match(
    librarySource,
    /browser_cookie_source:\s*instagramSubscriptionUseBrowserCookies[\s\S]{0,180}DEFAULT_BROWSER_COOKIE_SOURCE/,
    "Instagram subscription save should persist Firefox when the source is blank",
  );
});

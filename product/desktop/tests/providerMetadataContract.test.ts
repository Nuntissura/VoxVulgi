import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const desktopRoot = path.resolve(import.meta.dirname, "..");
const repoRoot = path.resolve(desktopRoot, "..", "..");
const jobsSource = fs.readFileSync(path.join(repoRoot, "product", "engine", "src", "jobs.rs"), "utf8");
const jobsPageSource = fs.readFileSync(path.join(desktopRoot, "src", "pages", "JobsPage.tsx"), "utf8");
const libraryPageSource = fs.readFileSync(path.join(desktopRoot, "src", "pages", "LibraryPage.tsx"), "utf8");
const sharedSource = fs.readFileSync(path.join(desktopRoot, "src", "lib", "providerMetadata.ts"), "utf8");

test("provider enumeration is strict UTF-8 JSON rather than delimiter or lossy title parsing", () => {
  const start = jobsSource.indexOf("fn expand_yt_dlp_entries_with_sleep(");
  const end = jobsSource.indexOf("fn stamp_job_target_titles_by_url(", start);
  assert.ok(start >= 0 && end > start, "enumeration boundary must remain locatable");
  const boundary = jobsSource.slice(start, end);
  assert.match(boundary, /"--encoding"\.to_string\(\)/);
  assert.match(boundary, /"utf-8"\.to_string\(\)/);
  assert.match(boundary, /"--print-json"\.to_string\(\)/);
  assert.match(boundary, /serde_json::from_slice/);
  assert.doesNotMatch(boundary, /from_utf8_lossy/);
  assert.doesNotMatch(boundary, /split_once\('\t'\)/);
});

test("Jobs exposes canonical title provenance instead of flattening every title to one claim", () => {
  assert.match(sharedSource, /target_title_provenance\?: TitleProvenance \| null/);
  assert.match(sharedSource, /case "canonical_remote":[\s\S]*Canonical provider title/);
  assert.match(sharedSource, /case "stable_provider_id":[\s\S]*Provider ID fallback/);
  assert.match(jobsPageSource, /titleProvenanceLabel\(job\.target_title_provenance\)/);
  assert.match(jobsPageSource, /job\.target_title_problem/);
});

test("Media Library consumes the same canonical provenance vocabulary", () => {
  assert.match(sharedSource, /CanonicalLibraryTitleProjection/);
  assert.match(libraryPageSource, /type LibraryItem = CanonicalLibraryTitleProjection/);
  assert.match(libraryPageSource, /titleProvenanceLabel\(item\.title_provenance\)/);
  assert.match(libraryPageSource, /item\.title_problem/);
});

test("single downloads ingest prefixed JSON from raw bytes before lossy path handling", () => {
  const start = jobsSource.indexOf("fn ingest_ytdlp_download_metadata(");
  const end = jobsSource.indexOf("fn stamp_job_target_titles_by_url(", start);
  assert.ok(start >= 0 && end > start, "single-download metadata boundary must remain locatable");
  const boundary = jobsSource.slice(start, end);
  assert.match(boundary, /serde_json::from_slice/);
  assert.match(boundary, /VV_MEDIA_POST:/);
  assert.doesNotMatch(boundary, /from_utf8_lossy/);
});

import test from "node:test";
import assert from "node:assert/strict";

import {
  buildJobContextSummary,
  isCanonicalSingleVideoItem,
  isCanonicalYoutubeSingleVideoItem,
  jobTrackLabel,
  summarizeJobGroupTargets,
} from "../src/lib/archiverRuntime.ts";

function libraryItem(overrides = {}) {
  return {
    id: "item-1",
    created_at_ms: 2_000,
    source_type: "url_direct",
    source_uri: "https://www.youtube.com/watch?v=abc123",
    title: "SNSD comeback fancam",
    media_path: "D:\\Archive\\video\\youtube\\SNSD comeback fancam.mp4",
    duration_ms: 60_000,
    width: 1920,
    height: 1080,
    container: "mp4",
    video_codec: "h264",
    audio_codec: "aac",
    thumbnail_path: null,
    ...overrides,
  };
}

test("download direct job context reads persisted single url", () => {
  const context = buildJobContextSummary(
    {
      id: "job-1",
      item_id: null,
      job_type: "download_direct_url",
      params_json: JSON.stringify({
        url: "https://www.youtube.com/watch?v=abc123",
        output_dir: "D:\\Archive\\video",
      }),
    },
    {},
  );

  assert.equal(context.label, "https://www.youtube.com/watch?v=abc123");
  assert.equal(context.detail, "Target root: D:\\Archive\\video");
});

test("download direct job context prefers cached target title and keeps source url visible", () => {
  const context = buildJobContextSummary(
    {
      id: "job-1",
      item_id: null,
      job_type: "download_direct_url",
      target_title: "Cached YouTube title",
      params_json: JSON.stringify({
        url: "https://www.youtube.com/watch?v=abc123",
        output_dir: "D:\\Archive\\video",
      }),
    },
    {},
  );

  assert.equal(context.label, "Cached YouTube title");
  assert.equal(
    context.detail,
    "https://www.youtube.com/watch?v=abc123 | Target root: D:\\Archive\\video",
  );
});

test("job track labels use only the durable scheduler vocabulary", () => {
  assert.equal(jobTrackLabel("youtube_single"), "YouTube single");
  assert.equal(jobTrackLabel("youtube_recurring"), "YouTube background");
  assert.equal(jobTrackLabel("instagram_single"), "Instagram single");
  assert.equal(jobTrackLabel("instagram_recurring"), "Instagram background");
  assert.equal(jobTrackLabel("tiktok_single"), "TikTok single");
  assert.equal(jobTrackLabel("tiktok_recurring"), "TikTok background");
  assert.equal(jobTrackLabel("other_video"), "Other video");
  assert.equal(jobTrackLabel("image_archive"), "Image Archive");
  assert.equal(jobTrackLabel("localization"), "Localization");
  assert.equal(jobTrackLabel(null), "Unclassified");
  assert.equal(jobTrackLabel("youtube"), "Unclassified");
});

test("direct job context keeps origin separate from persisted track", () => {
  const context = buildJobContextSummary(
    {
      id: "job-instagram",
      item_id: null,
      job_type: "download_direct_url",
      track: "instagram_single",
      params_json: JSON.stringify({ url: "https://www.instagram.com/reel/example" }),
    },
    {},
  );

  assert.equal(context.track_label, "Instagram single");
  assert.equal(context.origin, "Direct download");
  assert.notEqual(context.origin, "Single video");
});


test("collapsed job group summary keeps distinct video sources visible", () => {
  const jobs = [
    { id: "job-1", item_id: null, job_type: "download_direct_url", params_json: "{}" },
    { id: "job-2", item_id: null, job_type: "download_direct_url", params_json: "{}" },
    { id: "job-3", item_id: null, job_type: "download_direct_url", params_json: "{}" },
    { id: "job-4", item_id: null, job_type: "download_direct_url", params_json: "{}" },
  ];
  const summary = summarizeJobGroupTargets(jobs, {
    "job-1": { label: "https://youtu.be/one", detail: null, target_path: null, target_action_label: null, track_label: "YouTube single" },
    "job-2": { label: "https://youtu.be/two", detail: null, target_path: null, target_action_label: null, track_label: "YouTube single" },
    "job-3": { label: "https://youtu.be/three", detail: null, target_path: null, target_action_label: null, track_label: "YouTube single" },
    "job-4": { label: "https://youtu.be/four", detail: null, target_path: null, target_action_label: null, track_label: "YouTube single" },
  });

  assert.equal(
    summary,
    "https://youtu.be/one | https://youtu.be/two | https://youtu.be/three | +1 more",
  );
});

test("single-video library classifier uses canonical lineage and never mapped paths", () => {
  const single = libraryItem({
    lineage_service: "youtube",
    lineage_origin_kind: "single",
    lineage_work_track: "youtube_single",
  });
  const subscription = libraryItem({
    id: "item-2",
    media_path: "\\\\MIR\\Archive\\MEOVV\\clip.mp4",
    lineage_service: "youtube",
    lineage_origin_kind: "subscription",
    lineage_work_track: "youtube_recurring",
  });
  const unknownMappedYoutube = libraryItem({
    id: "item-3",
    media_path: "\\\\MIR\\Archive\\Unknown\\clip.mp4",
  });

  assert.equal(isCanonicalSingleVideoItem(single), true);
  assert.equal(isCanonicalYoutubeSingleVideoItem(single), true);
  assert.equal(isCanonicalSingleVideoItem(subscription), false);
  assert.equal(isCanonicalYoutubeSingleVideoItem(subscription), false);
  assert.equal(isCanonicalSingleVideoItem(unknownMappedYoutube), false);
  assert.equal(isCanonicalYoutubeSingleVideoItem(unknownMappedYoutube), false);
});

test("youtube single classification keeps origin and scheduling track separate", () => {
  const manualPlaylistMember = libraryItem({
    lineage_service: "youtube",
    lineage_origin_kind: "playlist",
    lineage_work_track: "youtube_single",
  });
  const subscriptionChild = libraryItem({
    lineage_service: "youtube",
    lineage_origin_kind: "subscription",
    lineage_work_track: "youtube_recurring",
  });

  assert.equal(isCanonicalYoutubeSingleVideoItem(manualPlaylistMember), false);
  assert.equal(isCanonicalYoutubeSingleVideoItem(subscriptionChild), false);
});

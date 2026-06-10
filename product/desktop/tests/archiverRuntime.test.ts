import test from "node:test";
import assert from "node:assert/strict";

import {
  buildJobContextSummary,
  filterYoutubeSingleVideoItems,
  isSingleVideoLibraryItem,
  isYoutubeSingleVideoItem,
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


test("collapsed job group summary keeps distinct video sources visible", () => {
  const jobs = [
    { id: "job-1", item_id: null, job_type: "download_direct_url", params_json: "{}" },
    { id: "job-2", item_id: null, job_type: "download_direct_url", params_json: "{}" },
    { id: "job-3", item_id: null, job_type: "download_direct_url", params_json: "{}" },
    { id: "job-4", item_id: null, job_type: "download_direct_url", params_json: "{}" },
  ];
  const summary = summarizeJobGroupTargets(jobs, {
    "job-1": { label: "https://youtu.be/one", detail: null, target_path: null, target_action_label: null },
    "job-2": { label: "https://youtu.be/two", detail: null, target_path: null, target_action_label: null },
    "job-3": { label: "https://youtu.be/three", detail: null, target_path: null, target_action_label: null },
    "job-4": { label: "https://youtu.be/four", detail: null, target_path: null, target_action_label: null },
  });

  assert.equal(
    summary,
    "https://youtu.be/one | https://youtu.be/two | https://youtu.be/three | +1 more",
  );
});

test("single-video library classifier separates loose files from subscription containers", () => {
  const single = libraryItem();
  const subscription = libraryItem({
    id: "item-2",
    media_path: "D:\\Archive\\video\\subscriptions\\Girls Generation\\clip.mp4",
  });
  const localAudio = libraryItem({
    id: "item-3",
    source_type: "local_file",
    source_uri: "D:\\Music\\track.wav",
    media_path: "D:\\Music\\track.wav",
    video_codec: null,
    audio_codec: "pcm",
  });

  assert.equal(isSingleVideoLibraryItem(single, "D:\\Archive\\video"), true);
  assert.equal(isSingleVideoLibraryItem(subscription, "D:\\Archive\\video"), false);
  assert.equal(isSingleVideoLibraryItem(localAudio, "D:\\Archive\\video"), false);
});

test("youtube single-video list filters newest first and fuzzy matches title typos", () => {
  const rows = [
    libraryItem({ id: "old", created_at_ms: 1_000, title: "SNSD Tokyo fancam" }),
    libraryItem({ id: "new", created_at_ms: 3_000, title: "Haerin airport vlog" }),
    libraryItem({
      id: "sub",
      created_at_ms: 4_000,
      title: "Subscription video",
      media_path: "D:\\Archive\\video\\subscriptions\\Channel\\clip.mp4",
    }),
  ];

  assert.deepEqual(
    filterYoutubeSingleVideoItems(rows, "hrn vlog", "D:\\Archive\\video", "desc").map((item) => item.id),
    ["new"],
  );
  assert.deepEqual(
    filterYoutubeSingleVideoItems(rows, "", "D:\\Archive\\video", "desc").map((item) => item.id),
    ["new", "old"],
  );
  assert.equal(isYoutubeSingleVideoItem(rows[2], "D:\\Archive\\video"), false);
});

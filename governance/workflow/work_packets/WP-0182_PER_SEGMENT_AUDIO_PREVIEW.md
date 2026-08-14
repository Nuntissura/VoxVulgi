# Work Packet: WP-0182 - Per-Segment Audio Preview

## Metadata
- ID: WP-0182
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-08
- Target milestone: UX Polish

## Intent

- What: Add a play button on each subtitle segment that plays the dubbed audio for just that segment.
- Why: Currently operators must play the full video/mix and seek to the right timestamp to hear a specific segment. Per-segment preview enables rapid QC — click, listen, fix, repeat.

## Scope

In scope:
- Play button on each segment row in the subtitle editor.
- Plays the TTS/dub audio for that segment's time range from the mix or per-segment WAV artifacts.
- Visual playback indicator on the active segment.
- Stop on click or when playback finishes.
- Falls back gracefully if no dubbed audio exists for that segment.

Out of scope:
- Waveform visualization (separate WP).
- Editing audio from the segment view.

## Acceptance criteria
- Each segment row has a play button.
- Clicking play starts audio for that segment only.
- Playback stops at segment end or on re-click.
- Works with existing TTS/dub artifacts.
- `npm run build` passes.

## Research basis (2026-08-14)

- The HTML media contract defines `currentTime` as the playback seek position and exposes `timeupdate`, `ended`, `error`, and `loadedmetadata` lifecycle events. `play()` returns a promise that resolves only once playback has started and rejects when playback cannot begin; UI must not claim playback before that promise resolves.
- Selected approach: use the existing item mix WAV as the packet-permitted ranged source; seek it to the subtitle start; mark the row active only after `play()` resolves; stop against `timeupdate` at the subtitle end, with a bounded current-time poll as a fallback; clear state on media error, pause, natural end, re-click, item change, and unmount. Rejected: a wall-clock stop timer, because loading or buffering shortens the audible range; swallowed promise rejection, because it leaves false “playing” state; and new per-segment audio generation, because the existing mix satisfies the packet and editing/generation is out of scope.
- Risks and mitigations: missing mix disables playback with an explanatory label; invalid or empty timing is rejected before media creation; media load/play failure is visible; handlers are detached before aborting playback so a stale element cannot change the next segment's state; active-row styling plus text/icon state makes playback observable without relying on color alone.
- Validation plan: production TypeScript/Vite build, contract tests around the extracted range controller, built bundle inspection, and packaged headless UI inspection with an item that has an existing mix WAV.
- Primary sources checked: `https://html.spec.whatwg.org/multipage/media.html`, `https://developer.mozilla.org/en-US/docs/Web/API/HTMLMediaElement/play`, `https://developer.mozilla.org/en-US/docs/Web/API/HTMLMediaElement/currentTime`, and `https://developer.mozilla.org/en-US/docs/Web/API/HTMLMediaElement`.

## Implementation status (2026-08-14)

- Product code implemented: each subtitle row has a Play/Stop control against the existing mix WAV; invalid/missing timing and media failures are explicit; pending playback is cancelable; active playback is visible in text and row styling; stopping follows observed media time and all lifecycle exits detach the old element.
- Verification passed: focused range/end-boundary contract tests (`3 passed`), `npm run build`, `git diff --check`, and built-bundle inspection for the fallback/loading controls. The full frontend contract suite passed `189/190`; its sole failure is the unrelated existing `youtubeAuthProjection.test.ts` expectation against `OptionsPage.tsx`, which WP-0182 does not touch.
- Remaining before `DONE`: governed desktop build and packaged headless UI/app-boundary proof with an item whose mix WAV exists, as required by `PROOF_STANDARD.md`.

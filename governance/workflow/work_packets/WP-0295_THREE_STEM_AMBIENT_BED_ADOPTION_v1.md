# Work Packet: WP-0295 — Three-Stem Ambient Bed Adoption (v1)

## Metadata
- ID: WP-0295
- Owner: assistant (implementation) + operator (listening acceptance + test clip supply)
- Status: BLOCKED
- Created: 2026-08-05 (split out of WP-0294 after review)
- Depends on: **WP-0288** (separation benchmark must freeze the winner), WP-0294 (mixer controls + bed reporting), WP-0289 MT-03 (separation backend swap)
- Target milestone: Localization core recovery

## Intent
- What: Replace the music-oriented 2-stem separation with a **dialog / music / effects** model so the
  dub bed actually contains the room — audience, stingers, SFX, ambience.
- Why: Split from WP-0294 because this is a different risk class: it depends on a model that has not
  been selected yet, changes the installer payload, and carries a licence attribution obligation.
  Bundling it with buildable mixer work made WP-0294 unacceptable as drafted (its AC depended on an
  unchosen model and a test clip that does not exist).

## Grounding evidence (measured 2026-08-05, Haerin clip)
| Signal | Mean level | SECS vs speaker reference |
|---|---|---|
| Original audio | −28.6 dB | 0.9529 |
| Spleeter `vocals.wav` | −28.5 dB | 0.9351 |
| Spleeter `accompaniment.wav` | **−52.9 dB** | 0.1725 |

Spleeter put essentially all energy into vocals; the bed came out ~24 dB below source. **Spleeter
`2stems` is a music model** (vocals vs instruments) with no dialog/effects concept, so on speech-led
content it sweeps speech *and* ambience into "vocals" and leaves an empty bed. No gain setting can
recover ambience the separator already discarded.

## Candidates (license-verified in WP-0287, selection frozen by WP-0288)
- **Bandit-v2 `multi`** — code Apache-2.0, weights CC-BY-SA-4.0 (Zenodo), the only verified
  redistributable model actually trained for dialog/music/effects on cinematic data, with Japanese in
  the DnR v3 training set (Korean absent).
- **TIGER-DnR** — Apache-2.0 weights, 1.4 M params, CPU-viable, DnR speech SI-SDR 15.5 dB; repo has an
  MIT-file-vs-Apache-badge conflict that must be resolved before bundling.

## Scope
- In scope: adopt the WP-0288-selected 3-stem model as the dub bed source; add it to the separation
  backend selector (which today offers only `demucs` / `spleeter`, `SubtitleEditorPage.tsx`
  L3801-3807); bundle its weights in the offline payload with the licence file snapshotted; add the
  in-app CC-BY/CC-BY-SA attribution surface the licence requires.
- Out of scope: mixer controls and bed reporting (WP-0294); choosing the winner (WP-0288).

## Acceptance criteria
1. On a source with real ambience, the bed measurably retains it — bed level materially above the
   −52.9 dB Spleeter baseline on the same clip, measured with the same method.
2. **Bed dialog-leak gate**: bed SECS against the speaker reference stays low. A high-similarity bed
   means the original dialogue leaked into the effects stem and would put the source language back
   under the dub — this is a hard fail.
3. Offline: the model resolves entirely from bundled bytes with the network blocked (WP-0288
   packaging-gate method: dead proxy, not just `HF_HUB_OFFLINE=1`).
4. Attribution for CC-BY-SA weights is visible in-app.
5. Operator confirms by listening that the result sounds natural — voice forward, room intact.

## Blocked on / required inputs
- **WP-0288 has not selected a separation winner.** This packet cannot start until it does.
- **A test clip with strong ambience does not exist.** The current corpus is Haerin (7.2 s, little
  background) and Miyeon (2m55s). A game-show/crowd clip is an **operator input** and is the same gap
  already recorded in WP-0288's corpus requirements.

## Risks
- Dialog bleed into the bed (see AC2) — the failure mode that would silently reintroduce Korean.
- CPU cost: RoFormer/Bandit-class models are heavier than Spleeter, and the RTX 3090 is currently
  unused (`torch 2.3.1+cpu`); WP-0289 MT-08 matters here.
- Payload growth is accepted (PRODUCT_SPEC 8.1.8, size explicitly not a constraint) but the licence
  must be snapshotted at bundle time.

## Status updates
- 2026-08-05: Created by splitting WP-0294 on review finding that model adoption and mixer controls
  are different risk classes and must not share an acceptance gate.

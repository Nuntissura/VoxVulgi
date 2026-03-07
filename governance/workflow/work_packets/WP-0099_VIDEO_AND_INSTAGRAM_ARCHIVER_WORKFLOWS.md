# Work Packet: WP-0099 - Video and Instagram Archiver workflows

## Metadata
- ID: WP-0099
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-07
- Target milestone: Workflow and archive UX hardening

## Intent

- What: Harden the operator workflows inside Video Archiver and Instagram Archiver.
- Why: The current archive surfaces are missing a few practical controls that matter for day-to-day archive management.

## Scope

In scope:

- Add an `Open folder` action for saved YouTube subscriptions in Video Archiver.
- Add saved Instagram subscriptions with an interval-based heartbeat or cron-like refresh model.
- Add a recent-media thumbnail viewer in Instagram Archiver that shows the last 10 items without destructive crop framing.

Out of scope:

- New non-Instagram social providers in this WP.
- Full legacy-library reconciliation.

## Acceptance criteria

- Saved YouTube subscriptions expose a direct open-folder action when a mapped folder exists.
- Instagram Archiver can save and refresh recurring archive targets on an interval-based schedule.
- The recent thumbnail viewer shows full thumbnails for the last 10 pictures/stories/reels without the current crop loss.

## Test / verification plan

- Desktop build.
- Manual UI smoke for YouTube subscriptions and Instagram recurring archive flows.

## Status updates

- 2026-03-07: Created from operator feedback on archive workflow gaps in the current YouTube and Instagram surfaces.

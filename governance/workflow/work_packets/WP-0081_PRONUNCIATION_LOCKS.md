# Work Packet: WP-0081 - Pronunciation locks

## Metadata
- ID: WP-0081
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-06
- Target milestone: Phase 2 language quality controls

## Intent

- What: Add pronunciation locks so names, places, loanwords, and glossary terms are spoken consistently across episodes.
- Why: Educational localization quality drops quickly when the same term is pronounced differently from clip to clip.

## Scope

In scope:

- Add reusable pronunciation rules tied to templates, cast packs, or global glossary scope.
- Support phonetic override text and backend-specific pronunciation hints where available.
- Apply pronunciation locks in both preview and final dubbing paths.

Out of scope:

- Automatic phoneme generation for every language/backend combination.
- Cloud lexicon services.

## Acceptance criteria

- Operators can define and reuse pronunciation rules.
- Pronunciation locks can target global, series, or item scope.
- Dubbing jobs apply pronunciation overrides deterministically.

## Test / verification plan

- Rule application unit tests.
- Engine regression tests against glossary ordering.
- Manual smoke with proper nouns across multiple episodes.

## Status updates

- 2026-03-06: Created to extend text glossary control into spoken-output consistency.

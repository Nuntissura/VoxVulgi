---
file_id: WP-0321-S8-PRESERVATION-MAP
file_kind: preservation_map
updated_at: 2026-09-23
---

# WP-0321 S8 — Governance Document Simplification Preservation Map

<topic id="map" summary="Rule-ID canonical-location map before/after S8 dedup pass" wp="WP-0321">

Scope: `PROJECT_CODEX.md` vs `AGENTS.md`/`CLAUDE.md`; `governance/spec/TECHNICAL_DESIGN.md` vs `governance/spec/PRODUCT_SPEC.md`; `build_rules.md`; `governance/workflow/TASK_BOARD.md`. `product/**` was not touched.

## Rule-ID table (all IDs found by `grep -o "\[VV-[A-Z0-9-]*\]"` and `\[OPERATOR-AUTHORITY-[0-9-]*\]` across the six governed root/spec files, before edits)

| Rule ID | Old locations (definition) | Canonical location after | Pointer locations after |
|---|---|---|---|
| VV-CODEX-VERSION-001..003 | AGENTS.md, CLAUDE.md (dup def; prose restated w/o ID in PROJECT_CODEX.md §7) | AGENTS.md + CLAUDE.md (mirror pair, unchanged) | PROJECT_CODEX.md §7 (pointer sentence, prose restatement removed) |
| VV-CODEX-INSTALL-001..004 | AGENTS.md, CLAUDE.md (dup def; prose restated w/o ID in PROJECT_CODEX.md §8) | AGENTS.md + CLAUDE.md (mirror pair, unchanged) | PROJECT_CODEX.md §8 (pointer sentence, prose restatement removed) |
| VV-CODEX-MANUAL-001, 002 | PROJECT_CODEX.md | PROJECT_CODEX.md (unchanged) | — |
| VV-SOT-001..008 | AGENTS.md, CLAUDE.md | AGENTS.md + CLAUDE.md (mirror pair, unchanged) | — |
| VV-STARTUP-001..004 | AGENTS.md, CLAUDE.md | AGENTS.md + CLAUDE.md (mirror pair, unchanged) | — |
| VV-DBRUNTIME-001..005 | AGENTS.md, CLAUDE.md | AGENTS.md + CLAUDE.md (mirror pair, unchanged) | — |
| VV-AGENT-MANUAL-001..004 | AGENTS.md, CLAUDE.md | AGENTS.md + CLAUDE.md (mirror pair, unchanged) | — |
| OPERATOR-AUTHORITY-001..006 | AGENTS.md, CLAUDE.md | AGENTS.md + CLAUDE.md (mirror pair, unchanged) | — |
| VV-BUILD-AGENT-001..006 | build_rules.md | build_rules.md (unchanged) | — |
| VV-BUILD-INSTALL-000, 010 | build_rules.md (pointer only; canonical home is `offline-installer-runtime/GUIDE.md`, pre-existing) | offline-installer-runtime/GUIDE.md (unchanged, out of S8 scope) | build_rules.md (pre-existing pointer, unchanged) |
| VV-BUILD-VERSION-001..004 | build_rules.md | build_rules.md (unchanged) | — |
| VV-VERSION-001..003 | PRODUCT_SPEC.md §8.1.7 (def) **and** TECHNICAL_DESIGN.md §2.1.1 (dup def, extra design detail) | PRODUCT_SPEC.md §8.1.7 | TECHNICAL_DESIGN.md §2.1.1 (pointer + retained unique design-detail sentence on `build_desktop_target.ps1`) |
| VV-0319-POLICY-001..009 | PRODUCT_SPEC.md (def) **and** TECHNICAL_DESIGN.md (byte-identical dup def) | PRODUCT_SPEC.md ("Archive quality and subscription recovery (WP-0319)") | TECHNICAL_DESIGN.md (pointer line) |
| VV-0320-POLICY-001..006 | PRODUCT_SPEC.md | PRODUCT_SPEC.md (unchanged) | — |
| VV-0320-DESIGN-001..004 | TECHNICAL_DESIGN.md | TECHNICAL_DESIGN.md (unchanged) | — |
| VV-0321-POLICY-001 | PRODUCT_SPEC.md | PRODUCT_SPEC.md (unchanged) | — |
| VV-0321-DESIGN-001 | TECHNICAL_DESIGN.md | TECHNICAL_DESIGN.md (unchanged) | — |
| VV-INSTALL-SCOPE-001, 002 | PRODUCT_SPEC.md | PRODUCT_SPEC.md (unchanged) | — |

Note: AGENTS.md/CLAUDE.md are an intentional, operator-required mirror pair (per repo CLAUDE.md and global GLOBAL-META-010/014); both retain every ID definition identically and are not treated as a duplication violation.

## Non-ID prose removed (verbatim/near-verbatim restatements), with canonical sentence retained

| # | Removed from (file:old line) | Canonical sentence it duplicates | Replacement |
|---|---|---|---|
| 1 | PROJECT_CODEX.md:71 (partial — the version/changelog-retention clause) | VV-CODEX-VERSION-001 (AGENTS.md/CLAUDE.md) | Pointer clause; unique part (caller must state expected version, write build log) retained in the same sentence |
| 2 | PROJECT_CODEX.md:72 | VV-CODEX-VERSION-002 (AGENTS.md/CLAUDE.md) | Folded into the same pointer sentence |
| 3 | PROJECT_CODEX.md:73 | VV-CODEX-VERSION-003 (AGENTS.md/CLAUDE.md) | Folded into the same pointer sentence |
| 4 | PROJECT_CODEX.md:74 (partial) | AGENTS.md "Desktop Build Output Policy" ("must not use spaces; prefer snake_case") | Sentence kept, pointer appended |
| 5 | PROJECT_CODEX.md:82 | VV-CODEX-INSTALL-004 (AGENTS.md/CLAUDE.md) | Pointer to VV-CODEX-INSTALL-001..004 |
| 6 | PROJECT_CODEX.md:89 ("Keep the keep-vs-full distinction explicit...") | AGENTS.md "Installer Maintenance Mode Policy" (same distinction) | Folded into pointer note on the labels list |
| 7 | PROJECT_CODEX.md:90 ("Managed desktop installer builds retain the already-assigned semantic version...") | AGENTS.md/CLAUDE.md "Installer Maintenance Mode Policy" (verbatim same sentence) | Removed; covered by the same pointer |
| 8 | PROJECT_CODEX.md:99-103 (5 diagnostics quick-index bullets: vvfreeze.cmd, report path, trace path, in-app button, worker sanity check) | AGENTS.md/CLAUDE.md "Headless Agent Bridge" + "Freeze Report (WP-0221)" (same facts, fuller detail) | Single pointer sentence; no unique fact was only in PROJECT_CODEX (all also stated in AGENTS.md/CLAUDE.md) |
| 9 | TECHNICAL_DESIGN.md:137-139 (VV-VERSION-001..003 full text) | PRODUCT_SPEC.md §8.1.7 VV-VERSION-001..003 | Pointer + retained unique design-only sentence (`build_desktop_target.ps1` byte-for-byte / caller-stated-version behavior, which PRODUCT_SPEC does not state) |
| 10 | TECHNICAL_DESIGN.md:856-868 (VV-0319-POLICY-001..009 full text, byte-identical to PRODUCT_SPEC) | PRODUCT_SPEC.md "Archive quality and subscription recovery (WP-0319)" | Pointer line; zero unique content lost (blocks were byte-identical, confirmed via `diff`) |

## Task-board format check

`governance/workflow/TASK_BOARD.md` rows are not machine-parsed: `grep -rn "TASK_BOARD"` over `governance/scripts`, `offline-installer-runtime/scripts`, and `product/**/*.{ts,rs,ps1}` returned no matches. Row/table structure was left untouched; only a one-line "How to read this board" note was added after the status legend.

## Line counts (before -> after)

| File | Before | After | Delta |
|---|---|---|---|
| PROJECT_CODEX.md | 110 | 100 | -10 |
| AGENTS.md | 273 | 273 | 0 (unchanged; canonical, mirrors CLAUDE.md) |
| CLAUDE.md | 273 | 273 | 0 (unchanged; canonical, mirrors AGENTS.md) |
| build_rules.md | 71 | 71 | 0 (already tight; no duplicate prose found beyond pre-existing pointers) |
| governance/spec/PRODUCT_SPEC.md | 683 | 683 | 0 (kept as canonical POLICY/VERSION home; not the side with duplicate text) |
| governance/spec/TECHNICAL_DESIGN.md | 874 | 861 | -13 |
| governance/workflow/TASK_BOARD.md | 343 | 347 | +4 (header note added; no rows changed) |

## Verification: rule ID before/after presence and single-definition check

Baseline extraction command (run before edits): `grep -o "\[VV-[A-Z0-9-]*\]" <file>` and `grep -o "\[OPERATOR-AUTHORITY-[0-9-]*\]" <file>` over `AGENTS.md`, `CLAUDE.md`, `PROJECT_CODEX.md`, `build_rules.md`, `governance/spec/PRODUCT_SPEC.md`, `governance/spec/TECHNICAL_DESIGN.md`.

- Distinct rule IDs found before edits: 74.
- Distinct rule IDs found after edits: 74.
- `comm -23 idlist_before idlist_after` (IDs lost): empty — none lost.
- `comm -13 idlist_before idlist_after` (IDs invented): empty — none invented.
- Per-ID definition-file check (a line matching `^- [ID] ...`): every ID has exactly one definition location, except the 34 IDs owned by the AGENTS.md/CLAUDE.md mirror pair (intentionally defined identically in both, per operator mirror policy) and `VV-BUILD-INSTALL-000`/`VV-BUILD-INSTALL-010`, whose pre-existing canonical home is `offline-installer-runtime/GUIDE.md` (build_rules.md only ever held a pointer to them; unchanged by this pass, confirmed out of S8 scope).
- `diff CLAUDE.md AGENTS.md`: empty output — the two files are byte-identical (pre-existing state; S8 did not edit either file, so the mirror invariant was never at risk).

</topic>

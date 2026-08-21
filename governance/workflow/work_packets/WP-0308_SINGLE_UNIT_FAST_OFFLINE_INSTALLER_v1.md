---
file_id: WP-0308-v1
file_kind: work-packet
updated_at: 2026-08-21
---

<topic id="contract" status="in-progress" version="v1" wp="WP-0308" updated_at="2026-08-21">

# Work Packet: WP-0308 — Single-unit fast offline installer

## Metadata

- ID: WP-0308
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-08-21
- Refinement: `WP-0308_SINGLE_UNIT_FAST_OFFLINE_INSTALLER_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0308`
- Related/superseded delivery detail: WP-0265

## Intent

Make every future public VoxVulgi install/update one non-technical ISO download and materially reduce installation time by replacing raw recursive Inno payload entries with a few directly extracted non-solid archives.

## Base scope

- Implement the complete refinement across repo authority, product/technical specs, the Inno installer, governed build driver, tests, artifact manifests, logs, and proof.
- Preserve the full offline payload, Inno 7 long-path handling, core NSIS maintenance labels/behavior, `shellexec` elevation, semantic-version build policy, and all user data.
- Do not build or run the full payload while the operator's current v0.1.175 installation is actively saturating C:/D:; fixture verification may proceed quietly.

## Required order

1. Refine and update canonical authority/spec.
2. Implement external non-solid archive extraction and durable logging.
3. Implement archive/ISO build, reuse, audit, and single-public-artifact finalization.
4. Add focused regression, corruption, long-path, user-data-boundary, and representative performance tests.
5. Build the next semantic version and run full clean-profile/offline performance proof after the live installation no longer owns the disks.

## Acceptance and proof

- The refinement is the normative research, architecture, ROI, red-team, risk, hardening, and verification contract.
- Source/contracts may be completed during the current install, but WP-0308 cannot be `DONE` until one full governed ISO build and the clean-profile/offline timing gate pass.
- A build log or compiled wrapper alone cannot prove single-unit delivery; the final ISO must be independently listed and contain every expected archive and the root installer.
- No success claim may rely on a toy fixture in place of the full-payload timing gate.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0308" updated_at="2026-08-21">

# Status updates

- 2026-08-21: Created from the exact v0.1.175 slow-install observation, current installer/process/disk evidence, current WP-0265/spec/build inspection, and current official Inno Setup 7, 7-Zip 26.02, and Microsoft Oscdimg documentation. Selected one UDF ISO plus native Inno extraction of five external non-solid 7z archives.
- 2026-08-21: Implemented the one-ISO build driver, content-matched archive cache, non-solid fast-LZMA2 payload archives, source/archive SHA-256 checks, archive path/link audit, UDF creation, independent ISO listing, one-download artifact manifest, exact uncompressed progress bytes, per-archive runtime hashes, phase labels, and durable latest/final installer logs.
- 2026-08-21: Inno Setup 7.1.0 compiled the governed wrapper cleanly. A direct external-archive fixture installed byte-identical nested outputs, created the durable checkpoint log while Setup's source log remained open, and logged successful SHA-256 verification/extraction; replacing the archive with different bytes failed closed with exit code 5 and `File hash is incorrect`.
- 2026-08-21: Focused installer contracts and the complete desktop contract suite passed (249/249). Both PowerShell scripts passed an independent `System.Management.Automation.Language.Parser` syntax check. The repeatable >=2x representative speed gate is implemented but not run while v0.1.175 owns the operator's disks.
- 2026-08-21: Hardened logging from extraction-only evidence into an end-to-end audit: named payload start/completion events, elevated core handoff start/return, before/after installed state, observed registry/install-path/binary versions, explicit verification/failure reason, and terminal outcome. Core postcondition failure now fails the wrapper and preserves latest/timestamped logs.
- 2026-08-21: Inno Setup 7.1.0 compiled the hardened wrapper with the existing v0.1.175 core setup as a syntax-only fixture, and the complete desktop contract suite passed 251/251. This compile is not a rebuilt or publishable full ISO and does not advance the remaining full-payload proof gate.
- 2026-08-21: Remaining hard predecessor for `DONE`: build the next semantic-version full ISO, independently list it, run a clean-profile fully offline install/update, prove the <=30-minute local-SSD gate, and prove the default localization workflow offline after the active v0.1.175 installation releases C:/D:.

</topic>

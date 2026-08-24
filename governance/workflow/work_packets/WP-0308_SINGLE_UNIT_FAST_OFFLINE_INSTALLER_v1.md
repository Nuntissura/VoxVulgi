---
file_id: WP-0308-v1
file_kind: work-packet
updated_at: 2026-08-24
---

<topic id="contract" status="in-progress" version="v1" wp="WP-0308" updated_at="2026-08-24">

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

Make every future public VoxVulgi install/update one non-technical ISO download and materially reduce installation time by replacing raw recursive Inno payload entries with five directly extracted bounded-solid `.7z` archives using a 64 MiB solid block.

## Base scope

- Implement the complete refinement across repo authority, product/technical specs, the Inno installer, governed build driver, tests, artifact manifests, logs, and proof.
- Preserve the full offline payload, Inno 7 long-path handling, core NSIS maintenance labels/behavior, `shellexec` elevation, semantic-version build policy, and all user data.
- Do not build or run the full payload while the operator's current v0.1.175 installation is actively saturating C:/D:; fixture verification may proceed quietly.

## Required order

1. Refine and update canonical authority/spec.
2. Implement external 64 MiB bounded-solid archive extraction and durable logging while preserving Inno `SolidCompression=no` and `DiskSpanning=no`.
3. Implement archive/ISO build, reuse, audit, and single-public-artifact finalization.
4. Add focused regression, corruption, long-path, user-data-boundary, and representative performance tests.
5. Build the next semantic version and run full clean-profile/offline performance proof after the live installation no longer owns the disks.

## Acceptance and proof

- The refinement is the normative research, architecture, ROI, red-team, risk, hardening, and verification contract.
- Source/contracts may be completed during the current install, but WP-0308 cannot be `DONE` until one full governed ISO build and the clean-profile/offline timing gate pass.
- A build log or compiled wrapper alone cannot prove single-unit delivery; the final ISO must be independently listed and contain every expected archive and the root installer.
- No success claim may rely on a toy fixture in place of the full-payload timing gate.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0308" updated_at="2026-08-24">

# Status updates

- 2026-08-21: Created from the exact v0.1.175 slow-install observation, current installer/process/disk evidence, current WP-0265/spec/build inspection, and current official Inno Setup 7, 7-Zip 26.02, and Microsoft Oscdimg documentation. Initially selected one UDF ISO plus native Inno extraction of five external non-solid 7z archives; the archive-solid policy was superseded by the measured 2026-08-24 selection below.
- 2026-08-21: Implemented the one-ISO build driver, content-matched archive cache, initial non-solid fast-LZMA2 payload archives, source/archive SHA-256 checks, archive path/link audit, UDF creation, independent ISO listing, one-download artifact manifest, exact uncompressed progress bytes, per-archive runtime hashes, phase labels, and durable latest/final installer logs. The archive builder was later revised to the measured 64 MiB bounded-solid winner.
- 2026-08-21: Inno Setup 7.1.0 compiled the governed wrapper cleanly. A direct external-archive fixture installed byte-identical nested outputs, created the durable checkpoint log while Setup's source log remained open, and logged successful SHA-256 verification/extraction; replacing the archive with different bytes failed closed with exit code 5 and `File hash is incorrect`.
- 2026-08-21: Focused installer contracts and the complete desktop contract suite passed (249/249). Both PowerShell scripts passed an independent `System.Management.Automation.Language.Parser` syntax check. The repeatable >=2x representative speed gate is implemented but not run while v0.1.175 owns the operator's disks.
- 2026-08-21: Hardened logging from extraction-only evidence into an end-to-end audit: named payload start/completion events, elevated core handoff start/return, before/after installed state, observed registry/install-path/binary versions, explicit verification/failure reason, and terminal outcome. Core postcondition failure now fails the wrapper and preserves latest/timestamped logs.
- 2026-08-21: Inno Setup 7.1.0 compiled the hardened wrapper with the existing v0.1.175 core setup as a syntax-only fixture, and the complete desktop contract suite passed 251/251. This compile is not a rebuilt or publishable full ISO and does not advance the remaining full-payload proof gate.
- 2026-08-21: Remaining hard predecessor for `DONE`: build the next semantic-version full ISO, independently list it, run a clean-profile fully offline install/update, prove the <=30-minute local-SSD gate, and prove the default localization workflow offline after the active v0.1.175 installation releases C:/D:.
- 2026-08-23: The corrected real installer wait exposed a genuine performance regression: the 20,000-file `[Files] extractarchive` fixture took 730.078 seconds versus 430.115 seconds raw (0.589x, required 2x). Inno 7.1.0 source inspection identified per-member extraction and temp/rename processing. At that stage, the wrapper and fixture retained external non-solid `.7z` plus `ArchiveExtraction=enhanced/nopassword` but changed to runtime hashing and bulk extraction into a recoverable staging tree before managed-root promotion. A 1,000-file compile/execution smoke passed; the later 2026-08-24 result superseded the archive block policy.
- 2026-08-24: After the non-solid bulk layout failed the >=2x gate, current primary-source research compared bounded-solid 7z, Microsoft WIMGAPI/WIM apply, and wimlib. WIM remained technically viable but was rejected for this packet because it would add a new helper/API or GPLv3 CLI distribution surface, new provenance and installer integration, and no measured benefit after the lower-change archive candidate passed the exact gate.
- 2026-08-24: The canonical production-shaped 20,000-file benchmark selected fast LZMA2 with a bounded 64 MiB solid block. Four counterbalanced trials produced a 222.717-second legacy-raw median and 104.677-second archive median: 2.128x, passing the required >=2x gate. Raw and archive destinations had identical output-tree SHA-256. `ArchiveExtraction=enhanced/nopassword`, `DiskSpanning=no`, and Inno `SolidCompression=no` remain required; the latter controls Inno's compiled wrapper stream, not the external archive block policy.
- 2026-08-24: Governed v0.1.181 desktop and NSIS artifacts built after all six offline pack warmup gates passed. The full builder produced `VoxVulgi_0.1.181_x64_offline_full.iso` as one 8,008,456,192-byte UDF image. An independent 7-Zip listing found the root `Install_VoxVulgi.exe`, README, payload manifest, exactly five external `.7z` archives, and no `.bin` slices; an independent SHA-256 read matched the artifact manifest at `54b517b997b95449c82eb7a5f3e9a2826b545c819f84819938ad362992bc1c93`.
- 2026-08-24: A final adversarial review found and closed ambiguous top-level archive-solid metadata. Governed v0.1.182 records the realized per-archive truth: four `Solid=+` archives and one valid `Solid=-` models archive composed of two oversized singleton blocks, all under the same audited 64 MiB bounded-solid policy. The rebuilt single UDF ISO is 8,008,458,240 bytes with SHA-256 `e423fb9c13307f81cded7c2e6b2a5b4b9ddb39047f26aad047b80bd7363a091d`; an independent 7-Zip listing confirmed UDF 1.02, root `Install_VoxVulgi.exe`, README, payload manifest, exactly five `.7z` archives, and no `.bin` slices.
- 2026-08-24: WP-0308 remains `IN_PROGRESS`. The semantic-version ISO and independent topology/hash proof are complete, but a clean-profile fully offline install/update, <=30-minute local-SSD proof, and default offline localization workflow proof remain required before publication or `DONE`.

</topic>

# Work Packet: WP-0240 - vvstart repo-rule acknowledgement contract

## Status

DONE

## Owner

Codex

## Operator Request Preserved

- "verified? you should read and acknowledge the repo rules and instructions. change the cmd+script so this does not happen again"

## Intent

- What: Make `vvstart.cmd` / `governance/scripts/vv_start.ps1` emit an explicit required next-response acknowledgement contract after printing the repo authority surfaces.
- Why: The previous bootstrap text only said to read and follow the files. That was too soft; an agent could incorrectly report only that the command ran or that files existed.

## Scope

In scope:

- Add a required acknowledgement section to the bootstrap output.
- Make the `.cmd` entrypoint explicitly request that acknowledgement contract.
- Add a focused regression test for the bootstrap output.
- Keep the change inside governance/startup tooling.

Out of scope:

- Product runtime code.
- Desktop build or installer output.
- Rewriting authority surfaces.

## Research Basis

- Current repo path inspected:
  - `vvstart.cmd`
  - `governance/scripts/vv_start.ps1`
  - `justfile`
  - `PROJECT_CODEX.md`
  - `MODEL_BEHAVIOR.md`
  - `AGENTS.md`
  - `CLAUDE.md`
  - `governance/workflow/PROOF_STANDARD.md`
- Existing pattern found: `vvstart.cmd` is a thin wrapper around `vv_start.ps1`; `just vv-start` invokes the same script directly.
- Reuse opportunity: keep `vv_start.ps1` as the single bootstrap renderer so both `vvstart.cmd` and `just vv-start` get the same contract.
- Rejected option: add a separate post-start script. It would split the model bootstrap path and make future drift more likely.
- Selected approach: add the acknowledgement contract directly to `vv_start.ps1`, keep it enabled by default, and pass `-RequireAcknowledgement` from `vvstart.cmd` for explicitness.

## Acceptance Criteria

- `vvstart.cmd` invokes `vv_start.ps1` with the acknowledgement contract enabled.
- `vv_start.ps1` output includes a required agent acknowledgement section before the long file dump.
- `vv_start.ps1` output tells agents not to report only command completion or file existence.
- `vv_start.ps1` output includes an exact acknowledgement opening that names `PROJECT_CODEX.md`, `MODEL_BEHAVIOR.md`, and `AGENTS.md`.
- A focused test verifies the bootstrap contract.

## Verification

- Run `powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File governance/scripts/test_vv_start.ps1`.
- Run `.\vvstart.cmd` and confirm the acknowledgement contract appears in the output.
- Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0240/2026-05-20_vvstart_ack/summary.md`.

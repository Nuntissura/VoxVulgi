# Work Packet: WP-0155 - Voice clone basics contract and governance refresh

## Metadata
- ID: WP-0155
- Owner: Codex
- Status: DONE
- Created: 2026-03-24
- Target milestone: Educational-core voice-clone recovery

## Intent

- What: Convert the latest reusable-voice inspection into an explicit product/runtime contract for the educational-core cloning path, then queue narrowly scoped follow-on packets instead of one broad "voice cloning improvement" bucket.
- Why: The repo now contains substantial voice-template, cast-pack, memory, character, benchmark, and backend surfaces, but the basic operator promise is still too diffuse: capture a reusable voice, apply it to a later translated item, run the dub, and know whether the result was truly voice-preserved rather than plain TTS fallback.

## Scope

In scope:

- Record the current reusable-voice findings in governance.
- Update spec/design wording for the reusable-voice basics contract.
- Split remediation into smaller packets that isolate runtime truthfulness, first-run UX, reusable-asset drift, and proof.

Out of scope:

- Shipping the remediation itself.
- Replacing the managed OpenVoice + Kokoro default backend family in this packet.

## Acceptance criteria

- `PRODUCT_SPEC.md` and `TECHNICAL_DESIGN.md` explicitly define the basic reusable-voice contract and the clone-vs-fallback truthfulness requirement.
- `ROADMAP.md` and `TASK_BOARD.md` include the split follow-on packets.
- The follow-on packet set is narrow enough that failure, drift, or debt can be attributed to one packet rather than disappearing into a broad umbrella effort.

## Test / verification plan

- Governance review of the updated spec/design wording.
- Traceability check that all follow-on packets are queued in roadmap and task board.

## Risks / open questions

- The repo already has overlapping reusable-voice layers; governance alone does not remove that overlap.
- The runtime truthfulness question must be answered before the product can honestly claim cloned-voice success on the educational core path.

## Status updates

- 2026-03-24: Created from a code-and-governance inspection of Localization Studio's reusable-voice path.
- 2026-03-24: Completed. Spec/design now define the reusable-voice basics contract and queue `WP-0156` to `WP-0159` as the split remediation set.

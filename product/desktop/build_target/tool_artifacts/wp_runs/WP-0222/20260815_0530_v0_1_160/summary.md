---
file_id: WP-0222-PROOF-20260815-0530
file_kind: proof-summary
updated_at: 2026-08-15
wp: WP-0222
app_version: 0.1.160
outcome: BLOCKED
---

<topic id="outcome" status="blocked" version="v1" wp="WP-0222" updated_at="2026-08-15">

# Outcome

WP-0222's code and deterministic regression gates pass. Completion remains blocked only on the packet's explicit installed-app manual smoke: reveal an existing Library media file, observe Explorer select it, and confirm no error toast appears. Agent execution of that step would launch a foreground Explorer window and violate the repository's quiet/no-focus-steal testing rule.

</topic>

<topic id="verification" status="pass" version="v1" wp="WP-0222" updated_at="2026-08-15" ingestable="true">

# Verification commands and observations

```text
$env:CARGO_BUILD_JOBS=1; $env:RUST_TEST_THREADS=1
cargo test --manifest-path product/desktop/src-tauri/Cargo.toml windows_reveal_accepts_exit_one_only_for_file_selection -- --nocapture
Result: PASS; 1 passed, 0 failed. Cargo and rustc inherited BelowNormal priority.

node --import tsx --test tests/revealMediaTruthfulnessContract.test.ts
Result: PASS; 2 passed, 0 failed.

Source contract:
- successful OS status always succeeds;
- exit code 1 succeeds only when /select, is used for a file;
- folder reveal exit 1, file reveal exit 2, and missing exit code fail;
- genuine frontend reveal failure awaits copyPathToClipboard(item.media_path) before constructing the error state.

Managed packaged lineage:
- implementation has shipped since v0.1.19;
- current governed installer is v0.1.160;
- BUILD_CHANGELOG records WP-0222 in the v0.1.19 slice.
```

</topic>

<topic id="provenance-correction" status="complete" version="v1" wp="WP-0222" updated_at="2026-08-15">

# Provenance correction

The old packet called exit code 1 “Microsoft's documented behavior.” The checked Microsoft community pages document `/select,` syntax, not exit-code semantics. The success-with-exit-1 fact is therefore attributed to the operator's exact observed pre-fix runtime case, where Explorer opened while the app received `Some(1)` and showed the false error.

</topic>

<topic id="blocker" status="blocked" version="v1" wp="WP-0222" updated_at="2026-08-15">

# Exact remaining operator action

In the installed VoxVulgi v0.1.160 app, open Media Library, choose an existing on-disk item, trigger its file-reveal/Open-folder action, and report whether Explorer selected the target without a VoxVulgi error toast. This one observation passes or fails the final acceptance gate.

</topic>

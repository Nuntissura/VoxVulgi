# WP-0190 Summary

Status: DONE
Date: 2026-08-14

## Outcome
- The desktop shell uses a 19 px scaled base font, raises legacy 11–14 px inline text through explicit compatibility selectors, and renders runtime application metadata beside the VoxVulgi brand.
- The visible version is sourced from `appInfo.app_version`; the same value drives the document title and accessible brand label.

## Verification
- Inspected `product/desktop/src/App.css`: root base text uses `calc(19px * var(--font-scale, 1))`, legacy small-text compatibility rules are present, and `.brand-version` has dedicated styling.
- Inspected `product/desktop/src/App.tsx`: the brand, accessible label, and document title read `appInfo.app_version` rather than a duplicated literal.
- Governed v0.1.138 build completed successfully.
- Directly inspected the settled v0.1.138 screenshots for all eight top-level pages. Each visibly showed the `v0.1.138` badge and readable navigation, headings, labels, controls, and secondary text without overlap at the captured viewport.

## Evidence
- `evidence.json`
- `governance/snapshots/WP-0171_build_0_1_138/`
- `product/desktop/build_target/logs/build_desktop_target_20260814-143555_0_1_138.log`

## Notes
- No accessibility preference panel was added; that remains outside this packet's scope.

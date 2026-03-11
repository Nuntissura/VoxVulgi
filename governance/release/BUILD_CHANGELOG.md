# VoxVulgi Build Changelog

This changelog tracks desktop installer builds produced by `governance/scripts/build_desktop_target.ps1`.

## Policy

- Every desktop target build must increment the desktop app semantic version.
- Every desktop target build must append a build entry in this file.
- Every build entry must include the Work Packet IDs included in that build.
- Build entries are append-only and listed newest last.

## Entry Template

## <version> - <UTC timestamp>
- Work Packets: `<WP-ID>`, `<WP-ID>`
- Commit: `<short-sha>`
- Offline Bundle ID: `<bundle-id>`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_<version>_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_<version>_x64_en-US.msi`
- Notes: `<freeform summary>`

## Historical Baseline

## 0.1.0 - 2026-03-02T00:00:00Z
- Work Packets: `WP-0001` .. `WP-0064`
- Commit: `a289631`
- Offline Bundle ID: `offline_full_win64_20260301_232842`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.0_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.0_x64_en-US.msi`
- Notes: Baseline build before automated build changelog/version policy enforcement.

## 0.1.2 - 2026-03-03T06:41:59Z
- Work Packets: `WP-0071`
- Commit: `47fd7a6`
- Offline Bundle ID: `offline_full_win64_20260303_061326`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.2_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.2_x64_en-US.msi`
- Notes: Installer UX clarity: explicit Update/Repair vs Full reinstall vs Uninstall wording; app-data deletion text clarified.

## 0.1.3 - 2026-03-03T19:39:46Z
- Work Packets: `WP-0072`
- Commit: `74904a5`
- Offline Bundle ID: `offline_full_win64_20260303_191450`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.3_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.3_x64_en-US.msi`
- Notes: Installer pre-maintenance explainer page before maintenance action selection (Update/Repair, Full reinstall, Uninstall).

## 0.1.4 - 2026-03-07T00:27:49Z
- Work Packets: `WP-0092`, `WP-0093`, `WP-0094`
- Commit: `06db8ea`
- Offline Bundle ID: `offline_full_win64_20260306_235943`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.4_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.4_x64_en-US.msi`
- Notes: Installer build after voice workflow remediation hardening on 2026-03-07.

## 0.1.5 - 2026-03-08T19:48:51Z
- Work Packets: `WP-0129`, `WP-0130`
- Commit: `eb54fd6`
- Offline Bundle ID: `offline_full_win64_20260308_191916`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.5_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.5_x64_en-US.msi`
- Notes: Desktop target build via build_desktop_target.ps1.

## 0.1.6 - 2026-03-11T18:02:38Z
- Work Packets: `WP-0141`
- Commit: `40e0e3c`
- Offline Bundle ID: `offline_full_win64_20260311_173920`
- Artifacts:
  - `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.6_x64-setup.exe`
  - `product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_0.1.6_x64_en-US.msi`
- Notes: Installer maintenance standard refresh.

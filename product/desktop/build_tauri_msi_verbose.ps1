Set-Location product/desktop
$env:VVOFFLINE="1"
npm run tauri -- build --bundles msi --verbose

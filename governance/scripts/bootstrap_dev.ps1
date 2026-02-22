$ErrorActionPreference = "Stop"

function Step([string]$msg) {
  Write-Host ""
  Write-Host "==> $msg"
}

$root = (Resolve-Path (Join-Path $PSScriptRoot "..\\..")).Path

Step "Repo root: $root"

Step "Toolchain versions"
try { node --version } catch { Write-Warning "node not found" }
try { npm --version } catch { Write-Warning "npm not found" }
try { cargo --version } catch { Write-Warning "cargo not found" }
try { rustc --version } catch { Write-Warning "rustc not found" }

Step "Install JS deps (product/desktop)"
Push-Location (Join-Path $root "product\\desktop")
if (Test-Path "package-lock.json") {
  npm ci
} else {
  npm install
}
Pop-Location

Step "Fetch Rust deps (product/engine)"
Push-Location (Join-Path $root "product\\engine")
cargo fetch
Pop-Location

Step "Fetch Rust deps (product/desktop/src-tauri)"
Push-Location (Join-Path $root "product\\desktop\\src-tauri")
cargo fetch
Pop-Location

Step "Install runtime dependencies into app data (FFmpeg + whispercpp-tiny)"
Push-Location (Join-Path $root "product\\engine")
cargo run --bin voxvulgi_setup -- --install-all
Pop-Location

Step "Done"

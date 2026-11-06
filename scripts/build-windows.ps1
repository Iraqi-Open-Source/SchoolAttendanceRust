$ErrorActionPreference = 'Stop'
Set-Location (Resolve-Path (Join-Path $PSScriptRoot '..'))
if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
  throw 'Install Rust using https://rustup.rs first, then restart PowerShell.'
}
rustup toolchain install 1.77.2 --profile minimal
if ($LASTEXITCODE -ne 0) { throw 'rustup install failed' }
rustup toolchain install stable --profile minimal
if ($LASTEXITCODE -ne 0) { throw 'rustup install failed' }
$cli = (cargo tauri --version 2>$null | Out-String)
if ($cli -notmatch '1\.[0-9]+') {
  cargo +stable install tauri-cli --version 1.6.3 --locked
  if ($LASTEXITCODE -ne 0) { throw 'Tauri CLI install failed' }
}
cargo +stable generate-lockfile --manifest-path src-tauri/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw 'Could not generate Cargo.lock' }
python scripts/prepare_legacy_lock.py
if ($LASTEXITCODE -ne 0) { throw 'Could not prepare a Rust 1.77 lockfile' }
cargo +1.77.2 tauri build
if ($LASTEXITCODE -ne 0) { throw 'Build failed. Review dependency compatibility and cargo errors.' }
Write-Host 'Done. See src-tauri\target\release\bundle\ and school_attendance.exe'

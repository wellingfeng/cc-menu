param(
  [string]$Configuration = "release"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $root "dist"
$release = Join-Path $root "release"

if (Test-Path -LiteralPath $dist) {
  Remove-Item -LiteralPath $dist -Recurse -Force
}
if (Test-Path -LiteralPath $release) {
  Remove-Item -LiteralPath $release -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $dist, $release | Out-Null

cargo fmt --all -- --check
cargo test --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "self-test.ps1")
cargo build -p cc-menu --release

$cliExe = Join-Path $root "target\release\cc-menu.exe"
if (-not (Test-Path -LiteralPath $cliExe)) {
  $cliExe = Join-Path $root "target\release\cc-menu"
}
if (-not (Test-Path -LiteralPath $cliExe)) {
  throw "Could not find built cc-menu executable"
}

$env:CC_MENU_CLI_EXE = $cliExe
cargo build -p cc-menu-installer --release --features embedded-payload

$installerExe = Join-Path $root "target\release\cc-menu-installer.exe"
if (-not (Test-Path -LiteralPath $installerExe)) {
  $installerExe = Join-Path $root "target\release\cc-menu-installer"
}
if (-not (Test-Path -LiteralPath $installerExe)) {
  throw "Could not find built installer executable"
}

$publicInstaller = Join-Path $release "cc-menu-setup-win-x64.exe"
Copy-Item -LiteralPath $installerExe -Destination $publicInstaller -Force
Copy-Item -LiteralPath $cliExe -Destination (Join-Path $dist "cc-menu.exe") -Force
Copy-Item -LiteralPath "README.md" -Destination (Join-Path $dist "README.md") -Force
Copy-Item -LiteralPath "LICENSE" -Destination (Join-Path $dist "LICENSE") -Force
Compress-Archive -Path (Join-Path $dist "*") -DestinationPath (Join-Path $release "cc-menu-portable-win-x64.zip") -Force

& $publicInstaller --self-test

$manifest = @{
  name = "cc-menu"
  version = (cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).packages[0].version
  built_at = (Get-Date).ToUniversalTime().ToString("o")
  assets = @(
    @{ path = "cc-menu-setup-win-x64.exe"; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $publicInstaller).Hash.ToLowerInvariant() },
    @{ path = "cc-menu-portable-win-x64.zip"; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $release "cc-menu-portable-win-x64.zip")).Hash.ToLowerInvariant() }
  )
}
$manifest | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $release "manifest.json") -Encoding UTF8

@"
# CC Menu Release

Assets:

- cc-menu-setup-win-x64.exe: per-user Windows installer with embedded CLI payload.
- cc-menu-portable-win-x64.zip: portable CLI build.
- manifest.json: SHA-256 hashes and build metadata.

Validation completed by build script:

- cargo fmt --all -- --check
- cargo test --workspace
- scripts/self-test.ps1
- cc-menu-setup-win-x64.exe --self-test
"@ | Set-Content -LiteralPath (Join-Path $release "release-notes.md") -Encoding UTF8

Get-ChildItem -LiteralPath $release | Format-Table Name, Length

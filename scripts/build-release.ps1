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

$doubleClickSmokeRoot = Join-Path $root ".cc-menu-test\installer-double-click"
if (Test-Path -LiteralPath $doubleClickSmokeRoot) {
  Remove-Item -LiteralPath $doubleClickSmokeRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $doubleClickSmokeRoot | Out-Null
$doubleClickInstallDir = Join-Path $doubleClickSmokeRoot "Programs\cc-menu"
$registryPrefix = "CCMenuBuildTest"
$doubleClickOutput = "`r`n" | & $publicInstaller --install-dir $doubleClickInstallDir --registry-prefix $registryPrefix --wait
if ($LASTEXITCODE -ne 0) {
  throw "Installer no-argument smoke test failed"
}
$doubleClickOutputText = $doubleClickOutput -join "`n"
if ($doubleClickOutputText -notmatch "Press Enter to close this installer") {
  throw "Installer no-argument smoke test did not expose the double-click pause prompt"
}
if (-not (Test-Path -LiteralPath (Join-Path $doubleClickInstallDir "cc-menu.exe"))) {
  throw "Installer no-argument smoke test did not install cc-menu.exe"
}
foreach ($key in @(
  "HKCU\Software\Classes\Directory\Background\shell\$registryPrefix",
  "HKCU\Software\Classes\Directory\Background\shell\$registryPrefix`Codex",
  "HKCU\Software\Classes\Directory\shell\$registryPrefix",
  "HKCU\Software\Classes\Directory\shell\$registryPrefix`Codex",
  "HKCU\Software\Classes\Folder\shell\$registryPrefix",
  "HKCU\Software\Classes\Folder\shell\$registryPrefix`Codex"
)) {
  reg query $key | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "Installer no-argument smoke test did not create registry key: $key"
  }
}
& $publicInstaller --install-dir $doubleClickInstallDir --registry-prefix $registryPrefix --uninstall --quiet
if ($LASTEXITCODE -ne 0) {
  throw "Installer no-argument smoke test failed to uninstall"
}
foreach ($key in @(
  "HKCU\Software\Classes\Directory\Background\shell\$registryPrefix",
  "HKCU\Software\Classes\Directory\shell\$registryPrefix",
  "HKCU\Software\Classes\Folder\shell\$registryPrefix"
)) {
  $previousErrorActionPreference = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  reg query $key *> $null
  $exists = $LASTEXITCODE -eq 0
  $ErrorActionPreference = $previousErrorActionPreference
  if ($exists) {
    throw "Installer no-argument smoke test did not remove registry key: $key"
  }
}

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
- cc-menu-setup-win-x64.exe no-argument double-click smoke test with visible pause prompt
- HKCU Explorer context menu registry create/delete verification
"@ | Set-Content -LiteralPath (Join-Path $release "release-notes.md") -Encoding UTF8

Get-ChildItem -LiteralPath $release | Format-Table Name, Length

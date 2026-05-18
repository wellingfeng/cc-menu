param(
  [Parameter(Mandatory=$true)]
  [string]$Repo,
  [string]$Tag = "",
  [string]$Title = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$release = Join-Path $root "release"
if (-not (Test-Path -LiteralPath $release)) {
  throw "Release directory does not exist. Run scripts\build-release.ps1 first."
}

gh auth status | Out-Host

if ([string]::IsNullOrWhiteSpace($Tag)) {
  $Tag = "v" + (Get-Date -Format "yyyyMMdd-HHmm")
}
if ([string]::IsNullOrWhiteSpace($Title)) {
  $Title = "CC Menu $Tag"
}

$notes = Join-Path $release "release-notes.md"
$assets = @(
  (Join-Path $release "cc-menu-setup-win-x64.exe"),
  (Join-Path $release "cc-menu-portable-win-x64.zip"),
  (Join-Path $release "manifest.json")
)
foreach ($asset in $assets) {
  if (-not (Test-Path -LiteralPath $asset)) {
    throw "Missing release asset: $asset"
  }
}

$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
& gh release view $Tag --repo $Repo --json tagName *> $null
$releaseExists = $LASTEXITCODE -eq 0
$ErrorActionPreference = $previousErrorActionPreference
if (-not $releaseExists) {
  & gh release create $Tag --repo $Repo --title $Title --notes-file $notes
  if ($LASTEXITCODE -ne 0) {
    throw "Failed to create release $Tag"
  }
}

& gh release upload $Tag --repo $Repo @assets --clobber
if ($LASTEXITCODE -ne 0) {
  throw "Failed to upload release assets for $Tag"
}
& gh release view $Tag --repo $Repo --json tagName,url,assets | Out-Host
if ($LASTEXITCODE -ne 0) {
  throw "Failed to verify release $Tag"
}

param(
  [string]$Workspace = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($Workspace)) {
  $Workspace = Join-Path $root ".cc-menu-test"
}

function Invoke-CargoChecked {
  & cargo @args
  if ($LASTEXITCODE -ne 0) {
    throw "Command failed: cargo $($args -join ' ')"
  }
}

function Invoke-CcMenuChecked {
  & $CcMenuExe @args
  if ($LASTEXITCODE -ne 0) {
    throw "Command failed: $CcMenuExe $($args -join ' ')"
  }
}

function Invoke-CcMenuJson {
  $output = & $CcMenuExe @args
  if ($LASTEXITCODE -ne 0) {
    throw "Command failed: $CcMenuExe $($args -join ' ')"
  }
  return ($output -join "`n")
}

if (Test-Path -LiteralPath $Workspace) {
  $resolved = Resolve-Path -LiteralPath $Workspace
  if (-not $resolved.Path.StartsWith($root)) {
    throw "Refusing to remove test workspace outside repository: $($resolved.Path)"
  }
  Remove-Item -LiteralPath $Workspace -Recurse -Force
}

Invoke-CargoChecked build -p cc-menu
$CcMenuExe = Join-Path $root "target\debug\cc-menu.exe"
if (-not (Test-Path -LiteralPath $CcMenuExe)) {
  $CcMenuExe = Join-Path $root "target\debug\cc-menu"
}
if (-not (Test-Path -LiteralPath $CcMenuExe)) {
  throw "Could not find debug cc-menu executable"
}

Invoke-CcMenuChecked --workspace $Workspace init
Invoke-CcMenuChecked --workspace $Workspace menu sync
$menuJson = Invoke-CcMenuJson --workspace $Workspace menu print --format json
$menu = $menuJson | ConvertFrom-Json
$labels = @($menu."top-level" | ForEach-Object { $_.label })
if (($labels -join "|") -ne "Claude Code|Codex|Gemini|CC-Menu") {
  throw "Unexpected top-level labels: $($labels -join ', ')"
}

Invoke-CcMenuChecked --workspace $Workspace launch --agent codex --cwd $root --mode native --dry-run
Invoke-CcMenuChecked --workspace $Workspace launch --agent codex --cwd $root --mode gateway --cache --tts --dry-run
Invoke-CcMenuChecked --workspace $Workspace gateway chat --strategy fixed --prompt "self test fixed"
Invoke-CcMenuChecked --workspace $Workspace gateway chat --strategy fallback --prompt "self test fallback"
Invoke-CcMenuChecked --workspace $Workspace gateway chat --strategy race --prompt "self test race"
Invoke-CcMenuChecked --workspace $Workspace gateway chat --strategy broadcast --prompt "self test broadcast"

$ccSwitch = Join-Path $Workspace "cc-switch.json"
@{
  agents = @(
    @{
      id = "custom-reviewer"
      "display-name" = "Custom Reviewer"
      command = @("reviewer")
      provider = "local"
      model = "reviewer-v1"
      account = "Local"
      endpoint = "local"
      "credential-ref" = $null
    }
  )
} | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $ccSwitch -Encoding UTF8
Invoke-CcMenuChecked --workspace $Workspace sync preview --file $ccSwitch
Invoke-CcMenuChecked --workspace $Workspace sync apply --file $ccSwitch

$sessionRoot = Join-Path $Workspace "sample-sessions"
New-Item -ItemType Directory -Force -Path $sessionRoot | Out-Null
$projectPath = (Resolve-Path -LiteralPath $root).Path
@{
  "session-id" = "codex-session-1"
  channel = "codex"
  "project-path" = $projectPath
  title = "Self-test session"
  "last-active" = (Get-Date).ToUniversalTime().ToString("o")
  "transcript-path" = (Join-Path $sessionRoot "transcript.jsonl")
} | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $sessionRoot "session.json") -Encoding UTF8
Invoke-CcMenuChecked --workspace $Workspace sessions scan
Invoke-CcMenuChecked --workspace $Workspace sessions list --cwd $projectPath
Invoke-CcMenuChecked --workspace $Workspace sessions resume --session codex-session-1 --target claude --cwd $projectPath
Invoke-CcMenuChecked --workspace $Workspace platform generate --platform windows --out (Join-Path $Workspace "platform-windows")
Invoke-CcMenuChecked --workspace $Workspace platform generate --platform macos --out (Join-Path $Workspace "platform-macos")
Invoke-CcMenuChecked --workspace $Workspace self-test

Write-Host "CC Menu self-test passed"

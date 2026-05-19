[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$releaseRoot = Join-Path $repoRoot "src-tauri\target\release\bundle"
$githubReleaseRoot = Join-Path $releaseRoot "github-release"
$cliRoot = Join-Path $releaseRoot "cli"
$webReleaseRoot = Join-Path $repoRoot "private-components\web-app\public\releases"

$requiredFiles = @(
  @{ Source = Join-Path $githubReleaseRoot "Dropply_1.0.0_x64_EN-FR-setup.exe"; Destination = "Dropply_1.0.0_x64_EN-FR-setup.exe" },
  @{ Source = Join-Path $githubReleaseRoot "Dropply_1.0.0_x64_EN-FR.msi"; Destination = "Dropply_1.0.0_x64_EN-FR.msi" },
  @{ Source = Join-Path $githubReleaseRoot "Dropply_1.0.0_x64_EN-FR_SHA256.txt"; Destination = "Dropply_1.0.0_x64_EN-FR_SHA256.txt" },
  @{ Source = Join-Path $githubReleaseRoot "Dropply_CLI_1.0.0_Windows_x64_EN-FR.zip"; Destination = "Dropply_CLI_1.0.0_Windows_x64_EN-FR.zip" },
  @{ Source = Join-Path $githubReleaseRoot "Dropply_BrowserShare_1.0.0_Extension.zip"; Destination = "Dropply_BrowserShare_1.0.0_Extension.zip" },
  @{ Source = Join-Path $cliRoot "README.txt"; Destination = "README.txt" }
)

New-Item -ItemType Directory -Force -Path $webReleaseRoot | Out-Null

foreach ($file in $requiredFiles) {
  if (-not (Test-Path -LiteralPath $file.Source)) {
    throw "Missing release asset: $($file.Source)"
  }

  Copy-Item -LiteralPath $file.Source -Destination (Join-Path $webReleaseRoot $file.Destination) -Force
}

Write-Host "Synced public web release assets to $webReleaseRoot" -ForegroundColor Green

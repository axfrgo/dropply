$ErrorActionPreference = "Stop"

$bundleDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$sourceExe = Join-Path $bundleDir "dropply-cli.exe"
$preferredDir = Join-Path $env:LOCALAPPDATA "Microsoft\WindowsApps"
$fallbackDir = Join-Path $env:LOCALAPPDATA "Dropply\bin"
$profilePath = $PROFILE.CurrentUserCurrentHost
$profileMarkerStart = "# >>> dropply-cli completion >>>"
$profileMarkerEnd = "# <<< dropply-cli completion <<<"

if (-not (Test-Path $sourceExe)) {
  throw "Could not find dropply-cli.exe beside the installer script."
}

function Get-InstallTarget {
  if (Test-Path $preferredDir) {
    $probe = Join-Path $preferredDir ".dropply-write-test.tmp"
    try {
      Set-Content -Path $probe -Value "ok" -Encoding ASCII
      Remove-Item -Force $probe
      return @{
        Path = $preferredDir
        NeedsPathUpdate = $false
      }
    } catch {
    }
  }

  return @{
    Path = $fallbackDir
    NeedsPathUpdate = $true
  }
}

$target = Get-InstallTarget
$targetDir = $target.Path
$targetExe = Join-Path $targetDir "dropply-cli.exe"
$targetCmd = Join-Path $targetDir "dropply-cli.cmd"
$completionScriptPath = Join-Path $targetDir "dropply-cli-completion.ps1"

New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
Copy-Item -Force -Path $sourceExe -Destination $targetExe

$cmdWrapper = @'
@echo off
"%~dp0dropply-cli.exe" %*
'@
Set-Content -Path $targetCmd -Value $cmdWrapper -Encoding ASCII

$completionScript = & $targetExe completions powershell
Set-Content -Path $completionScriptPath -Value $completionScript -Encoding UTF8

$escapedCompletionPath = $completionScriptPath.Replace("'", "''")
$profileSnippet = @"
$profileMarkerStart
if (Test-Path '$escapedCompletionPath') {
  . '$escapedCompletionPath'
}
$profileMarkerEnd
"@

if (-not (Test-Path (Split-Path -Parent $profilePath))) {
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $profilePath) | Out-Null
}

$profileContents = if (Test-Path $profilePath) {
  Get-Content -Path $profilePath -Raw
} else {
  ""
}

if ($profileContents -notmatch [regex]::Escape($profileMarkerStart)) {
  if ($profileContents -and -not $profileContents.EndsWith([Environment]::NewLine)) {
    Add-Content -Path $profilePath -Value ""
  }
  Add-Content -Path $profilePath -Value $profileSnippet -Encoding UTF8
}

if ($target.NeedsPathUpdate) {
  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  $pathEntries = @()
  if ($userPath) {
    $pathEntries = $userPath.Split(';') | Where-Object { $_.Trim() -ne '' }
  }

  $alreadyPresent = $pathEntries | Where-Object { $_.TrimEnd('\') -ieq $targetDir.TrimEnd('\') }
  if (-not $alreadyPresent) {
    $newPath = if ($userPath -and $userPath.Trim()) {
      "$userPath;$targetDir"
    } else {
      $targetDir
    }
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
  }
}

Write-Host ""
Write-Host "Dropply CLI installed to: $targetDir" -ForegroundColor Green
Write-Host "PowerShell completions are set up for new terminal sessions." -ForegroundColor Cyan
Write-Host "Open a new terminal window, then run: dropply-cli pair" -ForegroundColor Cyan

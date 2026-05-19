$ErrorActionPreference = "Stop"

$installDirs = @(
  (Join-Path $env:LOCALAPPDATA "Microsoft\WindowsApps"),
  (Join-Path $env:LOCALAPPDATA "Dropply\bin")
)
$profilePath = $PROFILE.CurrentUserCurrentHost
$profileMarkerStart = "# >>> dropply-cli completion >>>"
$profileMarkerEnd = "# <<< dropply-cli completion <<<"

foreach ($targetDir in $installDirs) {
  $targetExe = Join-Path $targetDir "dropply-cli.exe"
  $targetCmd = Join-Path $targetDir "dropply-cli.cmd"
  $completionScriptPath = Join-Path $targetDir "dropply-cli-completion.ps1"

  if (Test-Path $targetExe) {
    Remove-Item -Force $targetExe
  }

  if (Test-Path $targetCmd) {
    Remove-Item -Force $targetCmd
  }

  if (Test-Path $completionScriptPath) {
    Remove-Item -Force $completionScriptPath
  }

  if ((Test-Path $targetDir) -and ($targetDir -like "*\\Dropply\\bin")) {
    $remaining = Get-ChildItem -Force $targetDir -ErrorAction SilentlyContinue
    if (-not $remaining) {
      Remove-Item -Force $targetDir
    }
  }
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath) {
  $filtered = $userPath.Split(';') | Where-Object {
    $_.Trim() -and $_.TrimEnd('\') -ine (Join-Path $env:LOCALAPPDATA "Dropply\bin").TrimEnd('\')
  }
  [Environment]::SetEnvironmentVariable("Path", ($filtered -join ';'), "User")
}

if (Test-Path $profilePath) {
  $profileContents = Get-Content -Path $profilePath -Raw
  $snippetPattern = [regex]::Escape($profileMarkerStart) + ".*?" + [regex]::Escape($profileMarkerEnd)
  $updatedProfile = [regex]::Replace($profileContents, $snippetPattern, "", "Singleline").Trim()
  if ($updatedProfile) {
    Set-Content -Path $profilePath -Value $updatedProfile -Encoding UTF8
  } else {
    Clear-Content -Path $profilePath
  }
}

Write-Host ""
Write-Host "Dropply CLI removed." -ForegroundColor Green
Write-Host "Open a new terminal window to refresh your PATH." -ForegroundColor Cyan

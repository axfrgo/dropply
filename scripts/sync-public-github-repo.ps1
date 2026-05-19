[CmdletBinding()]
param(
  [string]$RepositoryUrl = "https://github.com/axfrgo/dropply.git",
  [string]$Branch = "main",
  [string]$Destination = ".publication\Dropply-public",
  [string]$CommitMessage = "Refresh public Dropply desktop and CLI sources"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

& (Join-Path $PSScriptRoot "prepare-public-repo.ps1") -Destination $Destination

$publicCopy = Join-Path $repoRoot $Destination
if (-not (Test-Path -LiteralPath $publicCopy)) {
  throw "Public copy was not created at $publicCopy"
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("dropply-public-sync-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null

try {
  Push-Location $tempRoot
  git clone --branch $Branch --single-branch $RepositoryUrl repo
  Pop-Location

  $cloneRoot = Join-Path $tempRoot "repo"

  robocopy $publicCopy $cloneRoot /MIR /XD "$cloneRoot\.git" > $null
  if ($LASTEXITCODE -ge 8) {
    throw "robocopy failed with exit code $LASTEXITCODE"
  }

  Push-Location $cloneRoot
  git status --short
  $pending = git status --porcelain
  if (-not $pending) {
    Write-Host "Public repo is already up to date." -ForegroundColor Green
    return
  }

  git add .
  git commit -m $CommitMessage
  git push origin $Branch
  Pop-Location
}
finally {
  if (Get-Location | Select-Object -ExpandProperty Path | Where-Object { $_ -like "$tempRoot*" }) {
    Pop-Location
  }

  if (Test-Path -LiteralPath $tempRoot) {
    Remove-Item -Recurse -Force -LiteralPath $tempRoot
  }
}

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$rootDir = [System.IO.Path]::GetFullPath((Join-Path $scriptDir ".."))
$backendSource = Join-Path $rootDir "private-components\backend"
$kernelSource = Join-Path $rootDir "packages\fortistate-kernel"
$preferredFcore = "C:\Users\alexj\Downloads\fcore-1.2.5\package\bin\fcore.js"
$fallbackFcore = Join-Path $rootDir "fcore-temp\package\bin\fcore.js"
$preferredNode = "C:\Users\alexj\Tools\node-v24.15.0-win-x64\node.exe"
$nodeExe = if (Test-Path $preferredNode) { $preferredNode } else { "node" }

if (Test-Path $preferredFcore) {
  $fcoreJs = $preferredFcore
} elseif (Test-Path $fallbackFcore) {
  $fcoreJs = $fallbackFcore
} else {
  throw "Could not find a FortiCore CLI. Checked:`n  $preferredFcore`n  $fallbackFcore"
}

$stagingRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("dropply-backend-fcore-" + [guid]::NewGuid().ToString("N"))
$stagingBackend = Join-Path $stagingRoot "backend"
$stagingKernel = Join-Path $stagingBackend "vendor\fortistate-kernel"

Write-Host "Using FortiCore CLI:" -ForegroundColor Cyan
Write-Host "  $fcoreJs"
Write-Host "Creating backend-only deploy bundle..." -ForegroundColor Cyan
Write-Host "  $stagingBackend"

New-Item -ItemType Directory -Force -Path $stagingBackend | Out-Null
New-Item -ItemType Directory -Force -Path $stagingKernel | Out-Null

$null = robocopy $backendSource $stagingBackend /MIR /XD node_modules dist dist-bench .turbo .next .git .data /XF *.log *.tmp *.bak
if ($LASTEXITCODE -gt 7) {
  throw "Failed to copy backend source with robocopy (exit $LASTEXITCODE)."
}

$null = robocopy $kernelSource $stagingKernel /MIR /XD node_modules dist .git
if ($LASTEXITCODE -gt 7) {
  throw "Failed to copy fortistate-kernel with robocopy (exit $LASTEXITCODE)."
}

$packageJsonPath = Join-Path $stagingBackend "package.json"
$packageLockPath = Join-Path $stagingBackend "package-lock.json"
$packageJson = Get-Content $packageJsonPath -Raw | ConvertFrom-Json
$packageJson.dependencies."@dropply/fortistate-kernel" = "file:./vendor/fortistate-kernel"
$packageJsonText = $packageJson | ConvertTo-Json -Depth 100
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($packageJsonPath, $packageJsonText, $utf8NoBom)

if (Test-Path $packageLockPath) {
  Remove-Item -LiteralPath $packageLockPath -Force
}

Push-Location $stagingBackend
try {
  Write-Host "Checking current dropply-backend status..." -ForegroundColor Cyan
  & $nodeExe $fcoreJs status dropply-backend
  if ($LASTEXITCODE -ne 0) {
    throw "Status check failed with exit code $LASTEXITCODE."
  }

  Write-Host ""
  Write-Host "Deploying dropply-backend from backend-only temp bundle..." -ForegroundColor Cyan
  Write-Host "This keeps FortiCore on the Fastify package and avoids packaging the full repo root."
  & $nodeExe $fcoreJs deploy . --name dropply-backend --no-cache --verbose
  if ($LASTEXITCODE -ne 0) {
    throw "Deploy command exited with code $LASTEXITCODE."
  }
}
finally {
  Pop-Location
}

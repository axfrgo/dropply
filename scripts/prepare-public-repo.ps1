param(
  [string]$Destination = ".publication\Dropply-public"
)

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$target = Join-Path $root $Destination
$destinationParent = Split-Path $target -Parent
if (!(Test-Path $destinationParent)) {
  New-Item -ItemType Directory -Force -Path $destinationParent | Out-Null
}

if (Test-Path $target) {
  Remove-Item -Recurse -Force $target
}

New-Item -ItemType Directory -Force -Path $target | Out-Null

# Mirror the repo, excluding private/build/staging artifacts
robocopy $root $target /MIR `
  /XD "$root\.git" `
      "$root\node_modules" `
      "$root\dist" `
      "$root\private-components" `
      "$root\src-tauri\target" `
      "$root\relay-server\target" `
      "$root\.publication" `
      "$root\.public-release" `
      "$root\.gemini" `
  /XF *.log > $null

Write-Host "Prepared public repo copy at $target"

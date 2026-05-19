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
      "$root\.tmp" `
      "$root\node_modules" `
      "$root\dist" `
      "$root\fcore-temp" `
      "$root\installers" `
      "$root\private-components" `
      "$root\src-tauri\target" `
      "$root\relay-server\target" `
      "$root\.publication" `
      "$root\.public-release" `
      "$root\.gemini" `
  /XF *.log `
      "Dropply_*.exe" `
      "Dropply_*.msi" > $null

$publicCargoToml = Join-Path $target "src-tauri\Cargo.toml"
if (Test-Path $publicCargoToml) {
  $cargo = Get-Content $publicCargoToml -Raw
  $cargo = $cargo -replace '(?ms)^\[features\]\r?\ndefault = \[\]\r?\nzenith = \["dep:zenith-core"\]\r?\n', "[features]`r`ndefault = []`r`n"
  $cargo = $cargo -replace '(?m)^zenith-core = \{ path = "\.\./private-components/zenith-substrate/zenith-kernel/zenith-core", optional = true \}\r?\n', ""
  Set-Content -Path $publicCargoToml -Value $cargo -Encoding ASCII
}

Write-Host "Prepared public repo copy at $target"

param(
  [string]$BundleDir = "src-tauri\target\release\bundle",
  [string]$OutputFile = "src-tauri\target\release\bundle\SHA256SUMS.txt",
  [string]$NamePrefix = "Dropply_"
)

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$bundlePath = Join-Path $root $BundleDir
$outputPath = Join-Path $root $OutputFile

if (!(Test-Path $bundlePath)) {
  throw "Bundle directory not found: $bundlePath"
}

$files = Get-ChildItem -Path $bundlePath -Recurse -File | Where-Object {
  $_.Extension -in @(".msi", ".exe", ".dmg", ".AppImage", ".deb", ".rpm") -and $_.Name.StartsWith($NamePrefix)
}

$lines = foreach ($file in $files) {
  $hash = Get-FileHash -Path $file.FullName -Algorithm SHA256
  "{0}  {1}" -f $hash.Hash.ToLowerInvariant(), $file.Name
}

Set-Content -Path $outputPath -Value $lines
Write-Host "Wrote checksums to $outputPath"

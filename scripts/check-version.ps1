# Verifies that package.json, Cargo.toml and (optionally) a release tag agree on the version.
param([string]$Tag = "")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$pkg = (Get-Content (Join-Path $root "package.json") -Raw | ConvertFrom-Json).version
$cargoLine = Select-String -Path (Join-Path $root "src-tauri/Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
$cargo = $cargoLine.Matches[0].Groups[1].Value
if ($pkg -ne $cargo) { Write-Error "package.json ($pkg) and Cargo.toml ($cargo) versions differ"; exit 1 }
if ($Tag) {
  $t = $Tag.TrimStart('v')
  if ($t -ne $pkg) { Write-Error "Tag $Tag does not match app version $pkg"; exit 1 }
}
Write-Host "Version OK: $pkg"

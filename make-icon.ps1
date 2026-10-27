# Rebuild Xnote's app icons from the approved Skale Club family master.
# icon-source.png is the artwork source; this script does not redraw the old X.
[CmdletBinding()]
param([string]$Source = (Join-Path $PSScriptRoot 'icon-source.png'))
$ErrorActionPreference = 'Stop'
$node = (Get-Command node -ErrorAction Stop).Source
$cli = Join-Path $PSScriptRoot 'node_modules/@tauri-apps/cli/tauri.js'
if (-not (Test-Path -LiteralPath $cli)) { throw 'Run npm ci before generating icons.' }
if (-not (Test-Path -LiteralPath $Source)) { throw "Icon source not found: $Source" }
$icons = Join-Path $PSScriptRoot 'src-tauri/icons'
& $node $cli icon $Source --output $icons
if ($LASTEXITCODE -ne 0) { throw "Icon generation failed with exit code $LASTEXITCODE" }
Copy-Item -LiteralPath (Join-Path $icons '128x128.png') -Destination (Join-Path $PSScriptRoot 'src/logo.png') -Force
Write-Host 'Xnote app and in-app icons regenerated.'

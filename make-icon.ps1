# Rebuild Xnote's app and in-app icons from the family mark (skaleclub-apps/design/skale-css/marks/xnote.svg).
[CmdletBinding()]
param([string]$Design = (Join-Path $HOME 'Dev/skaleclub-apps/design/skale-css'))
$ErrorActionPreference = 'Stop'
. (Join-Path $Design 'render-mark.ps1')

$icons = Join-Path $PSScriptRoot 'src-tauri/icons'
$pngs = @{
    '32x32.png' = 32; '64x64.png' = 64; '128x128.png' = 128; '128x128@2x.png' = 256; 'icon.png' = 512
    'StoreLogo.png' = 50; 'Square30x30Logo.png' = 30; 'Square44x44Logo.png' = 44; 'Square71x71Logo.png' = 71
    'Square89x89Logo.png' = 89; 'Square107x107Logo.png' = 107; 'Square142x142Logo.png' = 142
    'Square150x150Logo.png' = 150; 'Square284x284Logo.png' = 284; 'Square310x310Logo.png' = 310
}
foreach ($name in $pngs.Keys) { Save-MarkPng -App xnote -Size $pngs[$name] -Path (Join-Path $icons $name) }
Save-MarkIco -App xnote -Sizes 16, 24, 32, 48, 64, 256 -Path (Join-Path $icons 'icon.ico')
Save-MarkPng -App xnote -Size 128 -Path (Join-Path $PSScriptRoot 'src/logo.png')
Write-Host 'Xnote app and in-app icons regenerated from the family mark.'

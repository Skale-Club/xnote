# Publishes a new version: sets it in Cargo.toml, tauri.conf.json and
# package.json, commits, tags and pushes. GitHub Actions then builds, signs
# and releases the exe, and every installed copy offers the update.
#
#   ./scripts/release.ps1 0.2.0
param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = 'Stop'

if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Use a version like 0.2.0" }
Set-Location (Split-Path $PSScriptRoot)
if (git status --porcelain) { throw "Commit or discard your changes first." }
if (git tag --list "v$Version") { throw "v$Version already exists." }

$utf8 = New-Object System.Text.UTF8Encoding $false
function Set-Version($file, $pattern, $replacement) {
  $text = [IO.File]::ReadAllText($file)
  $new = ([regex]$pattern).Replace($text, $replacement, 1)
  if ($new -eq $text) { throw "No version found in $file" }
  [IO.File]::WriteAllText($file, $new, $utf8)
}
Set-Version 'src-tauri/Cargo.toml' '(?m)^version = "[^"]*"' "version = `"$Version`""
Set-Version 'src-tauri/tauri.conf.json' '"version": "[^"]*"' "`"version`": `"$Version`""
Set-Version 'package.json' '"version": "[^"]*"' "`"version`": `"$Version`""

git commit -am "Release v$Version"
git tag "v$Version"
git push
git push origin "v$Version"
Write-Host "Pushed v$Version. Follow the build on the repository's Actions tab."

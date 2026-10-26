# Draws icon-source.png (1024 px): an "X" over a sound wave, in the Skale
# blues on the near-black square shared by the Skale X apps.
# Then run:  npx tauri icon icon-source.png -o src-tauri/icons
param([string]$Out = (Join-Path $PSScriptRoot 'icon-source.png'))
Add-Type -AssemblyName System.Drawing
$size = 1024
$bmp = New-Object System.Drawing.Bitmap $size, $size
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = 'AntiAlias'
$g.Clear([System.Drawing.Color]::Transparent)

$r = 230; $m = 32; $w = $size - 2 * $m
$path = New-Object System.Drawing.Drawing2D.GraphicsPath
$path.AddArc($m, $m, $r, $r, 180, 90)
$path.AddArc($m + $w - $r, $m, $r, $r, 270, 90)
$path.AddArc($m + $w - $r, $m + $w - $r, $r, $r, 0, 90)
$path.AddArc($m, $m + $w - $r, $r, $r, 90, 90)
$path.CloseFigure()
$g.FillPath((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 12, 13, 18))), $path)

$soft = [System.Drawing.Color]::FromArgb(255, 143, 169, 238)
$brand = [System.Drawing.Color]::FromArgb(255, 81, 115, 214)

# Sound wave: rounded vertical bars across the middle, in soft blue
$bar = New-Object System.Drawing.Pen $soft, 46
$bar.StartCap = 'Round'; $bar.EndCap = 'Round'
$heights = 90, 190, 300, 150, 360, 220, 120, 260, 170, 80
for ($i = 0; $i -lt $heights.Count; $i++) {
  $x = 222 + $i * 64; $h = $heights[$i]
  $g.DrawLine($bar, $x, 512 - $h / 2, $x, 512 + $h / 2)
}

# "X" on top in CTA blue
$xpen = New-Object System.Drawing.Pen $brand, 120
$xpen.StartCap = 'Round'; $xpen.EndCap = 'Round'
$g.DrawLine($xpen, 300, 300, 724, 724)
$g.DrawLine($xpen, 724, 300, 300, 724)

$g.Dispose()
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
"saved $Out"

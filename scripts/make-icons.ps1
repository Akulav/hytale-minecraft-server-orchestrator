# Draws the Hyve icon (the same design as assets/logo.svg) with System.Drawing and writes:
#   assets/icon.png        256 x 256  (window icon, README)
#   assets/icon-1024.png   1024 x 1024 (macOS .icns is made from it in CI)
#   assets/icon.ico        16..256    (exe, setup, shortcuts)
# Run on Windows:  powershell -ExecutionPolicy Bypass -File scripts\make-icons.ps1
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
$root = Split-Path -Parent $PSScriptRoot
$assets = Join-Path $root "assets"

function Hex([float]$cx, [float]$cy, [float]$r) {
    $pts = New-Object 'System.Drawing.PointF[]' 6
    for ($i = 0; $i -lt 6; $i++) {
        $a = [Math]::PI / 180 * (60 * $i - 90)
        $pts[$i] = New-Object System.Drawing.PointF(($cx + $r * [Math]::Cos($a)), ($cy + $r * [Math]::Sin($a)))
    }
    return , $pts
}

function Draw([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)
    $s = $size / 64.0
    $rect = New-Object System.Drawing.RectangleF(0, 0, $size, $size)
    $outer = New-Object System.Drawing.Drawing2D.LinearGradientBrush($rect, [System.Drawing.Color]::FromArgb(255, 167, 139, 250), [System.Drawing.Color]::FromArgb(255, 34, 211, 238), 45.0)
    $blend = New-Object System.Drawing.Drawing2D.ColorBlend(3)
    $blend.Colors = @([System.Drawing.Color]::FromArgb(255, 167, 139, 250), [System.Drawing.Color]::FromArgb(255, 124, 58, 237), [System.Drawing.Color]::FromArgb(255, 34, 211, 238))
    $blend.Positions = @(0.0, 0.55, 1.0)
    $outer.InterpolationColors = $blend
    $g.FillPolygon($outer, (Hex (32 * $s) (32 * $s) (29.5 * $s)))
    $inner = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 13, 16, 32))
    $g.FillPolygon($inner, (Hex (32 * $s) (32 * $s) (23.5 * $s)))
    $hb = New-Object System.Drawing.Drawing2D.LinearGradientBrush($rect, [System.Drawing.Color]::FromArgb(255, 196, 181, 253), [System.Drawing.Color]::FromArgb(255, 103, 232, 249), 45.0)
    $pen = New-Object System.Drawing.Pen($hb, [float](5 * $s))
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $g.DrawLine($pen, [float](22 * $s), [float](20.5 * $s), [float](22 * $s), [float](43.5 * $s))
    $g.DrawLine($pen, [float](42 * $s), [float](20.5 * $s), [float](42 * $s), [float](43.5 * $s))
    $g.DrawLine($pen, [float](22 * $s), [float](32 * $s), [float](42 * $s), [float](32 * $s))
    $white = [System.Drawing.Brushes]::White
    foreach ($c in @(@(22, 20.5, 3.6), @(42, 43.5, 3.6), @(32, 32, 3.0))) {
        $r = $c[2] * $s
        $g.FillEllipse($white, [float]($c[0] * $s - $r), [float]($c[1] * $s - $r), [float](2 * $r), [float](2 * $r))
    }
    $g.Dispose()
    return $bmp
}

function PngBytes([int]$size) {
    $bmp = Draw $size
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    return , $ms.ToArray()
}

[IO.File]::WriteAllBytes((Join-Path $assets "icon.png"), (PngBytes 256))
[IO.File]::WriteAllBytes((Join-Path $assets "icon-1024.png"), (PngBytes 1024))

# .ico with PNG-compressed entries (Windows Vista and later)
$sizes = @(16, 24, 32, 48, 64, 128, 256)
$images = @()
foreach ($sz in $sizes) { $images += , (PngBytes $sz) }
$ms = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter($ms)
$w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $sz = $sizes[$i]; $len = $images[$i].Length
    $dim = if ($sz -ge 256) { 0 } else { $sz }
    $w.Write([Byte]$dim); $w.Write([Byte]$dim); $w.Write([Byte]0); $w.Write([Byte]0)
    $w.Write([UInt16]1); $w.Write([UInt16]32); $w.Write([UInt32]$len); $w.Write([UInt32]$offset)
    $offset += $len
}
foreach ($img in $images) { $w.Write($img) }
$w.Flush()
[IO.File]::WriteAllBytes((Join-Path $assets "icon.ico"), $ms.ToArray())
Write-Host "icons written to $assets"

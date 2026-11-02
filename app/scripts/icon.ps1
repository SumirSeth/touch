Add-Type -AssemblyName System.Drawing
$dir = Join-Path $PSScriptRoot '../assets'
New-Item -ItemType Directory -Force $dir | Out-Null
$b = New-Object System.Drawing.Bitmap 256,256
$g = [System.Drawing.Graphics]::FromImage($b)
$g.SmoothingMode = 'AntiAlias'
$g.Clear([System.Drawing.Color]::Transparent)
$brush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(0,103,192))
$g.FillEllipse($brush,8,8,240,240)
$pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White),8
$g.DrawRectangle($pen,51,83,154,111)
$g.DrawLine($pen,51,167,205,167)
foreach ($x in 92,128,164) {$g.FillEllipse([System.Drawing.Brushes]::White,$x-9,59,18,38)}
$b.Save((Join-Path $dir 'icon.png'),[System.Drawing.Imaging.ImageFormat]::Png)
$ms=New-Object System.IO.MemoryStream
$b.Save($ms,[System.Drawing.Imaging.ImageFormat]::Png)
$data=$ms.ToArray()
$out=[System.IO.File]::Create((Join-Path $dir 'icon.ico'))
$w=New-Object System.IO.BinaryWriter $out
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]1)
$w.Write([byte]0); $w.Write([byte]0); $w.Write([byte]0); $w.Write([byte]0)
$w.Write([uint16]1); $w.Write([uint16]32); $w.Write([uint32]$data.Length); $w.Write([uint32]22); $w.Write($data)
$w.Dispose(); $ms.Dispose(); $g.Dispose(); $b.Dispose(); $pen.Dispose(); $brush.Dispose()

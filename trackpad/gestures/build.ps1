# Compiles TrackpadGestures.exe with the .NET Framework compiler that ships with Windows.
$csc = "$env:WINDIR\Microsoft.NET\Framework64\v4.0.30319\csc.exe"
$out = Join-Path $PSScriptRoot 'TrackpadGestures.exe'
$sources = Get-ChildItem $PSScriptRoot -Filter *.cs | ForEach-Object FullName
& $csc /nologo /optimize /platform:x64 /target:winexe "/out:$out" /r:System.Windows.Forms.dll /r:System.Drawing.dll /r:System.Management.dll $sources
if ($LASTEXITCODE -eq 0) { "Built $out" }
